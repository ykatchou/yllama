use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::gguf;
use crate::manifest::{self, ModelEntry};
use crate::commands::models::hf_search;
use crate::commands::models::download;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};

/// How a GGUF already on disk is brought into `~/.yllama/models/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalMode {
    /// Duplicate the file into the models directory (on APFS/Btrfs this is a
    /// copy-on-write clone, so it is instant and costs no extra space).
    Copy,
    /// Point at the file where it lives, via a symlink.
    Link,
}

impl LocalMode {
    /// `--copy` / `--link` as passed on the CLI. Defaults to `Link`, which
    /// never duplicates tens of GB behind the user's back.
    pub fn from_flags(copy: bool, link: bool) -> Result<Option<Self>> {
        match (copy, link) {
            (true, true) => bail!("--copy and --link are mutually exclusive."),
            (true, false) => Ok(Some(LocalMode::Copy)),
            (false, true) => Ok(Some(LocalMode::Link)),
            (false, false) => Ok(None),
        }
    }
}

/// Expand a leading `~/`, which the shell leaves alone inside quotes.
fn expand_tilde(s: &str) -> PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => match dirs::home_dir() {
            Some(home) => home.join(rest),
            None => PathBuf::from(s),
        },
        None => PathBuf::from(s),
    }
}

/// Returns the path when `input` names a `.gguf` file that already exists on
/// disk, so it can be registered directly instead of being treated as a URL
/// or a HuggingFace search query.
fn local_gguf_path(input: &str) -> Option<PathBuf> {
    if input.starts_with("http") {
        return None;
    }
    let path = expand_tilde(input);
    let is_gguf = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    (is_gguf && path.is_file()).then_some(path)
}

/// Returns true for `owner/repo` shorthand (no scheme, exactly one `/`, no spaces).
fn is_model_id(s: &str) -> bool {
    !s.contains(' ') && s.split('/').count() == 2 && !s.starts_with("http")
}

/// Extract a `owner/repo` model ID from a plain HuggingFace repo page URL
/// like `https://huggingface.co/owner/repo` (no extra path segments after repo).
fn hf_repo_model_id(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("https://huggingface.co/")
        .or_else(|| url.strip_prefix("http://huggingface.co/"))?;
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() == 2 {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

pub async fn run(
    input: &str,
    name_override: Option<&str>,
    download_flag: bool,
    local_mode: Option<LocalMode>,
) -> Result<()> {
    match (local_gguf_path(input), local_mode) {
        (Some(path), mode) => {
            return add_local(&path, name_override, mode.unwrap_or(LocalMode::Link)).await
        }
        (None, Some(_)) => bail!(
            "--copy / --link expect a path to an existing .gguf file (got: {input})"
        ),
        (None, None) => {}
    }

    let mut download_url = input.to_string();

    if let Some(model_id) = hf_repo_model_id(input) {
        // Plain HF repo URL — pick a GGUF file interactively.
        println!("Fetching GGUF files from '{}'…", model_id);
        download_url = hf_search::pick_gguf_url(&model_id).await?;
        println!("Selected: {}", download_url);
    } else if is_model_id(input) {
        // owner/repo shorthand — pick a GGUF file interactively.
        println!("Fetching GGUF files from '{}'…", input);
        download_url = hf_search::pick_gguf_url(input).await?;
        println!("Selected: {}", download_url);
    } else if !input.starts_with("http") {
        // Free-text search query.
        println!("Searching Hugging Face for '{}'...", input);
        let models = hf_search::search_models(input).await?;

        if models.is_empty() {
            bail!("No models found for query: {}", input);
        }

        let model_options: Vec<String> = models.iter().map(|m| m.id.clone()).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a model")
            .items(&model_options)
            .default(0)
            .interact()?;

        let selected_model = &models[selection];
        println!("Selected model: {}", selected_model.id);

        download_url = hf_search::pick_gguf_url(&selected_model.id).await?;
        println!("Selected: {}", download_url);
    } else {
        // Direct file URL — /blob/ links are browser-facing, convert to /resolve/.
        download_url = download_url.replace("/blob/", "/resolve/");
    }

    // Derive filename from the last path segment (before any query string)
    let filename = download_url
        .split('?')
        .next()
        .unwrap_or(&download_url)
        .split('/')
        .last()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Cannot derive filename from URL: {download_url}"))?
        .to_string();

    if !filename.ends_with(".gguf") {
        bail!("URL does not point to a .gguf file (got: {filename})");
    }

    let name = match name_override {
        Some(n) => n.to_string(),
        None => filename.trim_end_matches(".gguf").to_string(),
    };

    let mut entries = manifest::load()?;

    if let Some(entry) = manifest::find(&entries, &name) {
        // Model already registered — download if not already done.
        if entry.downloaded {
            println!("Model '{name}' already registered and downloaded.");
        } else if download_flag {
            println!("Model '{name}' already registered. Downloading now…\n");
            download::run(&name).await?;
            println!("\nModel '{name}' is ready. Run `yllama serve` to start inference.");
        } else {
            println!("Model '{name}' already registered. Run `yllama models download {name}` to download it.");
        }
        return Ok(());
    }

    println!("Checking for MTP / speculative decoding support...");
    let mut mtp_builtin = gguf::has_builtin_mtp(&download_url).await.unwrap_or(false);
    let mut draft: Option<(String, String, String)> = None; // (url, filename, spec_type)
    let repo_id = hf_search::repo_id_from_hf_url(&download_url);

    if mtp_builtin {
        println!("MTP: built-in heads detected — will enable automatically at serve time.");
    } else if let Some(repo_id) = &repo_id {
        if let Some(variant_url) = hf_search::find_mtp_variant_url(repo_id, &filename)
            .await
            .unwrap_or(None)
        {
            println!(
                "MTP: an MTP-enabled variant of this exact quant is available \
                 (self-speculative decoding, same size, no extra download)."
            );
            let use_variant = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Download the MTP-enabled variant instead?")
                .default(true)
                .interact()?;
            if use_variant {
                download_url = variant_url;
                mtp_builtin = true;
            }
        }
    }

    if !mtp_builtin {
        if let Some(repo_id) = &repo_id {
            draft = hf_search::find_drafter_in_repo(repo_id, &filename)
                .await
                .unwrap_or(None);
        }
        match &draft {
            Some((_, draft_filename, spec_type)) => {
                println!(
                    "Speculative decoding: found drafter '{draft_filename}' ({spec_type}) \
                     bundled with this model — will download alongside the main file."
                );
            }
            None => println!("MTP / speculative decoding: not available for this model."),
        }
    }

    let (draft_url, draft_filename, draft_spec_type) = draft.map_or(
        (None, None, None),
        |(u, f, t)| (Some(u), Some(f), Some(t)),
    );

    entries.push(ModelEntry {
        name: name.clone(),
        hf_url: download_url.clone(),
        filename: filename.clone(),
        downloaded: false,
        size_bytes: None,
        extra_args: vec![],
        default_model: None,
        mtp_checked: true,
        mtp_builtin,
        draft_url,
        draft_filename,
        draft_spec_type,
        draft_downloaded: false,
        local_source: None,
    });
    manifest::save(&entries)?;
    println!("Added model '{name}'.");

    println!("Downloading '{name}' now…\n");
    download::run(&name).await?;
    println!("\nModel '{name}' is ready. Run `yllama serve` to start inference.");

    Ok(())
}

/// Register a GGUF that is already on disk. The file is either copied or
/// symlinked into `models_dir()` so everything downstream (serve, litellm,
/// delete) keeps working off `manifest::model_path`; the user's original file
/// is never moved or removed.
async fn add_local(src: &Path, name_override: Option<&str>, mode: LocalMode) -> Result<()> {
    let src = src
        .canonicalize()
        .with_context(|| format!("resolving {}", src.display()))?;

    let filename = src
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("Cannot derive a filename from {}", src.display()))?;

    let name = match name_override {
        Some(n) => n.to_string(),
        None => filename.trim_end_matches(".gguf").to_string(),
    };

    let mut entries = manifest::load()?;
    if manifest::find(&entries, &name).is_some() {
        bail!(
            "Model '{name}' is already registered. Pick another name with --name, \
             or run `yllama models delete {name}` first."
        );
    }

    // Entries are keyed by filename inside models_dir(), so two of them
    // sharing one file would let `models delete` pull it out from under the
    // other.
    if let Some(other) = entries.iter().find(|e| e.filename == filename) {
        bail!(
            "'{filename}' is already registered as '{}'. Delete that entry first, \
             or rename the file to register it a second time.",
            other.name
        );
    }

    let dest = manifest::models_dir().join(&filename);
    std::fs::create_dir_all(manifest::models_dir())?;

    let size = std::fs::metadata(&src)?.len();

    if dest == src {
        // Already sitting in the models directory — just register it in place.
        println!(
            "'{filename}' is already in {} — registering in place.",
            manifest::models_dir().display()
        );
    } else if dest.exists() {
        bail!(
            "{} already exists (registered under a different name?). \
             Rename the file or use --name with a model whose filename differs.",
            dest.display()
        );
    } else {
        match mode {
            LocalMode::Copy => copy_into_models_dir(&src, &dest, size).await?,
            LocalMode::Link => {
                link_into_models_dir(&src, &dest)?;
                println!("Linked {} -> {}", dest.display(), src.display());
            }
        }
    }

    println!("Checking for MTP / speculative decoding support...");
    let mtp_builtin = gguf::has_builtin_mtp_local(&dest).unwrap_or(false);
    if mtp_builtin {
        println!("MTP: built-in heads detected — will enable automatically at serve time.");
    } else {
        println!("MTP / speculative decoding: not available for this model.");
    }

    entries.push(ModelEntry {
        name: name.clone(),
        hf_url: String::new(),
        filename,
        downloaded: true,
        size_bytes: Some(size),
        extra_args: vec![],
        default_model: None,
        mtp_checked: true,
        mtp_builtin,
        draft_url: None,
        draft_filename: None,
        draft_spec_type: None,
        draft_downloaded: false,
        local_source: Some(src.display().to_string()),
    });
    manifest::save(&entries)?;

    println!(
        "\nAdded model '{name}' ({}). Run `yllama serve {name}` to start inference.",
        manifest::format_bytes(size)
    );
    Ok(())
}

async fn copy_into_models_dir(src: &Path, dest: &Path, size: u64) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_message(format!(
        "Copying {} into {}",
        manifest::format_bytes(size),
        manifest::models_dir().display()
    ));

    let (from, to) = (src.to_path_buf(), dest.to_path_buf());
    let result = tokio::task::spawn_blocking(move || std::fs::copy(&from, &to)).await?;
    // A partial copy left behind by a failure would look like a valid model.
    if let Err(e) = result {
        pb.finish_and_clear();
        let _ = std::fs::remove_file(dest);
        return Err(anyhow::Error::new(e)
            .context(format!("copying {} to {}", src.display(), dest.display())));
    }

    pb.finish_with_message(format!("Copied to {}", dest.display()));
    Ok(())
}

#[cfg(unix)]
fn link_into_models_dir(src: &Path, dest: &Path) -> Result<()> {
    std::os::unix::fs::symlink(src, dest)
        .with_context(|| format!("symlinking {} -> {}", dest.display(), src.display()))
}

#[cfg(windows)]
fn link_into_models_dir(src: &Path, dest: &Path) -> Result<()> {
    // Symlinks need Developer Mode or an elevated shell on Windows; a hard
    // link works unprivileged as long as both paths are on the same volume.
    std::os::windows::fs::symlink_file(src, dest)
        .or_else(|_| std::fs::hard_link(src, dest))
        .with_context(|| {
            format!(
                "linking {} -> {} (enable Developer Mode, or use --copy if the \
                 model lives on another drive)",
                dest.display(),
                src.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{hf_repo_model_id, is_model_id, local_gguf_path, LocalMode};

    fn blob_to_resolve(url: &str) -> String {
        url.replace("/blob/", "/resolve/")
    }

    fn filename_from_url(url: &str) -> &str {
        url.split('?')
            .next()
            .unwrap_or(url)
            .split('/')
            .last()
            .unwrap_or("")
    }

    #[test]
    fn test_blob_converted_to_resolve() {
        let url = "https://huggingface.co/owner/repo/blob/main/model.gguf";
        assert_eq!(
            blob_to_resolve(url),
            "https://huggingface.co/owner/repo/resolve/main/model.gguf"
        );
    }

    #[test]
    fn test_resolve_url_unchanged() {
        let url = "https://huggingface.co/owner/repo/resolve/main/model.gguf";
        assert_eq!(blob_to_resolve(url), url);
    }

    #[test]
    fn test_filename_extracted() {
        let url = "https://huggingface.co/owner/repo/resolve/main/gemma-Q4_K_M.gguf";
        assert_eq!(filename_from_url(url), "gemma-Q4_K_M.gguf");
    }

    #[test]
    fn test_filename_strips_query_string() {
        let url = "https://example.com/model.gguf?download=true";
        assert_eq!(filename_from_url(url), "model.gguf");
    }

    #[test]
    fn test_name_derived_from_filename() {
        let filename = "gemma-Q4_K_M.gguf";
        assert_eq!(filename.trim_end_matches(".gguf"), "gemma-Q4_K_M");
    }

    #[test]
    fn test_is_model_id() {
        assert!(is_model_id("unsloth/Qwen3.6-35B-A3B-GGUF"));
        assert!(is_model_id("Qwen/Qwen3.6-35B-A3B"));
        assert!(!is_model_id("https://huggingface.co/owner/repo"));
        assert!(!is_model_id("some search query"));
        assert!(!is_model_id("no-slash"));
    }

    #[test]
    fn test_local_gguf_path_detects_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("my-model.gguf");
        std::fs::write(&file, b"GGUF").unwrap();
        assert_eq!(local_gguf_path(file.to_str().unwrap()), Some(file));
    }

    #[test]
    fn test_local_gguf_path_ignores_urls_and_queries() {
        // A URL is never a local path, even if one happened to exist.
        assert_eq!(
            local_gguf_path("https://huggingface.co/owner/repo/resolve/main/m.gguf"),
            None
        );
        // Search queries and owner/repo shorthand don't exist on disk.
        assert_eq!(local_gguf_path("gemma"), None);
        assert_eq!(local_gguf_path("unsloth/Qwen3.6-35B-A3B-GGUF"), None);
    }

    #[test]
    fn test_local_gguf_path_rejects_non_gguf_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("notes.txt");
        std::fs::write(&other, b"x").unwrap();
        assert_eq!(local_gguf_path(other.to_str().unwrap()), None);
        assert_eq!(local_gguf_path(dir.path().to_str().unwrap()), None);
    }

    #[test]
    fn test_local_mode_from_flags() {
        assert_eq!(LocalMode::from_flags(false, false).unwrap(), None);
        assert_eq!(
            LocalMode::from_flags(true, false).unwrap(),
            Some(LocalMode::Copy)
        );
        assert_eq!(
            LocalMode::from_flags(false, true).unwrap(),
            Some(LocalMode::Link)
        );
        assert!(LocalMode::from_flags(true, true).is_err());
    }

    #[test]
    fn test_hf_repo_model_id() {
        assert_eq!(
            hf_repo_model_id("https://huggingface.co/unsloth/Qwen3.6-35B-A3B-GGUF"),
            Some("unsloth/Qwen3.6-35B-A3B-GGUF".to_string())
        );
        // URL with extra path segments is NOT a plain repo URL
        assert_eq!(
            hf_repo_model_id("https://huggingface.co/owner/repo/resolve/main/file.gguf"),
            None
        );
        assert_eq!(hf_repo_model_id("not-a-url"), None);
    }
}

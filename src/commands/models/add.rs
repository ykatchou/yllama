use anyhow::{bail, Result};

use crate::gguf;
use crate::manifest::{self, ModelEntry};
use crate::commands::models::hf_search;
use crate::commands::models::download;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};

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

pub async fn run(input: &str, name_override: Option<&str>, download_flag: bool) -> Result<()> {
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
    });
    manifest::save(&entries)?;
    println!("Added model '{name}'.");

    println!("Downloading '{name}' now…\n");
    download::run(&name).await?;
    println!("\nModel '{name}' is ready. Run `yllama serve` to start inference.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_model_id, hf_repo_model_id};

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

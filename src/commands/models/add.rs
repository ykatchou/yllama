use anyhow::{bail, Result};

use crate::manifest::{self, ModelEntry};
use crate::commands::models::hf_search;
use dialoguer::{theme::ColorfulTheme, Select};

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

pub async fn run(input: &str, name_override: Option<&str>) -> Result<()> {
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
    if manifest::find(&entries, &name).is_some() {
        bail!(
            "Model '{name}' already in manifest. \
             Use `yllama models list` to see all models."
        );
    }

    entries.push(ModelEntry {
        name: name.clone(),
        hf_url: download_url,
        filename,
        downloaded: false,
        size_bytes: None,
        extra_args: vec![],
        default_model: None,
    });
    manifest::save(&entries)?;
    println!("Added model '{name}'.");
    println!("Run `yllama models download {name}` to download it.");
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

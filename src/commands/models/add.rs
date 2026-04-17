use anyhow::{bail, Result};

use crate::manifest::{self, ModelEntry};
use crate::commands::models::hf_search;
use dialoguer::{theme::ColorfulTheme, Select};

pub async fn run(input: &str, name_override: Option<&str>) -> Result<()> {
    let mut download_url = input.to_string();

    // Check if it's a URL or a search query
    if !input.starts_with("http") {
        println!("Searching Hugging Face for '{}'...", input);
        let models = hf_search::search_models(input).await?;

        if models.is_empty() {
            bail!("No models found for query: {}", input);
        }

        let model_options: Vec<String> = models
            .iter()
            .map(|m| format!("{} ({})", m.model_name, m.id))
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a model")
            .items(&model_options)
            .default(0)
            .interact()?;

        let selected_model = &models[selection];
        println!("Selected model: {}", selected_model.id);

        download_url = hf_search::get_gguf_url(&selected_model.id).await?;
        println!("Found GGUF URL: {}", download_url);
    } else {
        // /blob/ links are browser-facing — convert to /resolve/ for direct downloads
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
    });
    manifest::save(&entries)?;
    println!("Added model '{name}'.");
    println!("Run `yllama models download {name}` to download it.");
    Ok(())
}

#[cfg(test)]
mod tests {
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
}

use anyhow::{bail, Result};
use serde::Deserialize;
use reqwest::Client;
use dialoguer::{theme::ColorfulTheme, Select};

#[derive(Debug, Deserialize)]
pub struct HfModel {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct HfFile {
    pub path: String,
}

pub async fn search_models(query: &str) -> Result<Vec<HfModel>> {
    let client = Client::new();
    let url = format!(
        "https://huggingface.co/api/models?search={}&sort=downloads&direction=-1",
        query
    );
    let response = client.get(url).send().await?;
    // The HF API returns a JSON array directly (not a wrapped object).
    let models: Vec<HfModel> = response.json().await?;
    Ok(models)
}

/// Fetch all GGUF file paths in a HF repo. Returns empty vec if repo not found.
pub async fn list_gguf_files(model_id: &str) -> Result<Vec<String>> {
    let client = Client::new();
    let url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Ok(vec![]);
    }
    let files: Vec<HfFile> = response.json().await?;
    let gguf_paths: Vec<String> = files
        .into_iter()
        .filter(|f| f.path.ends_with(".gguf"))
        .map(|f| f.path)
        .collect();
    Ok(gguf_paths)
}

/// Return a download URL for the chosen GGUF file in `model_id`.
/// If the repo has no GGUF files, also tries `{model_id}-GGUF` and a HF search
/// for GGUF variants and asks the user to confirm a different repo.
pub async fn pick_gguf_url(model_id: &str) -> Result<String> {
    let mut gguf_files = list_gguf_files(model_id).await?;

    // If the original repo has no GGUFs, look for a well-known GGUF variant.
    let mut resolved_id = model_id.to_string();
    if gguf_files.is_empty() {
        let gguf_variant = format!("{}-GGUF", model_id);
        let variant_files = list_gguf_files(&gguf_variant).await?;
        if !variant_files.is_empty() {
            println!(
                "No GGUF files in '{}'. Found GGUF repo: '{}'.",
                model_id, gguf_variant
            );
            resolved_id = gguf_variant;
            gguf_files = variant_files;
        } else {
            // Fall back to a keyword search on HF
            println!("No GGUF files found. Searching Hugging Face for GGUF variants…");
            let model_name = model_id.split('/').last().unwrap_or(model_id);
            let candidates = search_models(&format!("{} GGUF", model_name)).await?;
            let gguf_candidates: Vec<HfModel> = candidates
                .into_iter()
                .filter(|m| m.id.to_lowercase().contains("gguf"))
                .collect();

            if gguf_candidates.is_empty() {
                bail!("No GGUF variant found for '{}'", model_id);
            }

            let options: Vec<String> = gguf_candidates.iter().map(|m| m.id.clone()).collect();
            let sel = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select a GGUF repo")
                .items(&options)
                .default(0)
                .interact()?;

            resolved_id = gguf_candidates[sel].id.clone();
            gguf_files = list_gguf_files(&resolved_id).await?;
            if gguf_files.is_empty() {
                bail!("No GGUF files found in '{}'", resolved_id);
            }
        }
    }

    // Let the user pick the quantisation when more than one file is available.
    let path = if gguf_files.len() == 1 {
        gguf_files.into_iter().next().unwrap()
    } else {
        let sel = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Select a GGUF file from '{}'", resolved_id))
            .items(&gguf_files)
            .default(0)
            .interact()?;
        gguf_files[sel].clone()
    };

    Ok(format!(
        "https://huggingface.co/{}/resolve/main/{}",
        resolved_id, path
    ))
}

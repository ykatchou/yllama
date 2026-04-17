use anyhow::{bail, Result};
use serde::Deserialize;
use reqwest::Client;

#[derive(Debug, Deserialize)]
pub struct HfModel {
    pub id: String,
    pub model_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HfSearchResponse {
    pub results: Vec<HfModel>,
}

#[derive(Debug, Deserialize)]
struct HfFile {
    pub r#href: String,
    pub lfs: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HfRepoFilesResponse {
    pub files: Vec<HfFile>,
}

pub async fn search_models(query: &str) -> Result<Vec<HfModel>> {
    let client = Client::new();
    let url = format!("https://huggingface.co/api/models?search={}&sort=downloads&direction=-1", query);
    
    let response = client.get(url).send().await?;
    let search_data: HfSearchResponse = response.json().await?;
    
    Ok(search_data.results)
}

pub async fn get_gguf_url(model_id: &str) -> Result<String> {
    let client = Client::new();
    let url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);
    
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        bail!("Failed to fetch file list for model: {}", model_id);
    }
    
    let files_data: HfRepoFilesResponse = response.json().await?;
    
    for file in files_data.files {
        if file.r#href.ends_with(".gguf") {
            // Convert tree URL to resolve URL
            // tree/main/path/to/file.gguf -> resolve/main/path/to/file.gguf
            let resolve_url = file.r#href.replace("/tree/", "/resolve/");
            return Ok(resolve_url);
        }
    }
    
    bail!("No .gguf file found in model repository: {}", model_id);
}

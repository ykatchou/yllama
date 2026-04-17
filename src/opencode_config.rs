use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub temperature: bool,
    pub knowledge: Option<String>,
    pub release_date: Option<String>,
    pub last_updated: Option<String>,
    pub modalities: Option<HashMap<String, Vec<String>>>,
    #[serde(default)]
    pub open_weights: bool,
    pub cost: Option<HashMap<String, f64>>,
    pub limit: Option<HashMap<String, u64>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub npm: Option<String>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub models: HashMap<String, ModelInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OpencodeModelsConfig {
    #[serde(flatten)]
    pub providers: HashMap<String, Provider>,
}

pub fn opencode_config_path() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("opencode"))
        .unwrap_or_else(|| PathBuf::from("~/.config/opencode"))
}

pub async fn sync_with_models(base_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let models_url = format!("{}/v1/models", base_url);
    
    let response = client.get(&models_url).send().await?;
    if !response.status().is_success() {
        bail!("Failed to fetch models from llama-server for opencode sync");
    }
    
    let body: serde_json::Value = response.json().await?;
    let models = body["data"].as_array().cloned().unwrap_or_default();
    
    let config_path = opencode_config_path().join("models.json");
    if !config_path.exists() {
        // If it doesn't exist, we'll create it with just the local provider
        let config = OpencodeModelsConfig {
            providers: HashMap::new(),
        };
        save_config(&config_path, &config)?;
    }

    let config_content = fs::read_to_string(&config_path)?;
    let mut config: OpencodeModelsConfig = serde_json::from_str(&config_content)?;

    let local_provider = config.providers.entry("local".to_string()).or_insert(Provider {
        id: "local".to_string(),
        name: "Local Models".to_string(),
        env: vec![],
        npm: None,
        api: None,
        doc: None,
        models: HashMap::new(),
    });

    for model in models {
        if let Some(id) = model["id"].as_str() {
            if !local_provider.models.contains_key(id) {
                let model_info = ModelInfo {
                    id: id.to_string(),
                    name: model["id"].as_str().unwrap_or(id).to_string(),
                    family: model["family"].as_str().map(|s| s.to_string()),
                    attachment: false,
                    reasoning: false,
                    tool_call: true,
                    temperature: true,
                    knowledge: None,
                    release_date: None,
                    last_updated: None,
                    modalities: None,
                    open_weights: true,
                    cost: None,
                    limit: None,
                };
                local_provider.models.insert(id.to_string(), model_info);
            }
        }
    }

    save_config(&config_path, &config)?;
    Ok(())
}

fn save_config(path: &PathBuf, config: &OpencodeModelsConfig) -> Result<()> {
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

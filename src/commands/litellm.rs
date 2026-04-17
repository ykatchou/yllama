use anyhow::Result;
use std::path::PathBuf;

use crate::{config::Config, vibe_config};

pub async fn run(cfg: &Config, output: Option<PathBuf>) -> Result<()> {
    let base_url = format!("http://{}:{}", cfg.host, cfg.port);
    println!("Querying llama.cpp at {base_url}/v1/models ...");
    let models = vibe_config::fetch_models(&base_url).await?;
    if models.is_empty() {
        anyhow::bail!("No models found on the server. Is llama-server running?");
    }

    let dest = output.unwrap_or_else(|| PathBuf::from("litellm_config.yaml"));
    let content = build_litellm_config(&base_url, &models);
    std::fs::write(&dest, &content)?;
    println!("Written {} model(s) to {}", models.len(), dest.display());
    println!();
    println!("Start the proxy with:");
    println!("  litellm --config {}", dest.display());
    Ok(())
}

fn build_litellm_config(base_url: &str, models: &[serde_json::Value]) -> String {
    let mut lines = vec![
        "model_list:".to_string(),
    ];

    for m in models {
        let id = m["id"].as_str().unwrap_or("unknown");
        lines.push(format!("  - model_name: {id}"));
        lines.push("    litellm_params:".to_string());
        lines.push(format!("      model: openai/{id}"));
        lines.push(format!("      api_base: {base_url}/v1"));
        lines.push("      api_key: \"none\"".to_string());
    }

    lines.push(String::new());
    lines.push("litellm_settings:".to_string());
    lines.push("  drop_params: true".to_string());
    lines.push(String::new());

    lines.join("\n")
}

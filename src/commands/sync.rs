use anyhow::Result;

use crate::{config::Config, vibe_config};

pub async fn run(cfg: &Config) -> Result<()> {
    let base_url = format!("http://{}:{}", cfg.host, cfg.port);
    println!("Querying llama.cpp at {base_url}/v1/models ...");
    let models = vibe_config::fetch_models(&base_url).await?;
    if models.is_empty() {
        anyhow::bail!("No models found on the server. Is llama-server running?");
    }
    let ids: Vec<&str> = models
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    println!("Found {} model(s): {}", models.len(), ids.join(", "));
    vibe_config::sync_with_models(&base_url, &models)?;
    println!("Updated {}", vibe_config::vibe_config_path().display());
    Ok(())
}

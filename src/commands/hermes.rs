use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::{commands::select_model, config::Config, deps, llamacpp, manifest, vibe_config};

pub async fn run(cfg: &Config, folder: Option<PathBuf>, extra_args: &[String]) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // Check that hermes binary is available
    deps::check_binary("hermes").with_context(|| "Hermes binary not found on PATH")?;

    // Ensure server is running, auto-start if needed
    let model_name = if !llamacpp::is_running(cfg).await {
        println!("llama-server is not running — starting it...");
        let entries = manifest::load()?;
        let (entry, _) = select_model::select_model(&entries)?;
        println!("Auto-starting llama-server with model '{}'...", entry.name);
        let model_path = manifest::model_path(&entry);
        let mut child = llamacpp::spawn_daemon(cfg, &model_path, &entry.extra_args)?;
        llamacpp::write_pid(child.id())?;
        print!("Waiting for server to be ready...");
        llamacpp::wait_for_ready(cfg, 60, &mut child).await?;
        println!(" done.");
        entry.name.clone()
    } else {
        // Server is already running — query live model from server
        let base_url = format!("http://{}:{}", cfg.host, cfg.port);
        let models = vibe_config::fetch_models(&base_url).await?;
        let entries = manifest::load()?;
        let ids: Vec<&str> = models.iter().filter_map(|m| m["id"].as_str()).collect();
        manifest::resolve_running_model_name(&ids, &entries)
            .context("no model found on running server")?
    };

    let base_url = format!("http://{}:{}", cfg.host, cfg.port);

    // Sync vibe config so the model is discoverable
    println!("Syncing configurations...");
    crate::commands::sync::run(cfg).await?;

    // Update hermes config.yaml to point to local llama.cpp
    let hermes_dir = dirs::home_dir()
        .expect("no home dir")
        .join(".hermes")
        .join("config.yaml");
    println!("Configuring Hermes to use local llama.cpp at {base_url}/v1 ...");

    let hermes_config = if hermes_dir.exists() {
        std::fs::read_to_string(&hermes_dir).with_context(|| format!("reading {}", hermes_dir.display()))?
    } else {
        String::new()
    };

    // Use a simple text-based update approach: read current YAML, update model section
    // Hermes uses a simple structure: model.default, model.provider, model.base_url
    let new_config = update_hermes_config(&hermes_config, &base_url, &model_name);
    std::fs::write(&hermes_dir, &new_config)
        .with_context(|| format!("writing {}", hermes_dir.display()))?;

    // Launch hermes chat with env vars pointing to local llama.cpp
    println!("Launching Hermes in {} with model '{}'", folder.display(), model_name);
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("hermes")
        .current_dir(&folder)
        .arg("chat")
        .args(extra_args)
        .env("HERMES_CONFIG", hermes_dir.to_str().unwrap())
        .exec();
    anyhow::bail!("Failed to exec hermes: {err}");
}

/// Update the hermes config.yaml to point to the local llama.cpp server.
/// Preserves existing config structure while updating model settings.
fn update_hermes_config(current: &str, base_url: &str, model_name: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_model_section = false;

    for line in current.lines() {
        // Detect model: section (top-level)
        if line.starts_with("model:") && !line.contains("base_url") && !line.contains("default") && !line.contains("provider") {
            in_model_section = true;
            lines.push(line.to_string());
            continue;
        }

        if in_model_section {
            // Update base_url
            if line.starts_with("  base_url:") {
                lines.push(format!("  base_url: {}", base_url));
            }
            // Update default model
            else if line.starts_with("  default:") {
                lines.push(format!("  default: {}", model_name));
            }
            // Update provider
            else if line.starts_with("  provider:") {
                lines.push("  provider: custom".to_string());
            }
            // Check if we've left the model section (next top-level key, not indented)
            else if !line.starts_with(' ') {
                in_model_section = false;
                lines.push(line.to_string());
            }
            // Skip leading whitespace on blank lines within model section
            else if line.trim().is_empty() {
                in_model_section = false;
                lines.push(line.to_string());
            } else {
                lines.push(line.to_string());
            }
        } else {
            lines.push(line.to_string());
        }
    }

    // If model section was never found, add it
    if !in_model_section && !current.contains("model:") {
        lines.push(String::new());
        lines.push("model:".to_string());
        lines.push(format!("  default: {}", model_name));
        lines.push("  provider: custom".to_string());
        lines.push(format!("  base_url: {}/v1", base_url));
        lines.push(String::new());
    }

    lines.join("\n") + "\n"
}

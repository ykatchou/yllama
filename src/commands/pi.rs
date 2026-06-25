use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::{commands::select_model, config::Config, deps, llamacpp, manifest, vibe_config};

pub async fn run(cfg: &Config, folder: Option<PathBuf>, extra_args: &[String]) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // Check that pi binary is available
    deps::check_binary("pi").with_context(|| "pi binary not found on PATH")?;

    // Ensure server is running, auto-start if needed
    let model_name = if !llamacpp::is_running(cfg).await {
        println!("llama-server is not running — starting it...");
        let entries = manifest::load()?;
        let (entry, _) = select_model::select_model(&entries)?;
        println!("Auto-starting llama-server with model '{}'...", entry.name);
        let model_path = manifest::model_path(&entry);
        let pid = llamacpp::spawn_daemon(cfg, &model_path, &entry.extra_args)?;
        llamacpp::write_pid(pid)?;
        print!("Waiting for server to be ready...");
        llamacpp::wait_for_ready(cfg, 60).await?;
        println!(" done.");
        entry.name
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

    // Create a temporary PI_CODING_AGENT_DIR with a models.json
    // that overrides the openai provider's baseUrl to point to llama.cpp
    let tmp_dir = std::env::temp_dir().join("yllama-pi-config");
    std::fs::create_dir_all(tmp_dir.join("agent")).ok();
    let models_json = serde_json::json!({
        "providers": {
            "openai": {
                "baseUrl": format!("{}/v1", base_url),
                "apiKey": "sk-local"
            }
        }
    });
    let models_json_path = tmp_dir.join("agent").join("models.json");
    std::fs::write(
        &models_json_path,
        serde_json::to_string_pretty(&models_json).unwrap(),
    )?;

    // Launch pi with env vars pointing to local llama.cpp
    println!(
        "Launching pi in {} with model '{}'",
        folder.display(),
        model_name
    );
    use std::io::{BufRead, BufReader};
    let mut child = std::process::Command::new("pi")
        .current_dir(&folder)
        .arg("--provider")
        .arg("openai")
        .arg("--model")
        .arg(&model_name)
        .args(extra_args)
        .env("PI_CODING_AGENT_DIR", tmp_dir.join("agent").to_str().unwrap())
        .env("ANTHROPIC_AUTH_TOKEN", "local")
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Filter out the "model not found" warning from pi's stderr
    let stderr = BufReader::new(child.stderr.take().unwrap());
    for line in stderr.lines() {
        let line = line?;
        if !line.contains("not found for provider")
            && !line.contains("Using custom model id")
        {
            println!("{}", line);
        }
    }

    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("pi exited with status: {}", status);
    } else {
        Ok(())
    }
}

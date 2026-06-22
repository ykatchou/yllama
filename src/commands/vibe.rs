use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::{commands::select_model, config::Config, deps, llamacpp, manifest, vibe_config};

pub async fn run(cfg: &Config, folder: Option<PathBuf>, extra_args: &[String]) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // Check that vibe binary is available
    deps::check_binary("vibe").with_context(|| "vibe binary not found on PATH")?;

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

    // Sync all configurations
    println!("Syncing configurations...");
    crate::commands::sync::run(cfg).await?;

    // Launch vibe with env vars pointing to local llama.cpp
    println!("Launching Vibe in {} with model '{}'", folder.display(), model_name);
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("vibe")
        .current_dir(&folder)
        .env("LLAMA_HOST", &base_url)
        .args(extra_args)
        .exec();
    anyhow::bail!("Failed to exec vibe: {err}");
}

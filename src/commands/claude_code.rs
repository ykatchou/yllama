use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::{commands::select_model, config::Config, deps, llamacpp, manifest};

pub async fn run(cfg: &Config, folder: Option<PathBuf>, extra_args: &[String]) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // Check that claude binary is available
    deps::check_binary("claude")
        .or_else(|_| deps::check_binary("claude-code"))
        .with_context(|| "Claude Code binary not found on PATH")?;

    // Ensure server is running, auto-start if needed
    if !llamacpp::is_running(cfg).await {
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
    }

    let base_url = format!("http://{}:{}", cfg.host, cfg.port);

    // Get the model name for the --model flag
    let entries = manifest::load()?;
    let model_name = select_model::select_model(&entries)?.0.name;

    // Sync vibe config so Claude Code can discover the model
    println!("Syncing configurations...");
    crate::commands::sync::run(cfg).await?;

    // Launch claude with env vars pointing to local llama.cpp
    println!("Launching Claude Code in {} with model '{}'", folder.display(), model_name);
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("claude")
        .current_dir(&folder)
        .arg("--model")
        .arg(&model_name)
        .args(extra_args)
        .env("ANTHROPIC_BASE_URL", &base_url)
        .env("ANTHROPIC_AUTH_TOKEN", "local")
        .exec();
    anyhow::bail!("Failed to exec claude: {err}");
}

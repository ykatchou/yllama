use anyhow::{bail, Result};
use std::path::PathBuf;
use std::process::Command;
use crate::config::Config;
use crate::commands::sync::run as sync_run;
use crate::{llamacpp, manifest};

pub async fn run(cfg: &Config, folder: Option<PathBuf>) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // 1. Ensure llama-server is running
    if !llamacpp::is_running(cfg).await {
        println!("llama-server is not running — starting it...");
        let entries = manifest::load()?;
        let entry = entries
            .iter()
            .find(|e| e.downloaded && manifest::model_path(e).exists())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No downloaded models found. \
                     Run `yllama models download <name>` first."
                )
            })?
            .clone();
        let model_path = manifest::model_path(&entry);
        println!("Using model '{}'", entry.name);
        let pid = llamacpp::spawn_daemon(cfg, &model_path, &entry.extra_args)?;
        llamacpp::write_pid(pid)?;
        print!("Waiting for server to be ready...");
        llamacpp::wait_for_ready(cfg, 60).await?;
        println!(" done.");
    }

    // 2. Sync all configurations (vibe and opencode)
    println!("Syncing configurations...");
    sync_run(cfg).await?;

    // 3. Launch opencode
    let mut cmd = Command::new("opencode");
    cmd.arg(&folder);

    println!("Launching opencode in {}...", folder.display());
    let mut child = cmd.spawn()?;
    
    let status = child.wait()?;
    if !status.success() {
        bail!("opencode exited with error");
    }

    Ok(())
}

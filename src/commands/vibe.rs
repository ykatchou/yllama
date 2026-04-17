use anyhow::Result;
use std::path::PathBuf;

use crate::{config::Config, llamacpp, manifest};

pub async fn run(cfg: &Config, folder: Option<PathBuf>, extra_args: &[String]) -> Result<()> {
    let folder = folder.unwrap_or_else(|| std::env::current_dir().expect("no cwd"));

    // Ensure server is running, auto-start if needed
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

    // Sync all configurations (vibe and opencode)
    println!("Syncing configurations...");
    crate::commands::sync::run(cfg).await?;

    // Replace the current process with vibe
    println!("Launching vibe in {}", folder.display());
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new("vibe")
        .current_dir(&folder)
        .args(extra_args)
        .exec();
    anyhow::bail!("Failed to exec vibe: {err}");
}

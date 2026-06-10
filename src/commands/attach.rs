use anyhow::{bail, Result};
use crate::llamacpp;
use std::process::Command;

pub fn run() -> Result<()> {
    let pid = llamacpp::read_pid().ok_or_else(|| anyhow::anyhow!("No running llama-server found (no PID file)."))?;

    // Check if process is actually running
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()?;

    if !status.success() {
        bail!("llama-server (PID {}) is not running.", pid);
    }

    println!("Attaching to llama-server (PID {})...", pid);
    let log_path = llamacpp::log_path();

    if !log_path.exists() {
        bail!(
            "Log file not found at {}\n\
             The server may not have started yet. Run `yllama attach` after `yllama serve`.",
            log_path.display()
        );
    }

    let status = Command::new("tail")
        .arg("-f")
        .arg(&log_path)
        .status()?;

    if !status.success() {
        bail!("Failed to attach to log file: {}", log_path.display());
    }

    Ok(())
}

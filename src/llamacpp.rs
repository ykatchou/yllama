use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::config::{yllama_dir, Config};

pub fn pid_path() -> PathBuf {
    yllama_dir().join("llamacpp.pid")
}

pub fn log_path() -> PathBuf {
    yllama_dir().join("llamacpp.log")
}

pub async fn is_running(cfg: &Config) -> bool {
    let url = format!("http://{}:{}/health", cfg.host, cfg.port);
    reqwest::get(&url)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

pub fn read_pid() -> Option<u32> {
    std::fs::read_to_string(pid_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn write_pid(pid: u32) -> Result<()> {
    std::fs::create_dir_all(yllama_dir())?;
    std::fs::write(pid_path(), pid.to_string()).context("writing PID file")
}

pub fn clear_pid() {
    let _ = std::fs::remove_file(pid_path());
}

fn base_cmd(cfg: &Config, model_path: &Path, extra_args: &[String]) -> std::process::Command {
    let mut cmd = std::process::Command::new(&cfg.server_bin);
    let threads = (num_cpus::get() as i32 - 2).max(1);
    cmd.arg("-m")
        .arg(model_path)
        .arg("--host")
        .arg(&cfg.host)
        .arg("--port")
        .arg(cfg.port.to_string())
        .arg("-t")
        .arg(threads.to_string());
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd
}

/// Spawn llama-server as a detached background daemon.
/// Returns the child PID. Stdout/stderr are redirected to llamacpp.log.
pub fn spawn_daemon(cfg: &Config, model_path: &Path, extra_args: &[String]) -> Result<u32> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::fs::OpenOptions;

    let mut cmd = base_cmd(cfg, model_path, extra_args);
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())?;

    cmd.stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file);

    // New session — server outlives the terminal that started it
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("launching {}", cfg.server_bin))?;
    let pid = child.id();
    std::mem::forget(child); // Intentionally detached — don't wait
    Ok(pid)
}

/// Spawn llama-server in the foreground (inherits stdin/stdout/stderr).
/// Returns the Child so the caller can wait on it.
pub fn spawn_foreground(
    cfg: &Config,
    model_path: &Path,
    extra_args: &[String],
) -> Result<std::process::Child> {
    base_cmd(cfg, model_path, extra_args)
        .spawn()
        .with_context(|| format!("launching {}", cfg.server_bin))
}

pub fn kill_server() -> Result<()> {
    match read_pid() {
        None => bail!("No PID file found — is llama-server running? Start it with `yllama serve`."),
        Some(pid) => {
            let status = std::process::Command::new("kill")
                .arg(pid.to_string())
                .status()
                .context("running kill")?;
            if !status.success() {
                bail!("Failed to kill PID {pid} — process may have already exited");
            }
            clear_pid();
            println!("Stopped llama-server (PID {pid})");
            Ok(())
        }
    }
}

pub async fn wait_for_ready(cfg: &Config, timeout_secs: u64) -> Result<()> {
    use std::time::{Duration, Instant};
    let url = format!("http://{}:{}/health", cfg.host, cfg.port);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if let Ok(r) = reqwest::get(&url).await {
            if r.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    bail!("llama-server did not become ready within {timeout_secs}s")
}

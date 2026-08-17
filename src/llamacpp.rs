use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::config::{yllama_dir, Config};
use crate::deps;

/// Inspect llama-server's log output for known failure patterns and return
/// an actionable hint the user can act on immediately.
pub fn diagnose_error(log: &str) -> Option<String> {
    if log.contains("unknown model architecture") {
        let upgrade_cmd = if cfg!(target_os = "macos") {
            "brew upgrade llama.cpp"
        } else if cfg!(target_os = "linux") {
            "sudo apt update && sudo apt install --only-upgrade llama.cpp  # or: sudo dnf upgrade llama.cpp"
        } else {
            "rebuild/reinstall llama.cpp from https://github.com/ggerganov/llama.cpp"
        };
        Some(format!(
            "That GGUF's architecture isn't recognized by your installed llama-server.\n  \
             This usually means llama.cpp is out of date — try:\n\n  \
             {upgrade_cmd}\n\n  \
             ...then run this command again."
        ))
    } else {
        None
    }
}

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
/// Returns the Child (kept open only so the caller can poll `try_wait()`
/// while waiting for readiness — it's safe to drop once that's done, since
/// the process runs in its own session). Stdout/stderr go to llamacpp.log.
pub fn spawn_daemon(cfg: &Config, model_path: &Path, extra_args: &[String]) -> Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::fs::OpenOptions;

    // Check that the binary exists before trying to spawn
    deps::check_binary(&cfg.server_bin).with_context(|| {
        format!(
            "llama-server binary not found. \
             yllama serve only requires llama.cpp — integrations (vibe, claude) need more."
        )
    })?;

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

    cmd.spawn()
        .with_context(|| format!("launching {}", cfg.server_bin))
}

/// Spawn llama-server in the foreground (inherits stdin/stdout; stderr is
/// piped so it can be tee'd to the terminal while also being inspected for
/// known failure patterns).
/// Returns the Child so the caller can wait on it.
pub fn spawn_foreground(
    cfg: &Config,
    model_path: &Path,
    extra_args: &[String],
) -> Result<std::process::Child> {
    use std::process::Stdio;

    // Check that the binary exists before trying to spawn
    deps::check_binary(&cfg.server_bin).with_context(|| {
        format!(
            "llama-server binary not found. \
             yllama start only requires llama.cpp — integrations (vibe, claude) need more."
        )
    })?;

    base_cmd(cfg, model_path, extra_args)
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("launching {}", cfg.server_bin))
}

/// Wait for a foreground llama-server child to exit, echoing its stderr to
/// the terminal live and, if it exits with a failure, printing an
/// actionable hint when the failure matches a known pattern.
pub fn wait_foreground(mut child: std::process::Child) -> Result<()> {
    let stderr = child.stderr.take();
    let captured = Arc::new(Mutex::new(String::new()));
    let handle = stderr.map(|stderr| {
        let captured = Arc::clone(&captured);
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut buf = captured.lock().unwrap();
            for line in reader.lines().map_while(std::io::Result::ok) {
                eprintln!("{line}");
                buf.push_str(&line);
                buf.push('\n');
            }
        })
    });

    let status = child.wait().context("waiting for llama-server")?;
    if let Some(handle) = handle {
        let _ = handle.join();
    }

    if !status.success() {
        if let Some(hint) = diagnose_error(&captured.lock().unwrap()) {
            eprintln!("\n{hint}");
        }
    }

    Ok(())
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

/// Poll until the daemon's health endpoint responds, or bail early (with a
/// diagnosis, if we recognize the failure) if the process exits first.
pub async fn wait_for_ready(
    cfg: &Config,
    timeout_secs: u64,
    child: &mut std::process::Child,
) -> Result<()> {
    use std::time::{Duration, Instant};
    let url = format!("http://{}:{}/health", cfg.host, cfg.port);
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if let Ok(r) = reqwest::get(&url).await {
            if r.status().is_success() {
                return Ok(());
            }
        }
        if let Ok(Some(_status)) = child.try_wait() {
            let log = std::fs::read_to_string(log_path()).unwrap_or_default();
            if let Some(hint) = diagnose_error(&log) {
                eprintln!("\n{hint}");
            }
            bail!(
                "llama-server exited before becoming ready — see {}",
                log_path().display()
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    bail!("llama-server did not become ready within {timeout_secs}s")
}

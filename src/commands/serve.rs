use anyhow::{bail, Result};

use crate::{config::Config, llamacpp, manifest};

pub async fn run(
    cfg: &Config,
    model_name: Option<&str>,
    foreground: bool,
    cli_extra_args: &[String],
) -> Result<()> {
    let entries = manifest::load()?;
    let entry = match model_name {
        Some(name) => manifest::find(&entries, name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Model '{name}' not in manifest. \
                     Run `yllama models add <url>` and `yllama models download {name}` first."
                )
            })?
            .clone(),
        None => {
            let first_downloaded = entries
                .iter()
                .find(|e| e.downloaded && manifest::model_path(e).exists());

            match first_downloaded {
                Some(e) => {
                    println!("Warning: No model specified. Using the first available downloaded model: '{}'", e.name);
                    e.clone()
                }
                None => {
                    anyhow::bail!(
                        "No downloaded models found. \
                         Run `yllama models download <name>` first."
                    )
                }
            }
        }
    };

    let model_path = manifest::model_path(&entry);
    if !model_path.exists() {
        bail!(
            "Model file not found: {}\nRun `yllama models download {}` first.",
            model_path.display(),
            entry.name
        );
    }

    if llamacpp::is_running(cfg).await {
        println!(
            "llama-server is already running on {}:{}",
            cfg.host, cfg.port
        );
        return Ok(());
    }

    // Model-level defaults come first; CLI flags take precedence (appended last)
    let all_extra: Vec<String> = entry
        .extra_args
        .iter()
        .chain(cli_extra_args)
        .cloned()
        .collect();

    if !all_extra.is_empty() {
        println!("Extra args: {}", all_extra.join(" "));
    }

    println!("Starting llama-server with model '{}'...", entry.name);

    if foreground {
        let mut child = llamacpp::spawn_foreground(cfg, &model_path, &all_extra)?;
        let pid = child.id();
        llamacpp::write_pid(pid)?;
        println!(
            "llama-server running in foreground (PID {pid}) — press Ctrl-C to stop"
        );
        child.wait()?;
        llamacpp::clear_pid();
    } else {
        let pid = llamacpp::spawn_daemon(cfg, &model_path, &all_extra)?;
        llamacpp::write_pid(pid)?;
        print!("llama-server started (PID {pid}), waiting for ready...");
        llamacpp::wait_for_ready(cfg, 60).await?;
        println!(" ready at http://{}:{}", cfg.host, cfg.port);
    }

    Ok(())
}

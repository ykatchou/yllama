use anyhow::{bail, Result};

use crate::{config::Config, commands::select_model, llamacpp, manifest, ReasoningLevel};

pub async fn run(
    cfg: &Config,
    model_name: Option<&str>,
    foreground: bool,
    thinking: ReasoningLevel,
    cli_extra_args: &[String],
) -> Result<()> {
    if foreground {
        // Set up signal handler so PID file is cleaned on Ctrl-C
        let pid_cleanup = llamacpp::pid_path();
        let _ = ctrlc::set_handler(move || {
            let _ = std::fs::remove_file(&pid_cleanup);
            std::process::exit(0);
        });
    }

    // Check if server is already running before asking for model selection
    if llamacpp::is_running(cfg).await {
        println!(
            "llama-server is already running on {}:{}",
            cfg.host, cfg.port
        );
        return Ok(());
    }

    let entries = manifest::load()?;
    let (entry, default_to_set) = match model_name {
        Some(name) => {
            let e = manifest::find(&entries, name)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Model '{name}' not in manifest. \
                         Run `yllama models add <url>` and `yllama models download {name}` first."
                    )
                })?
                .clone();
            (e, None)
        }
        None => select_model::select_model(&entries)?,
    };

    // Persist "Set as default" if the user chose that option
    if let Some(default_name) = default_to_set {
        let mut entries = entries;
        if let Some(e) = entries.iter_mut().find(|e| e.name == default_name) {
            e.default_model = Some(default_name.clone());
            manifest::save(&entries)?;
            println!("'{default_name}' set as default model.");
        }
    }

    let model_path = manifest::model_path(&entry);
    if !model_path.exists() {
        bail!(
            "Model file not found: {}\nRun `yllama models download {}` first.",
            model_path.display(),
            entry.name
        );
    }

    // Model-level defaults come first; CLI flags take precedence (appended last)
    let reasoning_flag = match thinking {
        ReasoningLevel::Off => vec!["--reasoning".to_string(), "off".to_string()],
        _ => vec!["--reasoning".to_string(), "on".to_string()],
    };
    let all_extra: Vec<String> = entry
        .extra_args
        .iter()
        .chain(reasoning_flag.iter())
        .chain(cli_extra_args.iter())
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

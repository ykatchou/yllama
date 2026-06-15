use anyhow::{bail, Result};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;

use crate::manifest::{self, ModelEntry};

/// Returns the active model with a smooth, predictable flow:
/// - 0 downloaded → error
/// - 1 downloaded → returns it silently
/// - default_model set & valid → auto-selects it (no interaction)
/// - 2+ downloaded, no default → shows interactive TUI picker
/// - Non-TTY + 2+ models → warns and picks first downloaded
///
/// Returns `(selected_model, Some(model_name))` when user chose "Set as default".
/// The caller is responsible for persisting the change to the manifest.
pub fn select_model(entries: &[ModelEntry]) -> Result<(ModelEntry, Option<String>)> {
    let downloaded: Vec<&ModelEntry> = entries
        .iter()
        .filter(|e| e.downloaded && manifest::model_path(e).exists())
        .collect();

    match downloaded.len() {
        0 => bail!(
            "No downloaded models found. \
             Run `yllama models add <url>` then `yllama models download <name>` to add one."
        ),
        1 => Ok((downloaded[0].clone(), None)),
        _ => {
            // Check for an explicitly set default that is also downloaded
            let active_default = entries.iter().find_map(|e| {
                e.default_model.as_deref().filter(|name| {
                    downloaded.iter().any(|d| d.name == *name)
                })
            });

            // If a default is already set, use it silently — no picker needed
            if let Some(default_name) = active_default {
                let idx = downloaded
                    .iter()
                    .position(|e| e.name == default_name)
                    .unwrap();
                return Ok((downloaded[idx].clone(), None));
            }

            // No default set — show interactive picker
            let is_tty = std::io::stdout().is_terminal();

            let items: Vec<String> = downloaded
                .iter()
                .map(|e| {
                    let size = manifest::format_bytes(e.size_bytes.unwrap_or(0));
                    format!("{}  [{size}]", e.name)
                })
                .collect();

            let preselected = 0; // nothing is default yet, start at first

            let selection = if is_tty {
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Select a model")
                    .items(&items)
                    .default(preselected)
                    .interact()?;
                selection
            } else {
                eprintln!(
                    "Warning: terminal is not interactive and no default model is set. \
                     Using the first downloaded model: '{}'",
                    downloaded[0].name
                );
                eprintln!(
                    "To set a default: run `yllama serve` in a terminal, \
                     or set `default_model` in the manifest."
                );
                0
            };

            let model = downloaded[selection].clone();
            Ok((model, None))
        }
    }
}

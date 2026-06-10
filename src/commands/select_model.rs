use anyhow::{bail, Result};
use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;

use crate::manifest::{self, ModelEntry};

/// Returns the active model:
/// - 0 downloaded → error
/// - 1 downloaded → returns it silently
/// - 2+ downloaded → shows interactive Select menu, with "Set as default" option
/// - Non-TTY → silently picks the first downloaded model
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
             Run `yllama models download <name>` first."
        ),
        1 => Ok((downloaded[0].clone(), None)),
        _ => {
            let is_tty = std::io::stdout().is_terminal();

            // Check if a default is already marked
            let existing_default =
                entries.iter().find_map(|e| e.default_model.as_deref()).filter(|name| {
                    downloaded.iter().any(|d| d.name == *name)
                });

            // Build display items: "{name}  [{size}]"
            let items: Vec<String> = downloaded
                .iter()
                .map(|e| {
                    let size = manifest::format_bytes(e.size_bytes.unwrap_or(0));
                    if existing_default.is_some_and(|d| d == e.name) {
                        format!("{}  [{size}] (default)", e.name)
                    } else {
                        format!("{}  [{size}]", e.name)
                    }
                })
                .collect();

            // Add "Set as default" option
            let mut items_with_default = items.clone();
            let default_label = if existing_default.is_some() {
                "(change default)"
            } else {
                "Set as default"
            };
            items_with_default.push(default_label.to_string());
            let default_idx = items_with_default.len() - 1;

            // Pre-select: if a default exists, pre-select that model; otherwise first
            let preselected = existing_default
                .and_then(|name| downloaded.iter().position(|e| e.name == name))
                .unwrap_or(0);

            let selection = if is_tty {
                let prompt = if existing_default.is_some() {
                    "Select a model (already have a default set — use Space to change it)"
                } else {
                    "Select a model to use"
                };
                Select::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .items(&items_with_default)
                    .default(preselected)
                    .interact()?
            } else {
                // Non-interactive: pick the first downloaded model
                eprintln!(
                    "Warning: No model specified and terminal is not interactive. \
                     Using the first available downloaded model: '{}'",
                    downloaded[0].name
                );
                0
            };

            if selection == default_idx {
                // "Set as default" — mark the preselected model
                let model = downloaded[preselected].clone();
                Ok((model, Some(downloaded[preselected].name.clone())))
            } else {
                let model = downloaded[selection].clone();
                Ok((model, None))
            }
        }
    }
}

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::yllama_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub hf_url: String,
    pub filename: String,
    pub downloaded: bool,
    pub size_bytes: Option<u64>,
    /// Default extra llama.cpp flags for this model (e.g. ["-ngl", "35", "-c", "8192"]).
    /// Merged with any flags passed on the CLI at serve time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    /// When Some(name), this model is marked as the user's default choice.
    /// Set interactively via `yllama serve` menu when multiple models exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Whether speculative-decoding support (MTP or a separate drafter file)
    /// has been probed for this model yet. Old manifest entries default to
    /// `false` so `yllama models sync` knows to backfill them.
    #[serde(default)]
    pub mtp_checked: bool,
    /// True when the downloaded GGUF has MTP heads baked in (e.g. DeepSeek,
    /// GLM, or a `-MTP-GGUF` variant of a Qwen model) — self-speculative
    /// decoding is enabled automatically at serve time via `--spec-type
    /// draft-mtp`, no extra file needed.
    #[serde(default)]
    pub mtp_builtin: bool,
    /// Download URL for a separate speculative-decoding drafter GGUF bundled
    /// in the same HF repo (e.g. Muse-Glimmer's `dflash-kquant.gguf`), if one
    /// was found and the user opted to fetch it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_url: Option<String>,
    /// Filename of the drafter GGUF, relative to `models_dir()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_filename: Option<String>,
    /// The `--spec-type` value the drafter requires (e.g. `draft-dflash`,
    /// `draft-dspark`, `draft-eagle3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_spec_type: Option<String>,
    /// Whether the drafter GGUF has been downloaded.
    #[serde(default)]
    pub draft_downloaded: bool,
    /// For models registered from a file already on disk (`yllama models add
    /// <path.gguf>`), the absolute path the user pointed at. The entry in
    /// `models_dir()` is then a copy or a symlink of it, and there is nothing
    /// to download — `hf_url` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_source: Option<String>,
}

pub fn models_dir() -> PathBuf {
    yllama_dir().join("models")
}

fn manifest_path() -> PathBuf {
    models_dir().join("manifest.json")
}

pub fn load() -> Result<Vec<ModelEntry>> {
    let path = manifest_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).context("parsing manifest.json")
}

pub fn save(entries: &[ModelEntry]) -> Result<()> {
    let path = manifest_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let text = serde_json::to_string_pretty(entries)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}

pub fn find<'a>(entries: &'a [ModelEntry], name: &str) -> Option<&'a ModelEntry> {
    entries.iter().find(|e| e.name == name)
}

/// True for entries registered from a local file rather than a download.
pub fn is_local(entry: &ModelEntry) -> bool {
    entry.local_source.is_some()
}

pub fn model_path(entry: &ModelEntry) -> PathBuf {
    models_dir().join(&entry.filename)
}

/// Path to the entry's separate drafter GGUF, if one is registered.
/// Does not check whether the file has actually been downloaded yet.
pub fn draft_model_path(entry: &ModelEntry) -> Option<PathBuf> {
    entry.draft_filename.as_ref().map(|f| models_dir().join(f))
}

/// Return the active model: first, the user's marked default (if downloaded);
/// otherwise the first downloaded model in manifest order.
#[allow(dead_code)]
pub fn find_active_model(entries: &[ModelEntry]) -> Option<&ModelEntry> {
    // Check for an explicitly marked default
    if let Some(default_name) = entries.iter().find_map(|e| e.default_model.as_deref()) {
        if let Some(e) = entries.iter().find(|e| e.name == default_name && e.downloaded) {
            return Some(e);
        }
    }
    // Fallback: first downloaded
    entries.iter().find(|e| e.downloaded && model_path(e).exists())
}

/// Resolve the currently running model name from the server's `/v1/models` response.
/// Falls back to `find_active_model` if no match is found.
pub fn resolve_running_model_name(
    live_model_ids: &[&str],
    entries: &[ModelEntry],
) -> Option<String> {
    // Try to match a live model ID against manifest entries
    for live_id in live_model_ids {
        if let Some(entry) = entries.iter().find(|e| {
            e.name == *live_id
                || e.filename.contains(live_id)
                || live_id.contains(&e.name)
        }) {
            return Some(entry.name.clone());
        }
    }
    // Fallback: use the manifest's active model
    find_active_model(entries).map(|e| e.name.clone())
}

/// Format bytes into a human-readable string (GB / MB / B).
pub fn format_bytes(b: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if b >= GB {
        format!("{:.1} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else {
        format!("{b} B")
    }
}

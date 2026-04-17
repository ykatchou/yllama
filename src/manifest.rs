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

pub fn model_path(entry: &ModelEntry) -> PathBuf {
    models_dir().join(&entry.filename)
}

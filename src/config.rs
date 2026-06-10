use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_server_bin")]
    pub server_bin: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

fn default_server_bin() -> String {
    "llama-server".to_string()
}
fn default_port() -> u16 {
    4200
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_bin: default_server_bin(),
            port: default_port(),
            host: default_host(),
        }
    }
}

pub fn yllama_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".yllama")
}

pub fn load() -> Result<Config> {
    let path = yllama_dir().join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).context("parsing ~/.yllama/config.toml")
}

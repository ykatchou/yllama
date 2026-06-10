use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

const PROVIDER_NAME: &str = "llamacpp";

// --- extra_args parsing ------------------------------------------------------

/// Structured model parameters extracted from llama.cpp extra_args.
#[derive(Debug, Default)]
pub struct ModelParams {
    pub context_size: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub top_k: Option<i32>,
}

/// Parse llama.cpp extra_args and extract known model parameters.
///
/// Supported flags:
///   -c / --ctx-size N      → context_size
///   --temp N               → temperature
///   --top-k N              → top_k
///   --top-p N              → top_p
pub fn parse_extra_args(args: &[String]) -> ModelParams {
    let mut params = ModelParams::default();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-c" | "--ctx-size" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<u32>() {
                        params.context_size = Some(n);
                    }
                }
                i += 2;
                continue;
            }
            "--temp" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(v) = val.parse::<f64>() {
                        params.temperature = Some(v);
                    }
                }
                i += 2;
                continue;
            }
            "--top-k" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<i32>() {
                        params.top_k = Some(n);
                    }
                }
                i += 2;
                continue;
            }
            "--top-p" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(v) = val.parse::<f64>() {
                        params.top_p = Some(v);
                    }
                }
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    params
}

pub fn vibe_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".vibe")
        .join("config.toml")
}

pub async fn fetch_models(base_url: &str) -> Result<Vec<Value>> {
    let url = format!("{base_url}/v1/models");
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("querying {url}"))?;
    let data: Value = resp.json().await.context("parsing /v1/models response")?;
    Ok(data["data"].as_array().cloned().unwrap_or_default())
}

pub fn sync_with_models(
    base_url: &str,
    models: &[Value],
    manifest_entries: &[crate::manifest::ModelEntry],
) -> Result<()> {
    let path = vibe_config_path();
    let config: toml::Value = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).context("parsing ~/.vibe/config.toml")?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let new_content = rebuild(&config, base_url, models, manifest_entries);
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, &new_content)
        .with_context(|| format!("writing {}", path.display()))
}

// --- serialization helpers ---------------------------------------------------

pub fn dump_toml_value(val: &toml::Value) -> String {
    match val {
        toml::Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Float(f) => {
            let s = format!("{f}");
            if s.contains('.') { s } else { format!("{s}.0") }
        }
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(dt) => dt.to_string(),
        toml::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(dump_toml_value).collect();
            format!("[{}]", items.join(", "))
        }
        toml::Value::Table(t) => {
            let items: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{k} = {}", dump_toml_value(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

fn rebuild(
    config: &toml::Value,
    base_url: &str,
    models: &[Value],
    manifest_entries: &[crate::manifest::ModelEntry],
) -> String {
    let mut lines: Vec<String> = vec![];

    let empty_map = toml::map::Map::new();
    let table = match config {
        toml::Value::Table(t) => t,
        _ => &empty_map,
    };

    // Top-level scalar/inline-table keys (not [[providers]] / [[models]])
    let section_keys = ["providers", "models"];
    let mut top_keys: Vec<(String, toml::Value)> = table
        .iter()
        .filter(|(k, _)| !section_keys.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Update active_model to the first live model from the server
    if let Some(first) = models.first() {
        let live_id = first["id"].as_str().unwrap_or("unknown").to_string();
        if let Some(entry) = top_keys.iter_mut().find(|(k, _)| k == "active_model") {
            let old = entry.1.as_str().unwrap_or("").to_string();
            if old != live_id {
                println!("  active_model: {old:?} → {live_id:?}");
            }
            entry.1 = toml::Value::String(live_id);
        } else {
            top_keys.insert(0, ("active_model".to_string(), toml::Value::String(live_id)));
        }
    }

    for (k, v) in &top_keys {
        lines.push(format!("{k} = {}", dump_toml_value(v)));
    }
    if !top_keys.is_empty() {
        lines.push(String::new());
    }

    // [[providers]]: keep non-llamacpp entries, append ours
    let existing_providers: Vec<&toml::Value> = table
        .get("providers")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|p| p.get("name").and_then(|n| n.as_str()) != Some(PROVIDER_NAME))
                .collect()
        })
        .unwrap_or_default();

    let mut our_map = toml::map::Map::new();
    our_map.insert("name".into(), toml::Value::String(PROVIDER_NAME.into()));
    our_map.insert(
        "api_base".into(),
        toml::Value::String(format!("{base_url}/v1")),
    );
    our_map.insert("api_key_env_var".into(), toml::Value::String(String::new()));
    our_map.insert("api_style".into(), toml::Value::String("openai".into()));
    our_map.insert("backend".into(), toml::Value::String("generic".into()));
    let our_provider = toml::Value::Table(our_map);

    for p in existing_providers
        .into_iter()
        .chain(std::iter::once(&our_provider))
    {
        lines.push("[[providers]]".into());
        if let toml::Value::Table(t) = p {
            for (k, v) in t {
                lines.push(format!("{k} = {}", dump_toml_value(v)));
            }
        }
        lines.push(String::new());
    }

    // [[models]]: keep non-llamacpp entries, append ours from server
    let existing_models: Vec<&toml::Value> = table
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|m| m.get("provider").and_then(|p| p.as_str()) != Some(PROVIDER_NAME))
                .collect()
        })
        .unwrap_or_default();

    let new_models: Vec<toml::Value> = models
        .iter()
        .map(|m| {
            let id = m["id"].as_str().unwrap_or("unknown").to_string();
            let mut t = toml::map::Map::new();
            t.insert("name".into(), toml::Value::String(id.clone()));
            t.insert("provider".into(), toml::Value::String(PROVIDER_NAME.into()));
            t.insert("alias".into(), toml::Value::String(id.clone()));

            // Look up manifest entry for this model's extra_args
            let params = manifest_entries
                .iter()
                .find(|e| e.filename.contains(&id) || e.name.contains(&id))
                .map(|e| parse_extra_args(&e.extra_args))
                .unwrap_or_default();

            // Use extracted params, fall back to sensible defaults
            t.insert(
                "temperature".into(),
                toml::Value::Float(params.temperature.unwrap_or(0.7)),
            );
            if let Some(ctx) = params.context_size {
                t.insert("context_size".into(), toml::Value::Integer(ctx as i64));
            }
            if let Some(top_p) = params.top_p {
                t.insert("top_p".into(), toml::Value::Float(top_p));
            }
            if let Some(top_k) = params.top_k {
                t.insert("top_k".into(), toml::Value::Integer(top_k as i64));
            }
            t.insert("input_price".into(), toml::Value::Float(0.0));
            t.insert("output_price".into(), toml::Value::Float(0.0));
            toml::Value::Table(t)
        })
        .collect();

    for m in existing_models
        .into_iter()
        .chain(new_models.iter())
    {
        lines.push("[[models]]".into());
        if let toml::Value::Table(t) = m {
            for (k, v) in t {
                lines.push(format!("{k} = {}", dump_toml_value(v)));
            }
        }
        lines.push(String::new());
    }

    lines.join("\n") + "\n"
}

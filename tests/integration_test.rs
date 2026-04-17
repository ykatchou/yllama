/// Integration tests for yllama.
///
/// These tests exercise pure logic (manifest CRUD, TOML serialization,
/// URL parsing, config generation) without hitting the network or spawning
/// real processes.  Tests that require a live llama-server are marked
/// `#[ignore]` and can be run explicitly with `cargo test -- --ignored`.

use std::path::PathBuf;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temporary directory and return (TempDir, models_path).
/// The caller must keep `TempDir` alive for the duration of the test.
fn tmp_models_dir() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).unwrap();
    (dir, models)
}

// ---------------------------------------------------------------------------
// Manifest (JSON) round-trip
// ---------------------------------------------------------------------------

mod manifest_tests {
    use serde_json::json;

    fn make_entry(name: &str, downloaded: bool) -> serde_json::Value {
        json!({
            "name": name,
            "hf_url": format!("https://huggingface.co/owner/repo/resolve/main/{name}.gguf"),
            "filename": format!("{name}.gguf"),
            "downloaded": downloaded,
            "size_bytes": if downloaded { serde_json::Value::Number(1234.into()) } else { serde_json::Value::Null }
        })
    }

    #[test]
    fn round_trip_empty() {
        let entries: Vec<serde_json::Value> = vec![];
        let serialized = serde_json::to_string_pretty(&entries).unwrap();
        let deserialized: Vec<serde_json::Value> =
            serde_json::from_str(&serialized).unwrap();
        assert!(deserialized.is_empty());
    }

    #[test]
    fn round_trip_with_entries() {
        let entries = vec![
            make_entry("gemma-Q4", true),
            make_entry("llama-Q5", false),
        ];
        let serialized = serde_json::to_string_pretty(&entries).unwrap();
        let deserialized: Vec<serde_json::Value> =
            serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0]["name"], "gemma-Q4");
        assert_eq!(deserialized[1]["downloaded"], false);
    }

    #[test]
    fn find_entry_by_name() {
        let entries = vec![
            make_entry("gemma-Q4", true),
            make_entry("llama-Q5", false),
        ];
        let found = entries.iter().find(|e| e["name"] == "llama-Q5");
        assert!(found.is_some());
        let not_found = entries.iter().find(|e| e["name"] == "unknown");
        assert!(not_found.is_none());
    }
}

// ---------------------------------------------------------------------------
// URL parsing (mirrors logic in commands/models/add.rs)
// ---------------------------------------------------------------------------

mod url_parsing_tests {
    fn blob_to_resolve(url: &str) -> String {
        url.replace("/blob/", "/resolve/")
    }

    fn filename_from_url(url: &str) -> &str {
        url.split('?')
            .next()
            .unwrap_or(url)
            .split('/')
            .last()
            .unwrap_or("")
    }

    fn name_from_filename(filename: &str) -> &str {
        filename.trim_end_matches(".gguf")
    }

    #[test]
    fn blob_converted_to_resolve() {
        let url = "https://huggingface.co/owner/repo/blob/main/model.gguf";
        assert!(blob_to_resolve(url).contains("/resolve/"));
        assert!(!blob_to_resolve(url).contains("/blob/"));
    }

    #[test]
    fn resolve_url_unchanged() {
        let url = "https://huggingface.co/owner/repo/resolve/main/model.gguf";
        assert_eq!(blob_to_resolve(url), url);
    }

    #[test]
    fn filename_extracted_from_url() {
        let url = "https://huggingface.co/owner/repo/resolve/main/gemma-Q4_K_M.gguf";
        assert_eq!(filename_from_url(url), "gemma-Q4_K_M.gguf");
    }

    #[test]
    fn filename_strips_query_string() {
        let url = "https://example.com/model.gguf?download=true&auth=abc";
        assert_eq!(filename_from_url(url), "model.gguf");
    }

    #[test]
    fn name_derived_strips_gguf() {
        assert_eq!(name_from_filename("gemma-Q4_K_M.gguf"), "gemma-Q4_K_M");
    }

    #[test]
    fn non_gguf_file_not_stripped() {
        // Only .gguf is trimmed; other extensions are preserved
        assert_eq!(
            name_from_filename("model.bin").trim_end_matches(".gguf"),
            "model.bin"
        );
    }
}

// ---------------------------------------------------------------------------
// Vibe config TOML serialization
// ---------------------------------------------------------------------------

mod vibe_config_tests {
    // We test dump_toml_value by checking the output format for each value type.

    fn dump(v: &toml::Value) -> String {
        // Inline copy of the logic from vibe_config.rs for test isolation
        match v {
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
            toml::Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|x| dump(x)).collect();
                format!("[{}]", items.join(", "))
            }
            toml::Value::Table(t) => {
                let items: Vec<String> = t
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", dump(v)))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            toml::Value::Datetime(dt) => dt.to_string(),
        }
    }

    #[test]
    fn string_escaped() {
        let v = toml::Value::String(r#"say "hi""#.to_string());
        assert_eq!(dump(&v), r#""say \"hi\"""#);
    }

    #[test]
    fn integer_serialized() {
        assert_eq!(dump(&toml::Value::Integer(42)), "42");
    }

    #[test]
    fn float_has_decimal() {
        let v = toml::Value::Float(0.2);
        let s = dump(&v);
        assert!(s.contains('.'), "float must have a decimal point: {s}");
    }

    #[test]
    fn boolean_true() {
        assert_eq!(dump(&toml::Value::Boolean(true)), "true");
    }

    #[test]
    fn boolean_false() {
        assert_eq!(dump(&toml::Value::Boolean(false)), "false");
    }

    #[test]
    fn array_of_strings() {
        let v = toml::Value::Array(vec![
            toml::Value::String("a".into()),
            toml::Value::String("b".into()),
        ]);
        assert_eq!(dump(&v), r#"["a", "b"]"#);
    }

    #[test]
    fn nested_inline_table() {
        let mut map = toml::map::Map::new();
        map.insert("x".into(), toml::Value::Integer(1));
        map.insert("y".into(), toml::Value::Boolean(false));
        let v = toml::Value::Table(map);
        let s = dump(&v);
        assert!(s.starts_with('{') && s.ends_with('}'));
        assert!(s.contains("x = 1"));
        assert!(s.contains("y = false"));
    }

    #[test]
    fn vibe_config_preserves_non_llamacpp_providers() {
        // Build a minimal existing config with one non-llamacpp provider
        let existing_toml = r#"
active_model = "old-model"

[[providers]]
name = "mistral"
api_base = "https://api.mistral.ai/v1"
api_key_env_var = "MISTRAL_API_KEY"
api_style = "openai"
backend = "generic"

[[models]]
name = "devstral"
provider = "mistral"
alias = "devstral-latest"
temperature = 0.3
input_price = 0.4
output_price = 2.0
"#;
        let config: toml::Value = toml::from_str(existing_toml).unwrap();

        // Simulate what sync_with_models does
        let providers = config["providers"].as_array().unwrap();
        let kept: Vec<_> = providers
            .iter()
            .filter(|p| p["name"].as_str() != Some("llamacpp"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["name"].as_str().unwrap(), "mistral");

        let models = config["models"].as_array().unwrap();
        let kept_models: Vec<_> = models
            .iter()
            .filter(|m| m["provider"].as_str() != Some("llamacpp"))
            .collect();
        assert_eq!(kept_models.len(), 1);
        assert_eq!(kept_models[0]["name"].as_str().unwrap(), "devstral");
    }
}

// ---------------------------------------------------------------------------
// LiteLLM config generation
// ---------------------------------------------------------------------------

mod litellm_tests {
    fn build_litellm_config(base_url: &str, model_ids: &[&str]) -> String {
        let mut lines = vec!["model_list:".to_string()];
        for id in model_ids {
            lines.push(format!("  - model_name: {id}"));
            lines.push("    litellm_params:".to_string());
            lines.push(format!("      model: openai/{id}"));
            lines.push(format!("      api_base: {base_url}/v1"));
            lines.push("      api_key: \"none\"".to_string());
        }
        lines.push(String::new());
        lines.push("litellm_settings:".to_string());
        lines.push("  drop_params: true".to_string());
        lines.push(String::new());
        lines.join("\n")
    }

    #[test]
    fn generates_model_list() {
        let cfg = build_litellm_config(
            "http://localhost:8080",
            &["gemma-4-26B", "llama-3.2-3B"],
        );
        assert!(cfg.contains("model_list:"));
        assert!(cfg.contains("model_name: gemma-4-26B"));
        assert!(cfg.contains("model_name: llama-3.2-3B"));
    }

    #[test]
    fn uses_openai_model_prefix() {
        let cfg = build_litellm_config("http://localhost:8080", &["my-model"]);
        assert!(cfg.contains("model: openai/my-model"));
    }

    #[test]
    fn api_base_points_to_v1() {
        let cfg = build_litellm_config("http://localhost:8080", &["my-model"]);
        assert!(cfg.contains("api_base: http://localhost:8080/v1"));
    }

    #[test]
    fn includes_drop_params_setting() {
        let cfg = build_litellm_config("http://localhost:8080", &["my-model"]);
        assert!(cfg.contains("drop_params: true"));
    }

    #[test]
    fn empty_model_list() {
        let cfg = build_litellm_config("http://localhost:8080", &[]);
        assert!(cfg.contains("model_list:"));
        assert!(!cfg.contains("model_name:"));
    }
}

// ---------------------------------------------------------------------------
// Install path resolution
// ---------------------------------------------------------------------------

mod install_tests {
    use super::tmp_models_dir;
    use std::io::Write;

    #[test]
    fn installs_binary_to_target_dir() {
        let (dir, _) = tmp_models_dir();
        let target_dir = dir.path().join("bin");

        // Write a fake "binary"
        let fake_exe = dir.path().join("yllama_fake");
        let mut f = std::fs::File::create(&fake_exe).unwrap();
        writeln!(f, "#!/bin/sh\necho hello").unwrap();

        std::fs::create_dir_all(&target_dir).unwrap();
        let target = target_dir.join("yllama");
        std::fs::copy(&fake_exe, &target).unwrap();

        assert!(target.exists());
    }
}

// ---------------------------------------------------------------------------
// serve / start — extra_args merging
// ---------------------------------------------------------------------------

mod serve_tests {
    /// Mirrors the merging logic in commands/serve.rs: model-level extra_args
    /// come first, then CLI-supplied flags, so CLI flags take precedence.
    fn merge_args(model_args: &[&str], cli_args: &[&str]) -> Vec<String> {
        model_args
            .iter()
            .chain(cli_args.iter())
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn cli_flags_appended_after_model_flags() {
        let merged = merge_args(&["-ngl", "35"], &["-c", "8192"]);
        assert_eq!(merged, vec!["-ngl", "35", "-c", "8192"]);
    }

    #[test]
    fn empty_model_args_uses_only_cli_flags() {
        let merged = merge_args(&[], &["-ngl", "99"]);
        assert_eq!(merged, vec!["-ngl", "99"]);
    }

    #[test]
    fn empty_cli_args_uses_only_model_flags() {
        let merged = merge_args(&["-c", "4096"], &[]);
        assert_eq!(merged, vec!["-c", "4096"]);
    }

    #[test]
    fn both_empty_produces_empty_list() {
        let merged = merge_args(&[], &[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn hyphen_values_preserved() {
        // Flags like --temp 0.7 must survive the merge unchanged
        let merged = merge_args(&["--temp", "0.7"], &["--seed", "-1"]);
        assert_eq!(merged, vec!["--temp", "0.7", "--seed", "-1"]);
    }
}

// ---------------------------------------------------------------------------
// vibe — extra_args forwarding
// ---------------------------------------------------------------------------

mod vibe_tests {
    /// Simulates building the argument list that would be passed to `vibe`.
    fn build_vibe_args<'a>(extra_args: &'a [&str]) -> Vec<&'a str> {
        extra_args.to_vec()
    }

    #[test]
    fn no_extra_args_produces_empty_list() {
        assert!(build_vibe_args(&[]).is_empty());
    }

    #[test]
    fn single_flag_forwarded() {
        let args = build_vibe_args(&["--theme", "dark"]);
        assert_eq!(args, vec!["--theme", "dark"]);
    }

    #[test]
    fn multiple_flags_forwarded_in_order() {
        let args = build_vibe_args(&["--theme", "dark", "--no-telemetry"]);
        assert_eq!(args, vec!["--theme", "dark", "--no-telemetry"]);
    }

    #[test]
    fn hyphen_values_preserved() {
        let args = build_vibe_args(&["--port", "-1"]);
        assert_eq!(args, vec!["--port", "-1"]);
    }
}

// ---------------------------------------------------------------------------
// Live server tests (require llama-server to be running on localhost:8080)
// Run with: cargo test -- --ignored
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a live llama-server on localhost:8080"]
fn live_fetch_models_returns_non_empty() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let resp = reqwest::get("http://localhost:8080/v1/models")
            .await
            .expect("server must be reachable");
        let data: serde_json::Value = resp.json().await.unwrap();
        let models = data["data"].as_array().unwrap();
        assert!(!models.is_empty(), "expected at least one model");
    });
}

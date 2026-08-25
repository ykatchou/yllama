use anyhow::Result;
use std::path::PathBuf;

use crate::{config::Config, vibe_config};

/// A named sampling + reasoning bundle exposed as its own LiteLLM alias.
///
/// llama.cpp keeps `enable_thinking` and the sampler settings on the *request*,
/// not the loaded weights, so one running server can serve both presets. Each
/// preset becomes a `model_name` the client picks from its own model list —
/// that makes the model picker in Claude Code / pi the mode switch, with no
/// server restart and no reload of the GGUF.
struct Preset {
    /// Suffix appended to the upstream model id to form the alias.
    suffix: &'static str,
    /// `enable_thinking` value forwarded to the chat template.
    thinking: bool,
    /// Reasoning effort, only meaningful when `thinking` is true. The Qwen3.x
    /// template accepts `xhigh` | `medium` | `low` and raises on anything else.
    reasoning_effort: Option<&'static str>,
    temperature: f64,
    top_p: f64,
    top_k: i32,
    min_p: f64,
    presence_penalty: f64,
    repeat_penalty: f64,
}

/// Qwen3.x recommended presets. Thinking mode wants high entropy (long CoT
/// self-corrects, and reasoning legitimately repeats terms, so no presence
/// penalty). Instruct mode has no reasoning pass to catch a wrong turn, so it
/// samples tighter and leans on a presence penalty to stop long-form looping.
const PRESETS: &[Preset] = &[
    Preset {
        suffix: "think",
        thinking: true,
        reasoning_effort: Some("xhigh"),
        temperature: 1.0,
        top_p: 0.95,
        top_k: 20,
        min_p: 0.0,
        presence_penalty: 0.0,
        repeat_penalty: 1.0,
    },
    Preset {
        suffix: "instruct",
        thinking: false,
        reasoning_effort: None,
        temperature: 0.7,
        top_p: 0.80,
        top_k: 20,
        min_p: 0.0,
        presence_penalty: 1.5,
        repeat_penalty: 1.0,
    },
];

pub async fn run(cfg: &Config, output: Option<PathBuf>) -> Result<()> {
    let base_url = format!("http://{}:{}", cfg.host, cfg.port);
    println!("Querying llama.cpp at {base_url}/v1/models ...");
    let models = vibe_config::fetch_models(&base_url).await?;
    if models.is_empty() {
        anyhow::bail!("No models found on the server. Is llama-server running? Start it with `yllama serve`.");
    }

    let dest = output.unwrap_or_else(|| PathBuf::from("litellm_config.yaml"));
    let content = build_litellm_config(&base_url, &models);
    std::fs::write(&dest, &content)?;
    println!(
        "Written {} model(s) x {} alias(es) to {}",
        models.len(),
        PRESETS.len() + 1,
        dest.display()
    );
    println!();
    println!("Start the proxy with:");
    println!("  litellm --config {}", dest.display());
    println!();
    println!("Then switch modes from the client's model picker, e.g.:");
    if let Some(id) = models[0]["id"].as_str() {
        let base = alias_base(id);
        for p in PRESETS {
            println!("  /model {base}-{}", p.suffix);
        }
    }
    Ok(())
}

/// llama.cpp reports the loaded model's id as its full filesystem path (e.g.
/// `/Users/me/.yllama/models/Qwen3.8-27B-UD-Q4_K_M.gguf`). That is unusable as
/// a LiteLLM `model_name`, since the alias has to be typeable in a client's
/// `/model` picker. Reduce it to the bare file stem; ids that are not paths are
/// passed through untouched. The server accepts any `model` field on the
/// request (it only ever has one set of weights loaded), so the shortened name
/// is safe to send upstream too.
fn alias_base(id: &str) -> &str {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .strip_suffix(".gguf")
        .unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id))
}

/// Emit the plain passthrough alias plus one alias per preset, for every model
/// the server reports.
fn build_litellm_config(base_url: &str, models: &[serde_json::Value]) -> String {
    let mut lines = vec!["model_list:".to_string()];

    for m in models {
        let base = alias_base(m["id"].as_str().unwrap_or("unknown"));

        // Plain passthrough: server-side defaults, no sampling overrides.
        lines.push(format!("  - model_name: {base}"));
        lines.push("    litellm_params:".to_string());
        lines.push(format!("      model: openai/{base}"));
        lines.push(format!("      api_base: {base_url}/v1"));
        lines.push("      api_key: \"none\"".to_string());

        for p in PRESETS {
            lines.push(format!("  - model_name: {base}-{}", p.suffix));
            lines.push("    litellm_params:".to_string());
            lines.push(format!("      model: openai/{base}"));
            lines.push(format!("      api_base: {base_url}/v1"));
            lines.push("      api_key: \"none\"".to_string());
            // OpenAI-standard params: safe at the top level, where LiteLLM
            // recognises them and `drop_params` leaves them alone.
            lines.push(format!("      temperature: {:?}", p.temperature));
            lines.push(format!("      top_p: {:?}", p.top_p));
            lines.push(format!("      presence_penalty: {:?}", p.presence_penalty));
            // Everything llama.cpp-specific goes in extra_body — `drop_params`
            // would strip these as unknown if they sat at the top level.
            lines.push("      extra_body:".to_string());
            lines.push(format!("        top_k: {}", p.top_k));
            lines.push(format!("        min_p: {:?}", p.min_p));
            lines.push(format!("        repeat_penalty: {:?}", p.repeat_penalty));
            if let Some(effort) = p.reasoning_effort {
                lines.push(format!("        reasoning_effort: {effort}"));
            }
            lines.push("        chat_template_kwargs:".to_string());
            lines.push(format!("          enable_thinking: {}", p.thinking));
        }
    }

    lines.push(String::new());
    lines.push("litellm_settings:".to_string());
    lines.push("  drop_params: true".to_string());
    lines.push(String::new());

    lines.join("\n")
}

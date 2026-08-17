use anyhow::{bail, Result};
use serde::Deserialize;
use reqwest::Client;
use dialoguer::{theme::ColorfulTheme, Select};

#[derive(Debug, Deserialize)]
pub struct HfModel {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct HfFile {
    pub path: String,
}

pub async fn search_models(query: &str) -> Result<Vec<HfModel>> {
    let client = Client::new();
    let url = format!(
        "https://huggingface.co/api/models?search={}&sort=downloads&direction=-1",
        query
    );
    let response = client.get(url).send().await?;
    // The HF API returns a JSON array directly (not a wrapped object).
    let models: Vec<HfModel> = response.json().await?;
    Ok(models)
}

/// Fetch all GGUF file paths in a HF repo. Returns empty vec if repo not found.
pub async fn list_gguf_files(model_id: &str) -> Result<Vec<String>> {
    let client = Client::new();
    let url = format!("https://huggingface.co/api/models/{}/tree/main", model_id);
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Ok(vec![]);
    }
    let files: Vec<HfFile> = response.json().await?;
    let gguf_paths: Vec<String> = files
        .into_iter()
        .filter(|f| f.path.ends_with(".gguf"))
        .map(|f| f.path)
        .collect();
    Ok(gguf_paths)
}

/// Return a download URL for the chosen GGUF file in `model_id`.
/// If the repo has no GGUF files, also tries `{model_id}-GGUF` and a HF search
/// for GGUF variants and asks the user to confirm a different repo.
pub async fn pick_gguf_url(model_id: &str) -> Result<String> {
    let mut gguf_files = list_gguf_files(model_id).await?;

    // If the original repo has no GGUFs, look for a well-known GGUF variant.
    let mut resolved_id = model_id.to_string();
    if gguf_files.is_empty() {
        let gguf_variant = format!("{}-GGUF", model_id);
        let variant_files = list_gguf_files(&gguf_variant).await?;
        if !variant_files.is_empty() {
            println!(
                "No GGUF files in '{}'. Found GGUF repo: '{}'.",
                model_id, gguf_variant
            );
            resolved_id = gguf_variant;
            gguf_files = variant_files;
        } else {
            // Fall back to a keyword search on HF
            println!("No GGUF files found. Searching Hugging Face for GGUF variants…");
            let model_name = model_id.split('/').last().unwrap_or(model_id);
            let candidates = search_models(&format!("{} GGUF", model_name)).await?;
            let gguf_candidates: Vec<HfModel> = candidates
                .into_iter()
                .filter(|m| m.id.to_lowercase().contains("gguf"))
                .collect();

            if gguf_candidates.is_empty() {
                bail!("No GGUF variant found for '{}'", model_id);
            }

            let options: Vec<String> = gguf_candidates.iter().map(|m| m.id.clone()).collect();
            let sel = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select a GGUF repo")
                .items(&options)
                .default(0)
                .interact()?;

            resolved_id = gguf_candidates[sel].id.clone();
            gguf_files = list_gguf_files(&resolved_id).await?;
            if gguf_files.is_empty() {
                bail!("No GGUF files found in '{}'", resolved_id);
            }
        }
    }

    // Let the user pick the quantisation when more than one file is available.
    let path = if gguf_files.len() == 1 {
        gguf_files.into_iter().next().unwrap()
    } else {
        let prompt = if model_id == resolved_id {
            format!("Select a GGUF file from '{}'", resolved_id)
        } else {
            format!("Select a GGUF file from '{}' ({} variants found)", resolved_id, gguf_files.len())
        };
        let sel = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .items(&gguf_files)
            .default(0)
            .interact()?;
        gguf_files[sel].clone()
    };

    Ok(format!(
        "https://huggingface.co/{}/resolve/main/{}",
        resolved_id, path
    ))
}

/// Extract `owner/repo` from a HF resolve URL like
/// `https://huggingface.co/owner/repo/resolve/main/file.gguf`.
pub fn repo_id_from_hf_url(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("https://huggingface.co/")
        .or_else(|| url.strip_prefix("http://huggingface.co/"))?;
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    if parts.len() >= 2 {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

/// Look for an MTP-enabled variant of `main_filename` in the conventional
/// `<repo>-MTP-GGUF` sibling repo — the pattern used by quantizers (unsloth,
/// ggml-org, ...) for architectures (e.g. Qwen3.6) whose main quants don't
/// include the MTP head. These sibling repos mirror the main repo's filenames
/// exactly but with the MTP tensors grafted in, so the same quant can be
/// loaded standalone with `--spec-type draft-mtp` — no separate draft file
/// or `--model-draft` flag involved.
///
/// Returns the sibling repo's download URL for the matching quant, if found.
pub async fn find_mtp_variant_url(repo_id: &str, main_filename: &str) -> Result<Option<String>> {
    if repo_id.ends_with("-MTP-GGUF") {
        return Ok(None); // already the MTP variant
    }
    let variant_id = match repo_id.strip_suffix("-GGUF") {
        Some(base) => format!("{base}-MTP-GGUF"),
        None => format!("{repo_id}-MTP-GGUF"),
    };
    let files = list_gguf_files(&variant_id).await?;
    if files.iter().any(|f| f == main_filename) {
        Ok(Some(format!(
            "https://huggingface.co/{}/resolve/main/{}",
            variant_id, main_filename
        )))
    } else {
        Ok(None)
    }
}

/// Filename substrings (checked case-insensitively) that identify a
/// speculative-decoding "drafter" GGUF, mapped to the `--spec-type` value
/// llama-server expects for that drafter architecture. Order matters: more
/// specific keywords are checked first.
const DRAFTER_KEYWORDS: &[(&str, &str)] = &[
    ("dflash", "draft-dflash"),
    ("dspark", "draft-dspark"),
    ("eagle3", "draft-eagle3"),
    ("mtp", "draft-mtp"),
];

/// Look for a small speculative-decoding drafter GGUF bundled in the same HF
/// repo as the main model — e.g. Muse-Glimmer-30B-GGUF ships
/// `dflash-kquant.gguf` alongside its regular quants. This is a genuinely
/// separate (much smaller) file paired via `--model-draft`, distinct from MTP
/// heads baked into the main file. `mmproj-*` files (vision projectors) are
/// skipped since they're unrelated.
///
/// Returns `(download_url, filename, spec_type)` for the drafter if found.
pub async fn find_drafter_in_repo(
    repo_id: &str,
    main_filename: &str,
) -> Result<Option<(String, String, String)>> {
    let files = list_gguf_files(repo_id).await?;
    for file in files {
        if file == main_filename {
            continue;
        }
        let lower = file.to_lowercase();
        if lower.contains("mmproj") {
            continue;
        }
        if let Some((_, spec_type)) = DRAFTER_KEYWORDS.iter().find(|(kw, _)| lower.contains(kw)) {
            return Ok(Some((
                format!("https://huggingface.co/{}/resolve/main/{}", repo_id, file),
                file,
                spec_type.to_string(),
            )));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::repo_id_from_hf_url;

    #[test]
    fn repo_id_parsed_from_resolve_url() {
        assert_eq!(
            repo_id_from_hf_url(
                "https://huggingface.co/unsloth/Qwen3.6-35B-A3B-GGUF/resolve/main/Qwen3.6-35B-A3B-UD-IQ2_XXS.gguf"
            ),
            Some("unsloth/Qwen3.6-35B-A3B-GGUF".to_string())
        );
    }

    #[test]
    fn repo_id_none_for_non_hf_url() {
        assert_eq!(repo_id_from_hf_url("https://example.com/model.gguf"), None);
    }
}

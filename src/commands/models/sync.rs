use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::commands::models::{download, hf_search};
use crate::gguf;
use crate::manifest;

/// Backfill MTP / speculative-decoding detection for models registered
/// before this support existed. For a model that isn't downloaded yet and
/// has an MTP-enabled variant available, offers to fetch that variant
/// instead of the plain one. For any model with a bundled drafter file
/// (e.g. a DFlash/DSpark/MTP companion GGUF in the same repo), offers to
/// download it alongside the already-downloaded main file.
pub async fn run() -> Result<()> {
    let mut entries = manifest::load()?;
    if entries.is_empty() {
        println!("No models registered. Use `yllama models add <hf-url>` to add one.");
        return Ok(());
    }

    let mut checked = 0usize;
    let mut needs_download: Vec<String> = Vec::new();

    for i in 0..entries.len() {
        if entries[i].mtp_checked || manifest::is_local(&entries[i]) {
            continue;
        }
        checked += 1;

        let name = entries[i].name.clone();
        let hf_url = entries[i].hf_url.clone();
        let filename = entries[i].filename.clone();
        let downloaded = entries[i].downloaded;

        println!("Checking '{name}' for MTP / speculative decoding support...");
        let mut mtp_builtin = gguf::has_builtin_mtp(&hf_url).await.unwrap_or(false);
        let repo_id = hf_search::repo_id_from_hf_url(&hf_url);

        if mtp_builtin {
            println!("  MTP: built-in heads detected.");
        } else if let Some(repo_id) = &repo_id {
            if let Some(variant_url) = hf_search::find_mtp_variant_url(repo_id, &filename)
                .await
                .unwrap_or(None)
            {
                if downloaded {
                    println!(
                        "  MTP: an MTP-enabled variant is available for '{name}', but it's \
                         already downloaded. Re-run `yllama models add <mtp-url>` under a new \
                         name to switch."
                    );
                } else {
                    println!(
                        "  MTP: an MTP-enabled variant of '{name}' is available \
                         (self-speculative decoding, same size)."
                    );
                    let use_variant = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!("Use the MTP-enabled variant for '{name}'?"))
                        .default(true)
                        .interact()?;
                    if use_variant {
                        entries[i].hf_url = variant_url;
                        mtp_builtin = true;
                        needs_download.push(name.clone());
                    }
                }
            }
        }

        if !mtp_builtin {
            let draft = match &repo_id {
                Some(repo_id) => hf_search::find_drafter_in_repo(repo_id, &filename)
                    .await
                    .unwrap_or(None),
                None => None,
            };
            match draft {
                Some((draft_url, draft_filename, spec_type)) => {
                    println!(
                        "  Speculative decoding: found drafter '{draft_filename}' ({spec_type})."
                    );
                    let fetch_it = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!("Download the drafter for '{name}'?"))
                        .default(true)
                        .interact()?;
                    if fetch_it {
                        entries[i].draft_url = Some(draft_url);
                        entries[i].draft_filename = Some(draft_filename);
                        entries[i].draft_spec_type = Some(spec_type);
                        needs_download.push(name.clone());
                    }
                }
                None => println!("  MTP / speculative decoding: not available."),
            }
        }

        entries[i].mtp_builtin = mtp_builtin;
        entries[i].mtp_checked = true;
        manifest::save(&entries)?;
    }

    for name in &needs_download {
        println!("Downloading for '{name}'...");
        download::run(name).await?;
    }

    if checked == 0 {
        println!("Everything already checked — no missing MTP info.");
    } else {
        println!(
            "\nChecked {checked} model(s) for MTP / speculative decoding support, \
             fetched files for {} of them.",
            needs_download.len()
        );
    }

    Ok(())
}

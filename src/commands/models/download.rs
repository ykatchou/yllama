use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use tokio::io::AsyncWriteExt;

use crate::manifest;

pub async fn run(name: &str) -> Result<()> {
    let mut entries = manifest::load()?;
    let entry = manifest::find(&entries, name)
        .ok_or_else(|| {
            anyhow::anyhow!("Model '{name}' not found. Run `yllama models add <url>` first.")
        })?
        .clone();

    let dest = manifest::model_path(&entry);
    if let Some(source) = &entry.local_source {
        println!("Model '{name}' was registered from a local file ({source}) — nothing to download.");
        return Ok(());
    }
    if entry.downloaded && dest.exists() {
        println!(
            "Model '{name}' is already downloaded at {}",
            dest.display()
        );
    } else {
        let size =
            download_with_resume(&entry.hf_url, &dest, &entry.name, entry.size_bytes).await?;
        for e in entries.iter_mut() {
            if e.name == name {
                e.downloaded = true;
                e.size_bytes = Some(size);
            }
        }
        manifest::save(&entries)?;
    }

    if let (Some(draft_url), Some(draft_filename)) =
        (entry.draft_url.clone(), entry.draft_filename.clone())
    {
        if !entry.draft_downloaded {
            let draft_dest = manifest::models_dir().join(&draft_filename);
            println!("\nDownloading drafter '{draft_filename}'...");
            download_with_resume(&draft_url, &draft_dest, &draft_filename, None).await?;
            for e in entries.iter_mut() {
                if e.name == name {
                    e.draft_downloaded = true;
                }
            }
            manifest::save(&entries)?;
        }
    }

    Ok(())
}

/// Download `url` to `dest` with a progress bar, resuming from a `.gguf.tmp`
/// sibling file if one exists from an interrupted previous attempt.
/// `known_total`, when available, is used to report bytes remaining on resume.
async fn download_with_resume(
    url: &str,
    dest: &Path,
    label: &str,
    known_total: Option<u64>,
) -> Result<u64> {
    let tmp_path = dest.with_extension("gguf.tmp");

    if tmp_path.exists() {
        if let Ok(meta) = tokio::fs::metadata(&tmp_path).await {
            let msg = match known_total {
                Some(total) => format!(
                    "Resuming '{label}' — {} already downloaded, {} remaining",
                    manifest::format_bytes(meta.len()),
                    manifest::format_bytes(total.saturating_sub(meta.len()))
                ),
                None => format!(
                    "Resuming '{label}' — {} already downloaded",
                    manifest::format_bytes(meta.len())
                ),
            };
            println!("{msg}");
        }
    }

    if dest.exists() {
        println!("'{label}' is already downloaded at {}", dest.display());
        return Ok(tokio::fs::metadata(dest).await?.len());
    }

    println!("Downloading '{label}' from {url}");

    let client = reqwest::Client::builder()
        .user_agent("yllama/0.1")
        .build()?;

    let mut request = client.get(url);
    let resume_offset = if tmp_path.exists() {
        tokio::fs::metadata(&tmp_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    if resume_offset > 0 {
        request = request.header("Range", format!("bytes={}-", resume_offset));
    }

    let resp = request.send().await.with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        bail!("Download failed: HTTP {}", resp.status());
    }

    // If resuming, the server returns 206 with Content-Range
    let total = if resume_offset > 0 && resp.status().as_u16() == 206 {
        resp.content_length().unwrap_or(0) + resume_offset
    } else {
        resp.content_length().unwrap_or(0)
    };

    let total_downloaded = resume_offset;
    let pb = if total > 0 {
        let pb = ProgressBar::new(total - total_downloaded);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{msg}\n[{bar:50.cyan/blue}] {bytes}/{total_bytes} \
                     ({bytes_per_sec}, eta {eta})",
                )
                .unwrap()
                .progress_chars("=> "),
        );
        pb.set_message(format!("Downloading {label}"));
        pb
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} {msg}  {bytes} downloaded")
                .unwrap(),
        );
        pb.set_message(format!("Downloading {label}"));
        pb
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = if resume_offset > 0 {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tmp_path)
            .await
            .with_context(|| format!("opening {}", tmp_path.display()))?
    } else {
        tokio::fs::File::create(&tmp_path)
            .await
            .with_context(|| format!("creating {}", tmp_path.display()))?
    };

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading response chunk")?;
        pb.inc(chunk.len() as u64);
        file.write_all(&chunk).await?;
    }
    drop(file);

    tokio::fs::rename(&tmp_path, dest).await?;

    let size = tokio::fs::metadata(dest).await?.len();
    pb.finish_with_message(format!(
        "Saved {} ({:.1} GB)",
        dest.display(),
        size as f64 / 1_073_741_824.0
    ));

    Ok(size)
}

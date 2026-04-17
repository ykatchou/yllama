use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
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
    if entry.downloaded && dest.exists() {
        println!(
            "Model '{name}' is already downloaded at {}",
            dest.display()
        );
        return Ok(());
    }

    println!("Downloading '{}' from {}", entry.name, entry.hf_url);

    let client = reqwest::Client::builder()
        .user_agent("yllama/0.1")
        .build()?;
    let resp = client
        .get(&entry.hf_url)
        .send()
        .await
        .with_context(|| format!("GET {}", entry.hf_url))?;

    if !resp.status().is_success() {
        bail!("Download failed: HTTP {}", resp.status());
    }

    let total = resp.content_length();
    let pb = if let Some(len) = total {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{msg}\n[{bar:50.cyan/blue}] {bytes}/{total_bytes} \
                     ({bytes_per_sec}, eta {eta})",
                )
                .unwrap()
                .progress_chars("=> "),
        );
        pb.set_message(format!("Downloading {name}"));
        pb
    } else {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} {msg}  {bytes} downloaded")
                .unwrap(),
        );
        pb.set_message(format!("Downloading {name}"));
        pb
    };

    std::fs::create_dir_all(manifest::models_dir())?;
    let tmp_path = dest.with_extension("gguf.tmp");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("creating {}", tmp_path.display()))?;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading response chunk")?;
        pb.inc(chunk.len() as u64);
        file.write_all(&chunk).await?;
    }
    drop(file);

    tokio::fs::rename(&tmp_path, &dest).await?;

    let size = tokio::fs::metadata(&dest).await?.len();
    pb.finish_with_message(format!(
        "Saved {} ({:.1} GB)",
        dest.display(),
        size as f64 / 1_073_741_824.0
    ));

    for e in entries.iter_mut() {
        if e.name == name {
            e.downloaded = true;
            e.size_bytes = Some(size);
        }
    }
    manifest::save(&entries)?;
    Ok(())
}

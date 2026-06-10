use anyhow::Result;

use crate::manifest;

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        return url.to_string();
    }
    // Show the filename and ellipsize the middle
    let file_end = url.len() - 40.min(url.len() - 1);
    let truncated = &url[..file_end];
    let dotdotdot = "...";
    let url_part = &url[file_end..];
    format!("{}{}", truncated.trim_end_matches('/'), dotdotdot, url_part)
}

pub fn run() -> Result<()> {
    let entries = manifest::load()?;
    if entries.is_empty() {
        println!("No models registered. Use `yllama models add <hf-url>` to add one.");
        return Ok(());
    }

    let name_w = entries.iter().map(|e| e.name.len()).max().unwrap_or(4).max(4);
    let status_w = 14usize;
    let size_w = 10usize;
    let default_w = 9usize; // "*default" or " " padding
    let url_w = 45usize;

    println!(
        "{:<name_w$}  {:<status_w$}  {:<size_w$}  {:<default_w$}  URL",
        "NAME", "STATUS", "SIZE", "DEFAULT",
    );
    println!(
        "{}",
        "-".repeat(name_w + status_w + size_w + default_w + 14)
    );

    for e in &entries {
        let status = if e.downloaded { "downloaded" } else { "not downloaded" };
        let size = e
            .size_bytes
            .map(manifest::format_bytes)
            .unwrap_or_else(|| "-".to_string());
        let default_mark = e.default_model.as_deref().is_some_and(|d| d == e.name);
        let default_str = if default_mark {
            "*default".to_string()
        } else {
            " ".to_string()
        };
        let url_display = truncate_url(&e.hf_url, url_w);
        println!(
            "{:<name_w$}  {:<status_w$}  {:<size_w$}  {:<default_w$}  {}",
            e.name, status, size, default_str, url_display,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(crate::manifest::format_bytes(2_147_483_648), "2.0 GB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(crate::manifest::format_bytes(5_242_880), "5.0 MB");
    }

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(crate::manifest::format_bytes(512), "512 B");
    }
}

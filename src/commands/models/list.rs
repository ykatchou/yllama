use anyhow::Result;

use crate::manifest;

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
    let spec_w = 16usize;

    println!(
        "{:<name_w$}  {:<status_w$}  {:<size_w$}  {:<default_w$}  {:<spec_w$}  URL",
        "NAME", "STATUS", "SIZE", "DEFAULT", "SPEC-DECODE",
    );
    println!(
        "{}",
        "-".repeat(name_w + status_w + size_w + default_w + spec_w + 16)
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
        let spec_str = if e.mtp_builtin {
            "mtp".to_string()
        } else if e.draft_downloaded {
            e.draft_spec_type
                .clone()
                .unwrap_or_else(|| "drafter".to_string())
        } else if e.mtp_checked {
            "-".to_string()
        } else {
            "unchecked".to_string()
        };
        println!(
            "{:<name_w$}  {:<status_w$}  {:<size_w$}  {:<default_w$}  {:<spec_w$}  {}",
            e.name, status, size, default_str, spec_str, e.hf_url,
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

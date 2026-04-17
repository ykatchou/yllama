use anyhow::Result;

use crate::manifest;

pub fn run() -> Result<()> {
    let entries = manifest::load()?;
    if entries.is_empty() {
        println!("No models registered. Use `yllama models add <hf-url>` to add one.");
        return Ok(());
    }

    let name_w = entries.iter().map(|e| e.name.len()).max().unwrap_or(4).max(4);
    println!(
        "{:<name_w$}  {:<14}  {:<10}  URL",
        "NAME",
        "STATUS",
        "SIZE",
    );
    println!("{}", "-".repeat(name_w + 14 + 10 + 6 + 40));

    for e in &entries {
        let status = if e.downloaded { "downloaded" } else { "not downloaded" };
        let size = e
            .size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<name_w$}  {:<14}  {:<10}  {}",
            e.name, status, size, e.hf_url,
        );
    }
    Ok(())
}

fn format_bytes(b: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    if b >= GB {
        format!("{:.1} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else {
        format!("{b} B")
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(format_bytes(2_147_483_648), "2.0 GB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(format_bytes(5_242_880), "5.0 MB");
    }

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512 B");
    }
}

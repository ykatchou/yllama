use anyhow::Result;

use crate::manifest;

pub fn run(name: &str) -> Result<()> {
    let mut entries = manifest::load()?;
    let pos = entries
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| anyhow::anyhow!("Model '{name}' not found in manifest."))?;

    let path = manifest::model_path(&entries[pos]);
    if path.exists() {
        std::fs::remove_file(&path)?;
        println!("Deleted {}", path.display());
    } else {
        println!("No file on disk (already removed or never downloaded).");
    }

    entries.remove(pos);
    manifest::save(&entries)?;
    println!("Removed '{name}' from manifest.");
    Ok(())
}

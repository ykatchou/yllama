use anyhow::Result;

use crate::manifest;

pub fn run(name: &str) -> Result<()> {
    let mut entries = manifest::load()?;
    let pos = entries
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| anyhow::anyhow!("Model '{name}' not found in manifest."))?;

    let path = manifest::model_path(&entries[pos]);
    // `symlink_metadata` so a link whose target is gone still gets cleaned up.
    if std::fs::symlink_metadata(&path).is_ok() {
        std::fs::remove_file(&path)?;
        println!("Deleted {}", path.display());
    } else {
        println!("No file on disk (already removed or never downloaded).");
    }

    if let Some(source) = &entries[pos].local_source {
        println!("Left the original file untouched: {source}");
    }

    if let Some(draft_path) = manifest::draft_model_path(&entries[pos]) {
        if draft_path.exists() {
            std::fs::remove_file(&draft_path)?;
            println!("Deleted {}", draft_path.display());
        }
    }

    entries.remove(pos);
    manifest::save(&entries)?;
    println!("Removed '{name}' from manifest.");
    Ok(())
}

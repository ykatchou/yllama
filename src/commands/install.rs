use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn run(bin_dir: Option<PathBuf>) -> Result<()> {
    let current_exe = std::env::current_exe().context("cannot determine current executable path")?;

    let target_dir = bin_dir.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("no home dir")
            .join(".local")
            .join("bin")
    });

    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("creating {}", target_dir.display()))?;

    let target = target_dir.join("yllama");
    std::fs::copy(&current_exe, &target)
        .with_context(|| format!("copying to {}", target.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
    }

    println!("Installed yllama to {}", target.display());

    // Hint if the target dir isn't on PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let on_path = path_var
            .split(':')
            .any(|p| PathBuf::from(p) == target_dir);
        if !on_path {
            println!();
            println!(
                "Note: {} is not on your PATH. Add this to your shell profile:",
                target_dir.display()
            );
            println!("  export PATH=\"{}:$PATH\"", target_dir.display());
        }
    }

    Ok(())
}

use anyhow::{bail, Result};

/// Check whether a binary is available on PATH.
/// Returns the path if found, or an error with install instructions.
pub fn check_binary(name: &str) -> Result<String> {
    which::which(name)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| {
            anyhow::anyhow!(
                "`{name}` not found on PATH.\n\n\
                 {install_msg}",
                install_msg = install_instructions(name)
            )
        })
}

fn install_instructions(bin: &str) -> String {
    match bin {
        "llama-server" | "llama-cpp" => llama_cpp_instructions(),
        "claude" | "claude-code" => claude_code_instructions(),
        "vibe" => vibe_instructions(),
        _ => format!("Install `{bin}` and ensure it is on your PATH."),
    }
}

fn llama_cpp_instructions() -> String {
    let (platform, install_cmd, extra) = if cfg!(target_os = "macos") {
        (
            "macOS",
            "brew install llama-cpp",
            "\n\nAlternatively, build from source:\n  \
             git clone https://github.com/ggerganov/llama.cpp && cd llama.cpp && cargo build --release --bin llama-server\n  \
             mv target/release/llama-server ~/.local/bin/",
        )
    } else if cfg!(target_os = "linux") {
        (
            "Linux",
            "# apt (Debian/Ubuntu)\n  sudo apt install llama.cpp\n# or\n  # dnf (Fedora)\n  sudo dnf install llama.cpp",
            "\n\nAlternatively, build from source:\n  \
             git clone https://github.com/ggerganov/llama.cpp && cd llama.cpp && cargo build --release --bin llama-server\n  \
             sudo mv target/release/llama-server /usr/local/bin/",
        )
    } else {
        (
            "your OS",
            "install llama.cpp",
            "",
        )
    };

    format!(
        "llama.cpp ({platform}):\n\
         \n  {install_cmd}\n{extra}\n\
         \n  Verify: `llama-server --help` should display usage info."
    )
}

fn claude_code_instructions() -> String {
    if cfg!(target_os = "macos") {
        format!(
            "Claude Code:\n\
             \n  npm install -g @anthropic-ai/claude-code\n\
             \n  or via Homebrew:\n  \n  brew install Anthropic/claude-code/claude-code\n\
             \n  Verify: `claude --version` should show the installed version."
        )
    } else if cfg!(target_os = "linux") {
        format!(
            "Claude Code (Linux):\n\
             \n  npm install -g @anthropic-ai/claude-code\n\
             \n  Ensure ~/.npm-global/bin (or your npm prefix) is on your PATH:\n  \n  export PATH=\"$(npm config get prefix)/bin:$PATH\"\n\
             \n  Verify: `claude --version` should show the installed version."
        )
    } else {
        format!(
            "Claude Code:\n  \n  npm install -g @anthropic-ai/claude-code\n\
             \n  Verify: `claude --version` should show the installed version."
        )
    }
}

fn vibe_instructions() -> String {
    if cfg!(target_os = "macos") {
        format!(
            "Vibe:\n\
             \n  Vibe is a macOS-native app. Install it from the Mac App Store or the official website.\n\
             \n  After installation, ensure the app is launched at least once so its binary is registered.\n\
             \n  If using a symlink or custom path, make sure the binary is accessible:\n  \n  ln -s \"/Applications/Vibe.app/Contents/MacOS/Vibe\" ~/.local/bin/vibe"
        )
    } else {
        format!(
            "Vibe:\n  \n  Install Vibe from the official website and ensure its binary is on your PATH.\n\
             \n  If using a custom install path, create a symlink:\n  \n  ln -s /path/to/vibe ~/.local/bin/vibe"
        )
    }
}

/// Check all known yllama dependencies and return a combined error if any are missing.
#[allow(dead_code)]
pub fn check_all() -> Result<()> {
    let mut missing = Vec::new();

    if check_binary("llama-server").is_err() {
        missing.push("llama-server");
    }
    if check_binary("claude").is_err() {
        // Also check the alias
        if check_binary("claude-code").is_err() {
            missing.push("claude");
        }
    }
    // vibe is optional — only check if we're about to use it
    let _ = check_binary("vibe");

    if missing.is_empty() {
        Ok(())
    } else {
        let msg = missing
            .iter()
            .map(|b| format!("  - {b}"))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Missing dependencies:\n{msg}\n\n\
             Install them before using yllama integrations (vibe, claude).\n\
             Run `yllama serve` first — it only requires llama-server."
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_install_instructions_format() {
        // Just verify the functions don't panic
        let _ = crate::deps::llama_cpp_instructions();
        let _ = crate::deps::claude_code_instructions();
        let _ = crate::deps::vibe_instructions();
    }
}

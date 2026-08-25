# Windows Self-Contained Package Plan

Goal: produce a `dist/windows/` folder with `yllama.exe` + bundled `llama-server.exe`, launchable by double-click, no external deps for the end-user. User downloads GGUF models themselves.

---

## Phase 1 — Fix Windows Source Compatibility

Several files use Unix-only APIs. All fixes use `#[cfg(unix)]` / `#[cfg(windows)]` guards.

### `Cargo.toml`
Move `libc` to Unix-only:
```toml
# remove from [dependencies]
# add:
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

### `src/llamacpp.rs`

**`spawn_daemon`** — replace `pre_exec(setsid)` with platform branch:
```rust
#[cfg(unix)]
{
    use std::os::unix::process::CommandExt;
    unsafe { cmd.pre_exec(|| { libc::setsid(); Ok(()) }); }
}
#[cfg(windows)]
{
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS (0x8) | CREATE_NEW_PROCESS_GROUP (0x200)
    cmd.creation_flags(0x00000008 | 0x00000200);
}
```

**`kill_server`** — replace `Command::new("kill")`:
```rust
#[cfg(unix)]
Command::new("kill").arg(pid.to_string())...

#[cfg(windows)]
Command::new("taskkill").args(["/F", "/PID", &pid.to_string()])...
```

### `src/commands/attach.rs`
- Process-alive check: `kill -0` → `tasklist /FI "PID eq <n>"` on Windows
- `tail -f`: no `tail` on Windows → Rust polling loop:
  ```rust
  #[cfg(windows)]
  {
      let mut file = std::fs::File::open(&log_path)?;
      file.seek(SeekFrom::End(0))?;
      loop {
          let mut buf = String::new();
          file.read_to_string(&mut buf)?;
          if !buf.is_empty() { print!("{buf}"); }
          std::thread::sleep(std::time::Duration::from_millis(300));
      }
  }
  ```

### `src/commands/vibe.rs` and `src/commands/claude_code.rs`
Both use `CommandExt::exec()` (Unix process-replace). Guard it:
```rust
#[cfg(unix)]
{ use std::os::unix::process::CommandExt; cmd.exec(); }

#[cfg(windows)]
{ let s = cmd.status()?; std::process::exit(s.code().unwrap_or(1)); }
```

### `src/commands/install.rs`
Replace `path_var.split(':')` with `std::env::split_paths(&path_var)` (cross-platform).

---

## Phase 2 — Cross-Compile `yllama.exe`

**Target**: `x86_64-pc-windows-msvc` (self-contained `.exe`, no MinGW DLLs)

**Tooling**: `cargo-xwin` (downloads Windows SDK automatically, no VM/Docker needed)

```bash
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc
cargo xwin build --release --target x86_64-pc-windows-msvc
# Output: target/x86_64-pc-windows-msvc/release/yllama.exe
```

**Fallback**: GitHub Actions `windows-latest` runner if cross-compilation is blocked.

---

## Phase 3 — Bundle `llama-server.exe`

**Source**: llama.cpp GitHub releases
**Asset**: `llama-b<N>-bin-win-avx2-x64.zip`
- AVX2 = CPU-only, no CUDA/Vulkan, works on any x86-64 since ~2013
- ZIP contains `llama-server.exe` + `ggml.dll` + `llama.dll` (all three needed)

---

## Phase 4 — Package Structure

```
dist/windows/
├── yllama.exe          # compiled Rust binary
├── llama-server.exe    # from llama.cpp release ZIP
├── ggml.dll            # companion DLL (same ZIP)
├── llama.dll           # companion DLL (same ZIP)
├── run.bat             # double-click → cmd prompt with PATH set
├── yllama-pi.bat       # double-click → yllama pi directly
└── README.txt
```

**`run.bat`**:
```bat
@echo off
set PATH=%~dp0;%PATH%
echo yllama ready. Commands: yllama serve / yllama pi / yllama models ...
cmd /k
```

**`yllama-pi.bat`**:
```bat
@echo off
set PATH=%~dp0;%PATH%
yllama.exe pi %*
pause
```

---

## Phase 5 — Build Script `scripts/build-windows.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="$(git rev-parse --show-toplevel)/dist/windows"
TARGET="x86_64-pc-windows-msvc"

echo "[1/3] Compiling yllama.exe..."
cargo xwin build --release --target "$TARGET"
mkdir -p "$DIST_DIR"
cp "target/$TARGET/release/yllama.exe" "$DIST_DIR/"

echo "[2/3] Fetching latest llama.cpp release..."
LATEST=$(curl -s https://api.github.com/repos/ggerganov/llama.cpp/releases/latest \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "b\([0-9]*\)".*/\1/')
ASSET="llama-b${LATEST}-bin-win-avx2-x64.zip"
URL="https://github.com/ggerganov/llama.cpp/releases/download/b${LATEST}/${ASSET}"
TMP=$(mktemp -d)
curl -L --progress-bar -o "$TMP/llamacpp.zip" "$URL"
unzip -j "$TMP/llamacpp.zip" "*/llama-server.exe" "*/ggml.dll" "*/llama.dll" -d "$DIST_DIR/"
rm -rf "$TMP"

echo "[3/3] Writing launcher scripts..."
# ... write run.bat, yllama-pi.bat, README.txt

echo "Done: $DIST_DIR"
```

Add to `.gitignore`:
```
dist/windows/*.exe
dist/windows/*.dll
```

---

## Summary of Files to Touch

| File | Change |
|------|--------|
| `Cargo.toml` | Make `libc` Unix-only dependency |
| `src/llamacpp.rs` | `#[cfg]` guards for `setsid` and `kill` |
| `src/commands/attach.rs` | Windows `tasklist` + polling tail |
| `src/commands/vibe.rs` | `exec()` → `spawn+wait+exit` on Windows |
| `src/commands/claude_code.rs` | Same as vibe.rs |
| `src/commands/install.rs` | `split(':')` → `split_paths()` |
| `scripts/build-windows.sh` | New: assembly script |

---

## Notes

- `yllama pi` requires the `pi` npm binary on the Windows machine (`npm i -g @earendil-works/pi-coding-agent`). The `pi.rs` command code itself has no Unix-specific code — works as-is once llama-server is running.
- Vibe/Claude Code commands will compile but fail at runtime on Windows with a clean "binary not found" error — no extra handling needed.
- Daemon robustness: `DETACHED_PROCESS` is good enough for interactive use, not a proper Windows service.

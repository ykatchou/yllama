# yllama

A local AI stack manager for developers: one CLI to download GGUF models, run llama.cpp, and integrate with your favorite AI editor.

```
yllama models add unsloth/Qwen3.6-35B-A3B-GGUF
yllama models download qwen3.6-35b-a3b
yllama serve -ngl 35 -c 8192
```

## What it does

| Workflow          | Before yllama                          | With yllama                   |
|-------------------|----------------------------------------|-------------------------------|
| Get a model       | Search HuggingFace → copy URL → download manually | `yllama models add <url>`     |
| Start inference   | Spawn server, manage PID, health-check | `yllama serve`                |
| Editor integration| Manually sync host/port in every config | `yllama vibe` (auto-starts + syncs) |
| LiteLLM proxy     | Write YAML by hand                     | `yllama litellm`              |

## Prerequisites

yllama delegates inference to **[llama.cpp](https://github.com/ggerganov/llama.cpp)**. Editor integrations require additional tools.

### llama.cpp (`llama-server`)

**macOS**
```bash
brew install llama-cpp
```

**Linux**
```bash
# Debian / Ubuntu
sudo apt install llama.cpp

# Fedora
sudo dnf install llama.cpp
```

**Verify:** `llama-server --help` should display usage info.

### Claude Code (optional — needed for `yllama claude`)

```bash
npm install -g @anthropic-ai/claude-code
```

**Linux:** If npm installs to a custom prefix, add it to your PATH:
```bash
export PATH="$(npm config get prefix)/bin:$PATH"
```

**Verify:** `claude --version` should show the installed version.

### Vibe (optional — needed for `yllama vibe`)

Install [Vibe](https://www.vibe.dev) from the official source. After installation, ensure the binary is on your PATH (create a symlink if needed):
```bash
ln -s "/Applications/Vibe.app/Contents/MacOS/Vibe" ~/.local/bin/vibe
```

---

## Quick start

```bash
# Register, download, and go
yllama models add <hf-url>
yllama models download <name>
yllama serve                        # auto-selects your model
```

Or skip the two-step add/download with `--download`:

```bash
yllama models add <hf-url> --download
yllama serve
```

## Commands

### Models

```
yllama models list                  # show cached models
yllama models add <url>             # register a GGUF model (HF repo URLs, owner/repo, or search)
yllama models add <url> --download  # register and download in one step
yllama models download <name>       # download with progress bar
yllama models delete <name>         # remove from disk and registry
```

### Server

```
yllama start [model] [-- <flags>]   # foreground (debug-friendly)
yllama serve [model] [-- <flags>]   # background daemon
yllama stop                         # kill the running server
yllama attach                       # tail the server log
```

Extra flags are forwarded verbatim to `llama-server` (`-ngl`, `-c`, `--temp`, etc.).

### Integrations

```
yllama vibe [folder] [-- <args>]    # launch Vibe (auto-starts server, syncs config)
yllama claude [folder] [-- <args>]  # launch Claude Code with local llama.cpp
yllama sync                         # sync running server into ~/.vibe/config.toml
yllama litellm [--output <path>]    # generate LiteLLM proxy YAML
```

### Utility

```
yllama install [--dir PATH]         # install the binary to PATH
```

## Model selection

- **0 models** — error with a helpful message
- **1 model** — used automatically
- **default set** — used automatically (no picker)
- **2+ models, no default** — interactive TUI picker

## Configuration

| File                          | Contents                                    |
|-------------------------------|---------------------------------------------|
| `~/.yllama/config.toml`       | server host, port, binary path              |
| `~/.yllama/models/manifest.json` | registered models, download status, extra flags |

## Install

```bash
cargo install --path .
yllama install --dir ~/.local/bin   # or any PATH directory
```

## License

MIT

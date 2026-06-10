# yllama

A CLI tool for managing your local AI stack: **llama.cpp** (inference), **GGUF models**, and **Vibe** (editor).

## Quick Start

```bash
# Register a model from HuggingFace
yllama models add unsloth/Qwen3.6-35B-A3B-GGUF

# Download it
yllama models download qwen3.6-35b-a3b

# Start the server (or use serve for background mode)
yllama start
yllama serve
yllama serve -ngl 35 -c 8192   # GPU layers + context size
```

## Features

- **Model Management**: Register, download, and manage GGUF models from HuggingFace.
- **Interactive Selection**: Multiple models downloaded? Pick one with a terminal menu.
- **Server Management**: Start llama.cpp as a foreground process (for debugging) or background daemon.
- **Vibe Integration**: Launch Vibe with automatic server startup and config sync.
- **Claude Code Integration**: Use Claude Code with your local llama.cpp server.
- **LiteLLM Config**: Generate a LiteLLM proxy config from your running server.

## Commands

### Model Management

```bash
yllama models list                          # List all registered models
yllama models add <url>                     # Register a GGUF URL (supports HF repo URLs, owner/repo shorthand, or search queries)
yllama models download <name>               # Download a registered model with progress bar
yllama models delete <name>                 # Remove a model from cache
```

### Server Management

```bash
yllama serve [model] [-- <flags>]           # Start llama-server as a background daemon
yllama start [model] [-- <flags>]           # Start llama-server in the foreground (for debugging)
yllama stop                                 # Stop the running llama-server
yllama attach                               # Attach to the running server's log output
```

**Extra flags** are forwarded directly to llama-server:

| Category     | Flags                                              |
|-------------|----------------------------------------------------|
| GPU         | `-ngl <N>`, `--split-mode`, `--tensor-split`      |
| Performance | `-t/--threads`, `-b/--batch-size`, `-c/--ctx-size` |
| Generation  | `--temp`, `--top-k`, `--top-p`, `--seed`           |
| Advanced    | `--flash-attn`, `--rope-scaling`, `--kv-offload`   |

See `yllama serve --help` for the full reference.

### AI Editor Integration

```bash
yllama vibe [folder] [-- <vibe-args>]       # Launch Vibe (auto-starts server, syncs config)
yllama claude [folder] [-- <args>]          # Launch Claude Code with local llama.cpp
yllama sync                                 # Sync ~/.vibe/config.toml from the running server
yllama litellm                              # Generate a LiteLLM proxy config
```

### Utilities

```bash
yllama install [--dir PATH]                 # Install yllama binary into a PATH directory
```

## Model Selection

When no model is specified to `serve`, `start`, `vibe`, or `claude`:

- **0 downloaded**: Shows an error.
- **1 downloaded**: Uses it silently.
- **2+ downloaded**: An interactive menu appears — pick a model to use.
  - Choose **"Set as default"** to persist your choice. Future invocations will skip the menu and use the default directly.

## Configuration

Server settings (host, port, llama-server binary path) are stored in `~/.yllama/config.toml`.

Model registry (names, URLs, download status, extra flags) is stored in `~/.yllama/models/manifest.json`.

## Installation

```bash
yllama install                              # Installs yllama to ~/bin (default)
yllama install --dir ~/.local/bin           # Or specify a custom directory
```

## License

MIT

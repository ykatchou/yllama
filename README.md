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

## Quick start

```bash
# Register, download, and go
yllama models add <hf-url>
yllama models download <name>
yllama serve                        # auto-selects your model
```

## Commands

### Models

```
yllama models list                  # show cached models
yllama models add <url>             # register a GGUF model (HF repo URLs, owner/repo, or search)
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
- **2+ models** — interactive TUI picker; choose "Set as default" to persist

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

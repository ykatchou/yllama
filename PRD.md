# yllama — Product Requirements Document

## Overview

`yllama` is a local CLI tool for managing a personal AI stack composed of
llama.cpp (inference server), GGUF model files from HuggingFace, and Vibe
(AI-assisted coding editor).  It eliminates the manual steps of downloading
models, starting servers, and keeping tool configs in sync.

---

## User Stories

### US-1 — Add a model from HuggingFace
> As a developer, I want to register a HuggingFace GGUF model URL so that I
> can later download it with one command.

**Acceptance criteria:**
- `yllama models add <url>` accepts both `/blob/` and `/resolve/` HuggingFace
  URLs and normalises them to `/resolve/` for downloading.
- A short name is derived automatically from the filename (strip `.gguf`).
- An explicit name can be given with `--name <name>`.
- Running the command twice with the same name returns an error instead of
  creating a duplicate.
- The entry is persisted to `~/.yllama/models/manifest.json` immediately.

**Tests:** `url_parsing_tests`, `commands/models/add.rs` unit tests.

---

### US-2 — Download a model with a progress bar
> As a developer, I want to download a registered model with a visible
> progress bar so that I know how long to wait.

**Acceptance criteria:**
- `yllama models download <name>` streams the file to
  `~/.yllama/models/<filename>`.
- A `[================  ] bytes/total (speed, eta)` progress bar is shown when
  `Content-Length` is provided; a spinner is shown otherwise.
- The download is written to a `.tmp` file first and renamed on completion,
  preventing partial files from being mistaken for valid models.
- After download, `manifest.json` is updated: `downloaded = true`,
  `size_bytes = <actual size>`.
- Re-running the command when the file already exists prints a message and
  exits cleanly.

**Tests:** n/a for network (requires live server); file rename logic is
implicit in the implementation.

---

### US-3 — List cached models
> As a developer, I want to see all registered models and their download
> status in a table so that I know what is available locally.

**Acceptance criteria:**
- `yllama models list` prints a formatted table with columns:
  `NAME`, `STATUS` (`downloaded` / `not downloaded`), `SIZE`, `URL`.
- Sizes are shown in human-readable form: `15.3 GB`, `512 MB`, `1024 B`.
- When no models are registered the command prints a helpful hint.

**Tests:** `commands/models/list.rs` unit tests (format_bytes).

---

### US-4 — Delete a model
> As a developer, I want to delete a model from my cache to reclaim disk
> space.

**Acceptance criteria:**
- `yllama models delete <name>` removes the `.gguf` file from disk (if it
  exists) and the entry from `manifest.json`.
- If the file was never downloaded only the manifest entry is removed.
- Deleting a model that does not exist returns a clear error.

**Tests:** `install_tests::installs_binary_to_target_dir` exercises the
file-system copy path; delete follows the same pattern.

---

### US-5 — Start llama-server as a background daemon
> As a developer, I want to start llama-server with a specific model as a
> background process so that I can continue using my terminal.

**Acceptance criteria:**
- `yllama serve [model] [<extra-flags>]` spawns `llama-server` in a new session
  (via `setsid`) so it survives terminal closure.
- The PID is written to `~/.yllama/llamacpp.pid`.
- If no model name is given, the first downloaded model in the manifest is
  used; if none are downloaded an error is returned.
- If the server is already responding on the configured port the command
  prints a message and exits without spawning a second instance.
- After spawning, the command polls `GET /health` until the server is ready
  (timeout 60 s) before returning.
- Extra flags (e.g. `-ngl 35 -c 8192`) are forwarded verbatim to llama-server,
  merged after any model-level `extra_args` from the manifest.

**Tests:** live server test (`#[ignore]` in `integration_test.rs`).

---

### US-5b — Start llama-server in the foreground
> As a developer, I want to start llama-server in the foreground so that I can
> see its output directly and debug issues.

**Acceptance criteria:**
- `yllama start [model] [<extra-flags>]` spawns `llama-server` with inherited
  stdin/stdout/stderr (no daemonisation).
- The PID is written to `~/.yllama/llamacpp.pid` so `yllama stop` still works.
- Extra flags are forwarded identically to `yllama serve`.
- The process is waited on; when it exits the PID file is removed.

---

### US-6 — Stop the background llama-server
> As a developer, I want to stop the running llama-server cleanly.

**Acceptance criteria:**
- `yllama stop` reads the PID from `~/.yllama/llamacpp.pid`, sends `SIGTERM`,
  and removes the PID file.
- If no PID file exists a clear error is returned.
- If the process has already exited a clear error is returned.

---

### US-7 — Sync Vibe config from the running server
> As a developer, I want `~/.vibe/config.toml` to reflect the models that are
> actually loaded on the server so that Vibe shows the right options.

**Acceptance criteria:**
- `yllama sync` queries `GET /v1/models` on the configured server.
- Non-`llamacpp` providers and models in the existing config are preserved
  unchanged.
- The `llamacpp` provider entry is updated to point at the current
  `host:port`.
- A `[[models]]` entry is created for each model returned by the server.
- `active_model` is updated to the first live model.
- The command errors if the server is not reachable.

**Tests:** `vibe_config_tests::vibe_config_preserves_non_llamacpp_providers`,
`vibe_config_tests::*` (TOML serialization).

---

### US-8 — Launch Vibe with llama.cpp auto-started
> As a developer, I want `yllama vibe [folder] [<vibe-args>]` to ensure
> llama.cpp is running, sync the Vibe config, and then open Vibe in a given
> directory — all in one command.

**Acceptance criteria:**
- If llama-server is not running it is started using the first available
  downloaded model (same logic as `yllama serve`).
- `~/.vibe/config.toml` is synchronised before Vibe opens.
- Vibe is exec'd (replaces the yllama process) in the given folder, or the
  current directory if none is specified.
- Any trailing arguments after `--` (or after the folder) are forwarded
  verbatim to the `vibe` binary (e.g. `yllama vibe ~/project -- --theme dark`).
- Errors at any stage (no models, server timeout, `vibe` not on PATH) are
  reported before launching.

---

### US-9 — Generate a LiteLLM proxy config
> As a developer, I want to generate a `litellm_config.yaml` pointing at my
> local llama.cpp server so that I can expose it as an OpenAI-compatible
> endpoint to other tools.

**Acceptance criteria:**
- `yllama litellm [--output <path>]` queries the running server and writes a
  YAML file with one `model_list` entry per loaded model.
- Each entry uses `model: openai/<model-id>` and `api_base: <server>/v1`.
- `drop_params: true` is included under `litellm_settings`.
- The output path defaults to `./litellm_config.yaml`.
- A hint for starting the proxy (`litellm --config <file>`) is printed.

**Tests:** `litellm_tests::*`.

---

### US-10 — Install the binary to PATH
> As a developer, I want to install the `yllama` binary to a directory that
> is on my `$PATH` so that I can run it from any terminal.

**Acceptance criteria:**
- `yllama install [--dir <path>]` copies the currently running binary to the
  target directory (default: `~/.local/bin`).
- The target directory is created if it does not exist.
- The installed file is made executable (`chmod 755`).
- If the target directory is not on `$PATH`, a message with the required
  `export PATH=...` line is printed.

**Tests:** `install_tests::installs_binary_to_target_dir`.

---

## Configuration (`~/.yllama/config.toml`)

| Key          | Default          | Description                              |
|--------------|------------------|------------------------------------------|
| `server_bin` | `llama-server`   | Path or name of the llama.cpp binary.    |
| `port`       | `8080`           | Port for the llama.cpp HTTP server.      |
| `host`       | `127.0.0.1`      | Host for the llama.cpp HTTP server.      |

Missing config file → all defaults apply.

---

## Data Layout (`~/.yllama/`)

```
~/.yllama/
├── config.toml          # server_bin, port, host
├── llamacpp.pid         # PID of the running llama-server (written by serve)
└── models/
    ├── manifest.json    # Array of ModelEntry objects
    └── *.gguf           # Downloaded model files
```

---

### US-9 — Launch opencode with llama.cpp auto-started
> As a developer, I want `yllama opencode [folder] [<opencode-args>]` to ensure
> llama.cpp is running, sync the opencode config, and then open opencode in a given
> directory — all in one command.

**Acceptance criteria:**
- If llama-server is not running it is started using the first available
  downloaded model (same logic as `yllama serve`).
- `~/.cache/opencode/models.json` is synchronised before opencode opens.
- opencode is exec'd (replaces the yllama process) in the given folder, or the
  current directory if none is specified.
- Errors at any stage (no models, server timeout, `opencode` not on PATH) are
  reported before launching.

---

## Test Plan

### Unit tests (run with `cargo test`)

| File | Tests |
|------|-------|
| `src/commands/models/add.rs` | URL blob→resolve conversion, filename extraction, name derivation |
| `src/commands/models/list.rs` | `format_bytes`: GB, MB, B ranges |
| `tests/integration_test.rs` | Manifest JSON round-trip, URL parsing, TOML serialisation, LiteLLM YAML generation, install file-copy, serve/start extra_args merging, vibe extra_args forwarding |

### Integration tests (run with `cargo test -- --ignored`)

| Test | Requirement |
|------|-------------|
| `live_fetch_models_returns_non_empty` | llama-server running on `localhost:8080` |

### End-to-end verification checklist

```sh
# 1. Build
cargo build

# 2. Add a model
yllama models add https://huggingface.co/unsloth/gemma-4-26B-A4B-it-GGUF/resolve/main/UD-Q4_K_M.gguf
yllama models list   # shows "not downloaded"

# 3. Download
yllama models download UD-Q4_K_M
yllama models list   # shows "downloaded" + size

# 4. Serve (background)
yllama serve UD-Q4_K_M
curl http://localhost:8080/health   # 200 OK
cat ~/.yllama/llamacpp.pid          # PID printed

# 4b. Start (foreground — open a new terminal)
yllama start UD-Q4_K_M -- -ngl 35 -c 8192

# 5. Sync Vibe config
yllama sync
grep "llamacpp" ~/.vibe/config.toml  # provider + model present

# 6. LiteLLM config
yllama litellm --output /tmp/llm.yaml
cat /tmp/llm.yaml   # contains model entries

# 7. Vibe launch
yllama vibe /path/to/project -- --theme dark

# 8. Opencode launch
yllama opencode /path/to/project

# 9. Stop
yllama stop
curl http://localhost:8080/health   # connection refused

# 9. Delete model
yllama models delete UD-Q4_K_M
yllama models list   # empty

# 10. Install
cargo build --release
./target/release/yllama install
which yllama   # ~/.local/bin/yllama
```

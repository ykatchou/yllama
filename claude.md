# yllama Core Instructions

This file contains the core instructions and usage patterns for the `yllama` CLI.

## Overview
`yllama` is a CLI tool for managing a local AI stack: `llama.cpp` (inference), GGUF models, and `Vibe` (editor).

## Core Commands

### Model Management
- `yllama models add <url>`: Register a HuggingFace GGUF URL.
- `yllama models download <name>`: Download a registered model.
- `yllama models list`: List all registered models.
- `yllama models delete <name>`: Remove a model from cache.

### Server Management
- `yllama serve [model] [-- <flags>]`: Start llama-server as a background daemon.
- `yllama start [model] [-- <flags>]`: Start llama-server in the **foreground** (useful for debugging).
- `yllama stop`: Stop the running llama-server.
- `yllama attach`: Attach to the running llama-server output.

### Vibe Integration
- `yllama vibe [folder] [-- <vibe-args>]`: Launch Vibe, auto-starting the server and syncing config.

## Usage Patterns
- Changes should be committed along the way.
- Background server with GPU layers: `yllama serve -- -ngl 35 -c 8192`
- Foreground server (debug): `yllama start -- -ngl 35 -c 8192`
- Vibe with a specific theme: `yllama vibe -- --theme dark`
- Vibe in a folder with extra flags: `yllama vibe ~/project -- --theme dark`

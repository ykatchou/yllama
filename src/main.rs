mod commands;
mod config;
mod llamacpp;
mod manifest;
mod vibe_config;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "yllama",
    about = "Manage your local llama.cpp + Vibe AI stack",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch llama-server as a background daemon
    #[command(long_about = r#"
Launch llama-server as a background daemon.

If multiple models are downloaded and none is specified, an interactive menu
appears to select one. Use `yllama models download <name>` to add models.

MODEL:
    Name from cache (uses first downloaded model if omitted)

EXTRA FLAGS (forwarded verbatim to llama-server):
    GPU
      -ngl <N>          Number of GPU layers (e.g., -ngl 35)
      --split-mode <m>  Split mode: none, layer, row, tensor
      --tensor-split <f> Fraction of VRAM per GPU (e.g., --tensor-split 0.7,0.3,0)

    PERFORMANCE
      -t/--threads <N>  CPU threads (auto-detected if omitted)
      -b/--batch-size <N>   Logical processing batch size (default: 2048)
      -c/--ctx-size <N>   Context window size (e.g., -c 8192)
      --ubatch-size <N>   Physical unrolling batch size (default: 512)

    GENERATION
      --temp <f>      Sampling temperature (default: 0.8)
      --top-k <N>     Top-k sampling (default: 40)
      --top-p <f>     Top-p sampling (default: 0.95)
      --min-p <f>     Min-p sampling (default: 0.05)
      --seed <N>      RNG seed (default: random)

    ADVANCED
      --flash-attn          Enable Flash Attention
      --rope-scaling <m>    RoPE scaling: none, linear, yarn
      --kv-offload          Offload KV cache to CPU
      --mlock               Lock model in RAM
      --mmap                Memory-map model file (default)
      --no-mmap             Disable memory-mapping
      --lora <path>         Load a LoRA adapter

EXAMPLES:
    yllama serve                         # Use first/default downloaded model
    yllama serve my-model                # Use specific model
    yllama serve -ngl 35 -c 8192         # GPU layers + context size
    yllama start --temp 0.3 --top-p 0.9  # Foreground with generation params
"#)]
    Serve {
        /// Model name from cache (interactive selection if omitted and multiple models available)
        model: Option<String>,
        /// Extra flags forwarded verbatim to llama-server (e.g. -ngl 35 -c 8192)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
    /// Launch llama-server in the foreground (useful for debugging)
    #[command(long_about = r#"
Launch llama-server in the foreground (useful for debugging).

Same flags as `yllama serve` — see `yllama serve --help` for full details.
"#)]
    Start {
        /// Model name from cache (interactive selection if omitted and multiple models available)
        model: Option<String>,
        /// Extra flags forwarded verbatim to llama-server (e.g. -ngl 35 -c 8192)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
    /// Stop the running llama-server
    Stop,
    /// Attach to the running llama-server output
    Attach,
    /// Sync ~/.vibe/config.toml from the running server
    Sync,
    /// Launch Vibe in a directory (auto-starts llama.cpp and syncs Vibe config)
    Vibe {
        /// Directory to open in Vibe (defaults to current directory)
        folder: Option<PathBuf>,
        /// Extra arguments forwarded verbatim to the vibe binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
    /// Launch Claude Code with the local llama.cpp server (auto-starts llama.cpp)
    #[command(alias = "claude")]
    ClaudeCode {
        /// Directory to open in Claude Code (defaults to current directory)
        folder: Option<PathBuf>,
        /// Extra arguments forwarded verbatim to the claude binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },

    /// Generate a LiteLLM proxy config from the running server
    Litellm {
        /// Output file path (default: ./litellm_config.yaml)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Install the yllama binary into a directory on your PATH
    Install {
        /// Target directory (default: ~/bin)
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
    /// Manage the local GGUF model cache
    Models {
        #[command(subcommand)]
        subcommand: ModelsSubcommand,
    },
}

#[derive(Subcommand)]
enum ModelsSubcommand {
    /// List all registered models and their download status
    List,
    /// Register a HuggingFace GGUF URL or search by name
    Add {
        /// HuggingFace URL or search query (e.g. 'gemma')
        url: String,
        /// Short name for this model (derived from filename if omitted)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Download a registered model with a progress bar
    Download {
        /// Model name as shown in `yllama models list`
        name: String,
    },
    /// Delete a model from the cache and remove it from the manifest
    Delete {
        /// Model name as shown in `yllama models list`
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::load()?;

    match cli.command {
        Commands::Serve { model, extra_args } => {
            commands::serve::run(&cfg, model.as_deref(), false, &extra_args).await?;
        }
        Commands::Start { model, extra_args } => {
            commands::serve::run(&cfg, model.as_deref(), true, &extra_args).await?;
        }
        Commands::Stop => {
            commands::stop::run()?;
        }
        Commands::Attach => {
            commands::attach::run()?;
        }
        Commands::Sync => {
            commands::sync::run(&cfg).await?;
        }
        Commands::Vibe { folder, extra_args } => {
            commands::vibe::run(&cfg, folder, &extra_args).await?;
        }
        Commands::ClaudeCode { folder, extra_args } => {
            commands::claude_code::run(&cfg, folder, &extra_args).await?;
        }
        Commands::Litellm { output } => {
            commands::litellm::run(&cfg, output).await?;
        }
        Commands::Install { dir } => {
            commands::install::run(dir)?;
        }
        Commands::Models { subcommand } => match subcommand {
            ModelsSubcommand::List => {
                commands::models::list::run()?;
            }
            ModelsSubcommand::Add { url, name } => {
                commands::models::add::run(&url, name.as_deref()).await?;
            }
            ModelsSubcommand::Download { name } => {
                commands::models::download::run(&name).await?;
            }
            ModelsSubcommand::Delete { name } => {
                commands::models::delete::run(&name)?;
            }
        },
    }

    Ok(())
}

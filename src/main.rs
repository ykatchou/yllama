mod commands;
mod config;
mod deps;
mod llamacpp;
mod manifest;
mod vibe_config;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lower")]
enum ReasoningLevel {
    Off,
    Low,
    Medium,
    High,
    Max,
}

#[derive(Parser)]
#[command(
    name = "yllama",
    about = "Manage your local llama.cpp + Vibe AI stack",
    version,
    after_long_help = r#"
COMMANDS:
  SERVER
    serve (bg)    Launch llama-server as a background daemon
    start (fg)    Launch llama-server in the foreground
    stop          Stop the running server
    attach        View server logs

  MODELS
    models list   List all registered models
    models add    Register a model URL
    models download  Download a registered model
    models delete   Remove a model from cache

  INTEGRATION
    vibe          Launch Vibe (auto-starts server + syncs config)
    claude        Launch Claude Code (alias: claude-code)
    litellm       Generate LiteLLM proxy config
    sync          Sync Vibe config from running server
    install       Self-install yllama to PATH
"#
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
      -ngl <N>              Number of GPU layers (e.g., -ngl 35)
      --split-mode <m>      Split mode: none, layer, row, tensor
      --tensor-split <f>    Fraction of VRAM per GPU (e.g., --tensor-split 0.7,0.3,0)
      --rope-scaling <m>    RoPE scaling: none, linear, yarn
      --freq-params <f>     RoPE frequency params: none, linear, yarn, linear+yarn
      --offload-kv <b>      Offload KV cache: 0=off, 1=on, 2=auto (half/half of layers in VRAM)

    PERFORMANCE
      -t/--threads <N>      CPU threads (auto-detected if omitted)
      -b/--batch-size <N>   Logical processing batch size (default: 2048)
      -c/--ctx-size <N>     Context window size (e.g., -c 8192)
      --ubatch-size <N>     Physical unrolling batch size (default: 512)
      --batched             Process multiple prompts in parallel

    GENERATION
      --temp <f>            Sampling temperature (default: 0.8)
      --top-k <N>           Top-k sampling (default: 40)
      --top-p <f>           Top-p sampling (default: 0.95)
      --min-p <f>           Min-p sampling (default: 0.05)
      --xtc-p <f>           XTC probability threshold (default: 0.0)
      --typical-p <f>       Locally typical sampling parameter (default: 1.0)
      --repeat-penalty <f>  Repeat penalty (default: 1.1)
      --presence-penalty <f> Presence penalty (default: 0.0)
      --frequency-penalty <f> Frequency penalty (default: 0.0)
      --mirostat <N>        Mirostat sampling (0=off, 1=mirostat, 2=mirostat 2.0)
      --mirostat-ta <f>     Mirostat target alpha (default: 0.1)
      --mirostat-tau <f>    Mirostat tau (default: 5.0)
      --seed <N>            RNG seed (default: random, use -1 for time-based)
      --logit-bias <pat>    Logit bias: -1000..1000, or <token_hex>:<bias>
      --tk/--thinking <lvl> Reasoning level: off|low|medium|high|max (default: off)

    CONTEXT & SLOTS
      --slot-prompt <str>   Custom slot prompt
      --keep <N>            Keep model in memory after use (seconds)
      --no-ctx-drain        Disable context drain after use
      --log-idle            Log idle time in seconds
      --flush-logs          Flush logs to disk

    ADVANCED
      --flash-attn          Enable Flash Attention
      --kv-offload          Offload KV cache to CPU
      --mlock               Lock model in RAM
      --mmap                Memory-map model file (default)
      --no-mmap             Disable memory-mapping
      --no-pos-emb          Disable positional embeddings
      --no-cuda              Disable CUDA
      --no-ascend            Disable Ascend
      --lora <path>         Load a LoRA adapter
      --base <url>          Base model for LoRA fusion
      --log-prefix <str>    Log prefix
      --log-disable         Disable logging
      --log-only            Log only --log-prefix
      --log-no-meta         Don't log metadata
      --verbose             Verbose output (stdout)
      --host <ip>           Bind to IP (default: 127.0.0.1)
      --port <N>            Port (default: 8080)

EXAMPLES:
    yllama serve                         # Use first/default downloaded model
    yllama serve my-model                # Use specific model
    yllama serve my-model -- -ngl 35     # Use specific model with GPU layers
    yllama start -- -ngl 35 -c 8192      # Foreground with GPU + context size
"#)]
    #[command(alias = "bg")]
    Serve {
        /// Model name from cache (interactive selection if omitted and multiple models available)
        model: Option<String>,
        /// Reasoning/thinking level forwarded to llama-server as `--reasoning`.
        /// `off` disables reasoning; `low`, `medium`, `high`, `max` enable it with
        /// the corresponding effort (sent as `--reasoning on` — the OpenAI API
        /// parameter `reasoning_effort` overrides this per-request).
        #[arg(long, visible_alias = "tk", value_enum, default_value = "off")]
        thinking: ReasoningLevel,
        /// Extra flags forwarded verbatim to llama-server
        /// Use `--` to separate flags from the model name (e.g. `yllama serve model1 -- -ngl 35`)
        #[arg(last = true)]
        extra_args: Vec<String>,
    },
    /// Launch llama-server in the foreground (useful for debugging)
    #[command(long_about = r#"
Launch llama-server in the foreground (useful for debugging).

EXTRA FLAGS (forwarded verbatim to llama-server):
    GPU
      -ngl <N>              Number of GPU layers (e.g., -ngl 35)
      --split-mode <m>      Split mode: none, layer, row, tensor
      --tensor-split <f>    Fraction of VRAM per GPU (e.g., --tensor-split 0.7,0.3,0)
      --rope-scaling <m>    RoPE scaling: none, linear, yarn
      --freq-params <f>     RoPE frequency params: none, linear, yarn, linear+yarn
      --offload-kv <b>      Offload KV cache: 0=off, 1=on, 2=auto

    PERFORMANCE
      -t/--threads <N>      CPU threads (auto-detected if omitted)
      -b/--batch-size <N>   Logical processing batch size (default: 2048)
      -c/--ctx-size <N>     Context window size (e.g., -c 8192)
      --ubatch-size <N>     Physical unrolling batch size (default: 512)
      --batched             Process multiple prompts in parallel

    GENERATION
      --temp <f>            Sampling temperature (default: 0.8)
      --top-k <N>           Top-k sampling (default: 40)
      --top-p <f>           Top-p sampling (default: 0.95)
      --min-p <f>           Min-p sampling (default: 0.05)
      --xtc-p <f>           XTC probability threshold (default: 0.0)
      --typical-p <f>       Locally typical sampling parameter (default: 1.0)
      --repeat-penalty <f>  Repeat penalty (default: 1.1)
      --presence-penalty <f> Presence penalty (default: 0.0)
      --frequency-penalty <f> Frequency penalty (default: 0.0)
      --mirostat <N>        Mirostat sampling (0=off, 1=mirostat, 2=mirostat 2.0)
      --mirostat-ta <f>     Mirostat target alpha (default: 0.1)
      --mirostat-tau <f>    Mirostat tau (default: 5.0)
      --seed <N>            RNG seed (default: random)
      --logit-bias <pat>    Logit bias: <token_hex>:<bias>
      --tk/--thinking <lvl> Reasoning level: off|low|medium|high|max (default: off)

    CONTEXT & SLOTS
      --slot-prompt <str>   Custom slot prompt
      --keep <N>            Keep model in memory after use (seconds)
      --no-ctx-drain        Disable context drain after use

    ADVANCED
      --flash-attn          Enable Flash Attention
      --kv-offload          Offload KV cache to CPU
      --mlock               Lock model in RAM
      --mmap                Memory-map model file (default)
      --no-mmap             Disable memory-mapping
      --lora <path>         Load a LoRA adapter
"#)]
    #[command(alias = "fg")]
    Start {
        /// Model name from cache (interactive selection if omitted and multiple models available)
        model: Option<String>,
        /// Reasoning/thinking level forwarded to llama-server as `--reasoning`.
        /// `off` disables reasoning; `low`, `medium`, `high`, `max` enable it with
        /// the corresponding effort (sent as `--reasoning on` — the OpenAI API
        /// parameter `reasoning_effort` overrides this per-request).
        #[arg(long, visible_alias = "tk", value_enum, default_value = "off")]
        thinking: ReasoningLevel,
        /// Extra flags forwarded verbatim to llama-server
        /// Use `--` to separate flags from the model name (e.g. `yllama start -- -ngl 35`)
        #[arg(last = true)]
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
        /// Target directory (default: ~/.local/bin)
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
        /// HuggingFace URL (e.g. 'https://huggingface.co/owner/repo/blob/main/model.gguf'),
        /// owner/repo shorthand (auto-discovers GGUF files), or
        /// free-text search query (e.g. 'gemma')
        url: String,
        /// Short name for this model (derived from filename if omitted)
        #[arg(short, long)]
        name: Option<String>,
        /// Download the model immediately after registering it
        #[arg(short, long)]
        download: bool,
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
        Commands::Serve { model, thinking, extra_args } => {
            commands::serve::run(&cfg, model.as_deref(), false, thinking, &extra_args).await?;
        }
        Commands::Start { model, thinking, extra_args } => {
            commands::serve::run(&cfg, model.as_deref(), true, thinking, &extra_args).await?;
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
            ModelsSubcommand::Add { url, name, download } => {
                commands::models::add::run(&url, name.as_deref(), download).await?;
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

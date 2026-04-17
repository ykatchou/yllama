mod commands;
mod config;
mod llamacpp;
mod manifest;
mod vibe_config;
mod opencode_config;

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
    Serve {
        /// Model name from cache (uses first downloaded model if omitted)
        model: Option<String>,
        /// Extra flags forwarded verbatim to llama-server (e.g. -ngl 35 -c 8192)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
    /// Launch llama-server in the foreground (useful for debugging)
    Start {
        /// Model name from cache (uses first downloaded model if omitted)
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
    /// Launch opencode in a directory (auto-starts llama.cpp and syncs opencode config)
    Opencode {
        /// Directory to open in opencode (defaults to current directory)
        folder: Option<PathBuf>,
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
        Commands::Opencode { folder } => {
            commands::opencode::run(&cfg, folder).await?;
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

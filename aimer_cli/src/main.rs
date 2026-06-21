use crate::commands::version::VersionCommand;
use anyhow::Context;
use clap::{Parser, Subcommand};
use std::env::set_current_dir;

pub mod commands;
pub mod config;
pub mod errors;
pub mod targets;
pub mod tui;
pub mod console;

#[derive(Parser)]
#[command(name = "aimer")]
#[command(about = "Aimer Framework CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Show the version of the CLI
    #[arg(short = 'v', long = "version")]
    version: bool,

    /// Enable verbose (debug) logging
    #[arg(long, global = true)]
    verbose: bool,
}

/// Initialise the tracing subscriber. Honours `RUST_LOG` if set, otherwise
/// defaults to `warn` (or `debug` when `--verbose` is passed).
fn init_logging(verbose: bool) {
    use tracing_subscriber::{EnvFilter, fmt};

    let default_level = if verbose { "debug" } else { "warn" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    Create {
        /// Name of the project
        project_name: String,
    },

    /// Run the project (interactive picker, or scriptable with --target/--device)
    Run {
        /// Build/run for this target without showing the picker
        #[arg(short, long)]
        target: Option<String>,
        /// Run on the device with this id without showing the picker
        #[arg(short, long)]
        device: Option<String>,
    },

    /// Build the project for a target without launching it
    Build {
        /// Target to build for (defaults to aimer.toml's default_target)
        #[arg(short, long)]
        target: Option<String>,
        /// Build in release mode
        #[arg(short, long)]
        release: bool,
    },

    /// Assemble the distributable platform bundle (.app, .apk, .ipa, ...)
    Assemble {
        /// Target platform to bundle for (macos, android, ios, web, ...)
        platform: String,
        /// Assemble in release mode
        #[arg(short, long)]
        release: bool,
    },

    /// Check that the required toolchains are installed
    Doctor,

    /// Remove build artifacts (builds/ and target/)
    Clean,

    /// Generate a shell completion script
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, ...)
        shell: clap_complete::Shell,
        /// Install the script into the shell's completion directory instead of
        /// printing it to stdout
        #[arg(long)]
        install: bool,
    },
}



fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    #[cfg(debug_assertions)]
    {
        let currnt_dir = option_env!("MY_PROJECT_DIR");
        match currnt_dir {
            Some(dir) => {
                set_current_dir(dir)
                    .with_context(|| format!("failed to set current dir to '{dir}'"))?;
            }
            None => {
                tracing::debug!("MY_PROJECT_DIR is not set");
            }
        }
    }

    if cli.version {
        VersionCommand::execute();
        return Ok(());
    }

    match &cli.command {
        Some(Commands::Create { project_name }) => {
            commands::create::execute(project_name)?;
        }
        Some(Commands::Run { target, device }) => {
            commands::run::execute(target.clone(), device.clone())?;
        }
        Some(Commands::Build { target, release }) => {
            commands::build::execute(target.clone(), *release)?;
        }
        Some(Commands::Assemble { platform, release }) => {
            commands::assemble::execute(platform.clone(), *release)?;
        }
        Some(Commands::Doctor) => {
            commands::doctor::execute()?;
        }
        Some(Commands::Clean) => {
            commands::clean::execute()?;
        }
        Some(Commands::Completions { shell, install }) => {
            commands::completions::execute(*shell, *install)?;
        }
        None => {
            Cli::parse_from(["aimer", "--help"]);
        }
    }

    Ok(())
}

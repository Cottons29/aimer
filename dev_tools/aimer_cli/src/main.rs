use crate::commands::version::VersionCommand;
use crate::targets::{MigrateTarget, Targets};
use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand};
use std::env::set_current_dir;

pub mod commands;
pub mod config;
pub mod console;
pub mod errors;
pub mod targets;
pub mod tui;

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
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
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
        #[arg(short, long, value_enum)]
        target: Option<Targets>,
        /// Run on the device with this id without showing the picker
        #[arg(short, long)]
        device: Option<String>,
        /// Disable the interactive TUI and print logs to stdout/stderr instead.
        /// Useful when running from an IDE or CI where no terminal is available.
        #[arg(long)]
        no_tui: bool,
    },

    /// Build the project for a target without launching it
    Build {
        /// Target to build for (defaults to aimer.toml's default_target)
        #[arg(short, long, value_enum)]
        target: Option<Targets>,
        /// Build in release mode
        #[arg(short, long)]
        release: bool,
    },

    /// Assemble the distributable platform bundle (.app, .apk, .ipa, ...)
    Assemble {
        /// Target platform to bundle for (macos, android, ios, web, ...)
        #[arg(value_enum)]
        platform: Targets,
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

    /// Migrate platform build scaffolds to the latest version
    Migrate {
        /// Target to migrate (macos, windows, linux, android, ios, web, all)
        #[arg(value_enum)]
        target: MigrateTarget,
    },
}

fn main() -> anyhow::Result<()> {
    // Dynamic, self-updating shell completions. When the shell invokes the
    // binary with `COMPLETE=<shell>` set, this generates completions from the
    // *current* command tree (so newly added subcommands appear automatically)
    // and exits. Must run before anything writes to stdout.
    clap_complete::env::CompleteEnv::with_factory(Cli::command).complete();

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
        Some(Commands::Run { target, device, no_tui }) => {
            commands::run::execute(target.map(|t| t.to_string()), device.clone(), *no_tui)?;
        }
        Some(Commands::Build { target, release }) => {
            commands::build::execute(target.map(|t| t.to_string()), *release)?;
        }
        Some(Commands::Assemble { platform, release }) => {
            commands::assemble::execute(platform.to_string(), *release)?;
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
        Some(Commands::Migrate { target }) => {
            commands::migrate::execute(target.as_str().to_string())?;
        }
        None => {
            Cli::parse_from(["aimer", "--help"]);
        }
    }

    Ok(())
}

use std::env::{current_dir, set_current_dir};
use clap::{Parser, Subcommand};
use crate::commands::version::VersionCommand;

pub mod commands;
pub mod targets;

#[derive(Parser)]
#[command(name = "aimer")]
#[command(about = "Aimer Framework CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Show the version of the CLI
    #[arg(short = 'v', long = "version")]
    version: bool,
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_VERSION: &str = "0.1.1";

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    Create {
        /// Name of the project
        project_name: String,
    },

    /// Run the project
    Run,
}

pub fn start_cli() {
    #[cfg(debug_assertions)]
    set_current_dir("/Users/cottons/Documents/aimer-fw/playground/jaime").unwrap();
    let cli = Cli::parse();

    if cli.version {
        VersionCommand::execute();
        return;
    }

    match &cli.command {
        Some(Commands::Create { project_name }) => {
            commands::create::execute(project_name);
        }
        Some(Commands::Run) => {
            commands::run::execute();
        }
        None => {
            Cli::parse_from(["aimer", "--help"]);
        }
    }
}

use std::env::{current_dir, set_current_dir};
use clap::{Parser, Subcommand};

pub mod commands;
pub mod targets;
mod inspector;

#[derive(Parser)]
#[command(name = "aimer")]
#[command(about = "Aimer Framework CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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
    let cli = Cli::parse();

    match &cli.command {
        Commands::Create { project_name } => {
            commands::create::execute(project_name);
        }
        Commands::Run => {
            commands::run::execute();
        }
    }
}

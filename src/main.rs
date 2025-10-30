// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use clap::Parser;
use muesli::{cli::Cli, Result};

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command() {
        muesli::cli::Commands::Sync => {
            println!("Sync command - not yet implemented");
        }
        muesli::cli::Commands::List => {
            println!("List command - not yet implemented");
        }
        muesli::cli::Commands::Fetch { id } => {
            println!("Fetch command for ID: {} - not yet implemented", id);
        }
    }

    Ok(())
}

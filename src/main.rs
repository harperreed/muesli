// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use clap::Parser;
use muesli::{
    api::ApiClient, auth::resolve_token, cli::Cli, storage::Paths, sync::sync_all, Result,
};

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
            let token = resolve_token(cli.token)?;
            let mut client = ApiClient::new(token, Some(cli.api_base))?;

            if cli.no_throttle {
                client = client.disable_throttle();
            } else if let Some((min, max)) = cli.throttle_ms {
                client = client.with_throttle(min, max);
            }

            let paths = Paths::new(cli.data_dir)?;
            sync_all(&client, &paths)?;
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

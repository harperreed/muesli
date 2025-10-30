// ABOUTME: Command-line interface definitions using clap
// ABOUTME: Defines all subcommands and global flags

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "muesli")]
#[command(about = "Rust CLI for syncing Granola meeting transcripts", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Bearer token (overrides session/env)
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// API base URL
    #[arg(long, global = true, default_value = "https://api.granola.ai")]
    pub api_base: String,

    /// Override data directory
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Disable throttling (not recommended)
    #[arg(long, global = true)]
    pub no_throttle: bool,

    /// Throttle range in ms (min:max)
    #[arg(long, global = true, value_parser = parse_throttle_range)]
    pub throttle_ms: Option<(u64, u64)>,
}

fn parse_throttle_range(s: &str) -> Result<(u64, u64), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("Expected format: min:max".into());
    }

    let min = parts[0].parse().map_err(|_| "Invalid min value")?;
    let max = parts[1].parse().map_err(|_| "Invalid max value")?;

    if min > max {
        return Err("min must be <= max".into());
    }

    Ok((min, max))
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Sync all documents (default)
    Sync,

    /// List all documents
    List,

    /// Fetch a specific document by ID
    Fetch {
        /// Document ID to fetch
        id: String,
    },
}

impl Cli {
    pub fn command(&self) -> Commands {
        self.command.clone().unwrap_or(Commands::Sync)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_throttle_range_valid() {
        let result = parse_throttle_range("100:300").unwrap();
        assert_eq!(result, (100, 300));
    }

    #[test]
    fn test_parse_throttle_range_invalid() {
        assert!(parse_throttle_range("300:100").is_err());
        assert!(parse_throttle_range("abc:def").is_err());
        assert!(parse_throttle_range("100").is_err());
    }
}

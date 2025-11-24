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
    Sync {
        /// Force reindex of all documents without re-downloading
        #[arg(long)]
        #[cfg(feature = "index")]
        reindex: bool,
    },

    /// List all documents
    List,

    /// Fetch a specific document by ID
    Fetch {
        /// Document ID to fetch
        id: String,
    },

    /// Search indexed documents (requires 'index' feature)
    #[cfg(feature = "index")]
    Search {
        /// Search query string
        query: String,

        /// Maximum number of results to return
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,

        /// Use semantic search with embeddings (requires 'embeddings' feature)
        #[arg(long)]
        #[cfg(feature = "embeddings")]
        semantic: bool,
    },

    /// Open the data directory in the system file browser
    Open,

    /// Fix file modification dates to match meeting creation dates
    FixDates,

    /// Store OpenAI API key in system keychain (macOS only)
    #[cfg(feature = "summaries")]
    SetApiKey {
        /// OpenAI API key
        api_key: String,
    },

    /// Configure summarization settings (model, context window, prompt)
    #[cfg(feature = "summaries")]
    SetConfig {
        /// OpenAI model to use (e.g., gpt-5, gpt-4o, gpt-4o-mini)
        #[arg(long)]
        model: Option<String>,

        /// Context window size in characters
        #[arg(long)]
        context_window: Option<usize>,

        /// Path to custom prompt file
        #[arg(long)]
        prompt_file: Option<std::path::PathBuf>,

        /// Show current configuration
        #[arg(long)]
        show: bool,
    },

    /// Summarize a transcript using OpenAI
    #[cfg(feature = "summaries")]
    Summarize {
        /// Document ID to summarize
        doc_id: String,

        /// Save summary to file (default: print to stdout)
        #[arg(long)]
        save: bool,
    },
}

impl Cli {
    pub fn command(&self) -> Commands {
        self.command.clone().unwrap_or(Commands::Sync {
            #[cfg(feature = "index")]
            reindex: false,
        })
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

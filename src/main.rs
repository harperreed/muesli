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
            let client = create_client(&cli)?;
            let paths = Paths::new(cli.data_dir)?;
            sync_all(&client, &paths)?;
        }
        muesli::cli::Commands::List => {
            let client = create_client(&cli)?;
            let docs = client.list_documents()?;

            for doc in docs {
                let date = doc.created_at.format("%Y-%m-%d");
                let title = doc.title.as_deref().unwrap_or("Untitled");
                println!("{}\t{}\t{}", doc.id, date, title);
            }
        }
        muesli::cli::Commands::Fetch { id } => {
            let client = create_client(&cli)?;
            let paths = Paths::new(cli.data_dir)?;
            paths.ensure_dirs()?;

            // Fetch metadata and transcript
            let meta = client.get_metadata(&id)?;
            let raw = client.get_transcript(&id)?;

            // Compute filename
            let date = meta.created_at.format("%Y-%m-%d").to_string();
            let slug = muesli::util::slugify(meta.title.as_deref().unwrap_or("untitled"));
            let base_filename = format!("{}_{}", date, slug);

            // Convert to markdown
            let md = muesli::convert::to_markdown(&raw, &meta, &id)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files
            let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
            let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

            let raw_json = serde_json::to_string_pretty(&raw)?;
            muesli::storage::write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
            muesli::storage::write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            println!("wrote {}", json_path.display());
            println!("wrote {}", md_path.display());
        }
        #[cfg(feature = "index")]
        muesli::cli::Commands::Search {
            query,
            limit,
            #[cfg(feature = "embeddings")]
            semantic,
        } => {
            let paths = Paths::new(cli.data_dir)?;

            // Check for semantic search
            #[cfg(feature = "embeddings")]
            {
                if semantic {
                    // Check if vector store exists
                    let metadata_path = paths.index_dir.join("vectors.meta.json");
                    if !metadata_path.exists() {
                        eprintln!("No vector store found. Run 'muesli sync' first to generate embeddings.");
                        std::process::exit(1);
                    }

                    // Perform semantic search
                    let results = muesli::embeddings::semantic_search(&paths, &query, limit)?;

                    // Handle empty results
                    if results.is_empty() {
                        println!("No results found for: {}", query);
                        return Ok(());
                    }

                    // Display results
                    for (rank, result) in results.iter().enumerate() {
                        let title = result.title.as_deref().unwrap_or("Untitled");
                        println!(
                            "{}. {} ({}) [score: {:.3}]  {}",
                            rank + 1,
                            title,
                            result.date,
                            result.score,
                            result.path
                        );
                    }
                    return Ok(());
                }
            }

            // Fall back to text search
            // Check if index exists
            if !paths.index_dir.exists() {
                eprintln!("No index found. Run 'muesli sync' first to build the index.");
                std::process::exit(1);
            }

            // Open the index
            let index = muesli::index::text::create_or_open_index(&paths.index_dir)?;

            // Perform the search
            let results = muesli::index::text::search(&index, &query, limit)?;

            // Handle empty results
            if results.is_empty() {
                println!("No results found for: {}", query);
                return Ok(());
            }

            // Display results
            for (rank, result) in results.iter().enumerate() {
                let title = result.title.as_deref().unwrap_or("Untitled");
                println!("{}. {} ({})  {}", rank + 1, title, result.date, result.path);
            }
        }
    }

    Ok(())
}

/// Creates an API client with auth and throttle configuration from CLI flags.
fn create_client(cli: &Cli) -> Result<ApiClient> {
    let token = resolve_token(cli.token.clone())?;
    let mut client = ApiClient::new(token, Some(cli.api_base.clone()))?;

    if cli.no_throttle {
        client = client.disable_throttle();
    } else if let Some((min, max)) = cli.throttle_ms {
        client = client.with_throttle(min, max);
    }

    Ok(client)
}

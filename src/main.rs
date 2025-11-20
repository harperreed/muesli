// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use clap::Parser;
use muesli::{
    api::ApiClient,
    auth::resolve_token,
    cli::Cli,
    storage::Paths,
    sync::{fix_dates, sync_all},
    Result,
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
        muesli::cli::Commands::Sync {
            #[cfg(feature = "index")]
            reindex,
        } => {
            let client = create_client(&cli)?;
            let paths = Paths::new(cli.data_dir)?;
            #[cfg(feature = "index")]
            {
                sync_all(&client, &paths, reindex)?;
            }
            #[cfg(not(feature = "index"))]
            {
                sync_all(&client, &paths, false)?;
            }
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

            // Set file modification time to meeting creation date
            muesli::storage::set_file_time(&json_path, &meta.created_at)?;
            muesli::storage::set_file_time(&md_path, &meta.created_at)?;

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
        muesli::cli::Commands::Open => {
            let paths = Paths::new(cli.data_dir)?;
            paths.ensure_dirs()?;

            // Open the data directory in the system file browser
            if let Err(e) = open::that(&paths.data_dir) {
                eprintln!("Failed to open data directory: {}", e);
                std::process::exit(1);
            }
            println!("Opened data directory: {}", paths.data_dir.display());
        }
        muesli::cli::Commands::FixDates => {
            let paths = Paths::new(cli.data_dir)?;
            fix_dates(&paths)?;
        }
        #[cfg(feature = "summaries")]
        muesli::cli::Commands::SetApiKey { api_key } => {
            muesli::summary::set_api_key_in_keychain(&api_key)?;
        }
        #[cfg(feature = "summaries")]
        muesli::cli::Commands::Summarize { doc_id, save } => {
            let paths = Paths::new(cli.data_dir)?;

            // Find the markdown file for this doc_id
            let md_path = find_transcript_by_id(&paths, &doc_id)?;

            // Read the transcript
            let content = std::fs::read_to_string(&md_path)?;

            // Extract body (skip frontmatter)
            let body = if content.starts_with("---\n") {
                content
                    .split("---\n")
                    .nth(2)
                    .unwrap_or(&content)
                    .to_string()
            } else {
                content
            };

            // Get API key
            let api_key = std::env::var("OPENAI_API_KEY")
                .or_else(|_| muesli::summary::get_api_key_from_keychain())?;

            // Run async summarization
            println!("Summarizing transcript...");
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let summary = rt.block_on(muesli::summary::summarize_transcript(&body, &api_key))?;

            if save {
                // Save to summaries directory
                let filename = md_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| {
                        muesli::Error::Filesystem(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid filename",
                        ))
                    })?;
                let summary_path = paths.summaries_dir.join(format!("{}_summary.md", filename));

                muesli::storage::write_atomic(&summary_path, summary.as_bytes(), &paths.tmp_dir)?;
                println!("✅ Summary saved to: {}", summary_path.display());
            } else {
                // Print to stdout
                println!("\n{}\n", summary);
            }
        }
    }

    Ok(())
}

/// Find a transcript file by document ID
#[cfg(feature = "summaries")]
fn find_transcript_by_id(paths: &Paths, doc_id: &str) -> muesli::Result<std::path::PathBuf> {
    use std::fs;

    let entries = fs::read_dir(&paths.transcripts_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read frontmatter to check doc_id
        if let Some(fm) = muesli::storage::read_frontmatter(&path)? {
            if fm.doc_id == doc_id {
                return Ok(path);
            }
        }
    }

    Err(muesli::Error::Filesystem(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("No transcript found for document ID: {}", doc_id),
    )))
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

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

    match cli.command {
        None => {
            use clap::CommandFactory;
            Cli::command().print_help().expect("Failed to print help");
            println!();
        }
        Some(muesli::cli::Commands::Sync {
            force,
            #[cfg(feature = "index")]
            reindex,
        }) => {
            let client = create_client(&cli)?;
            let paths = Paths::new(cli.data_dir)?;
            #[cfg(feature = "index")]
            {
                sync_all(&client, &paths, reindex, force)?;
            }
            #[cfg(not(feature = "index"))]
            {
                sync_all(&client, &paths, false, force)?;
            }
        }
        Some(muesli::cli::Commands::List) => {
            let client = create_client(&cli)?;
            let docs = client.list_documents()?;

            for doc in docs {
                let date = doc.created_at.format("%Y-%m-%d");
                let title = doc.title.as_deref().unwrap_or("Untitled");
                println!("{}\t{}\t{}", doc.id, date, title);
            }
        }
        #[cfg(feature = "storage")]
        Some(muesli::cli::Commands::Local) => {
            let paths = Paths::new(cli.data_dir)?;
            let conn = muesli::db::connection::open_or_create(&paths.db_path)?;
            let mut docs = muesli::db::queries::list_documents(&conn)?;
            docs.reverse();

            if docs.is_empty() {
                eprintln!("No documents found. Run 'muesli sync' first.");
            } else {
                for doc in &docs {
                    let date = doc.created_at.format("%Y-%m-%d");
                    let title = doc.title.as_deref().unwrap_or("Untitled");
                    println!("{}\t{}\t{}", date, doc.doc_id, title);
                }
            }
        }
        #[cfg(feature = "storage")]
        Some(muesli::cli::Commands::Show { ref doc_id, full }) => {
            let paths = Paths::new(cli.data_dir.clone())?;
            let conn = muesli::db::connection::open_or_create(&paths.db_path)?;

            let doc = muesli::db::queries::get_document(&conn, doc_id)?
                .ok_or_else(|| muesli::Error::Filesystem(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("No document found with ID: {}", doc_id),
                )))?;

            let date = doc.created_at.format("%Y-%m-%d %H:%M");
            let title = doc.title.as_deref().unwrap_or("Untitled");
            let duration = doc.duration_seconds
                .map(|d| format!("{}m", d / 60))
                .unwrap_or_else(|| "—".to_string());

            println!("{}", title);
            println!("{}\t{}\t{}", date, duration, doc.doc_id);
            if !doc.attendees.is_empty() {
                println!("Attendees: {}", doc.attendees.join(", "));
            }
            if !doc.labels.is_empty() {
                println!("Labels: {}", doc.labels.join(", "));
            }

            if full {
                // Print the full transcript from the markdown file
                if let Some(ref filename) = doc.filename {
                    let md_path = paths.transcripts_dir.join(format!("{}.md", filename));
                    if md_path.exists() {
                        let content = std::fs::read_to_string(&md_path)?;
                        // Strip YAML frontmatter if present
                        let body = if content.starts_with("---\n") {
                            content
                                .splitn(3, "---\n")
                                .nth(2)
                                .unwrap_or(&content)
                                .to_string()
                        } else {
                            content
                        };
                        println!("\n{}", body);
                    } else {
                        eprintln!("Transcript file not found: {}", md_path.display());
                    }
                } else {
                    eprintln!("No transcript file recorded for this document.");
                }
            } else if let Some(ref summary) = doc.summary_text {
                println!("\n{}", summary);
            } else if let Some(ref notes) = doc.notes {
                println!("\n{}", notes);
            } else {
                eprintln!("\nNo summary available. Use --full to show the transcript.");
            }
        }
        #[cfg(feature = "index")]
        Some(muesli::cli::Commands::Find { query, limit, text }) => {
            let paths = Paths::new(cli.data_dir)?;

            if text {
                // Text search via Tantivy
                if !paths.index_dir.exists() {
                    eprintln!("No index found. Run 'muesli sync' first to build the index.");
                    std::process::exit(1);
                }
                let index = muesli::index::text::create_or_open_index(&paths.index_dir)?;
                let results = muesli::index::text::search(&index, &query, limit)?;

                if results.is_empty() {
                    println!("No results found for: {}", query);
                } else {
                    for (rank, result) in results.iter().enumerate() {
                        let title = result.title.as_deref().unwrap_or("Untitled");
                        println!("{}. {} ({})  {}", rank + 1, title, result.date, result.doc_id);
                    }
                }
            } else {
                // Semantic search via embeddings
                #[cfg(feature = "embeddings")]
                {
                    let metadata_path = paths.index_dir.join("vectors.meta.json");
                    if !metadata_path.exists() {
                        eprintln!("No vector store found. Run 'muesli sync' first to generate embeddings.");
                        std::process::exit(1);
                    }

                    let results = muesli::embeddings::semantic_search(&paths, &query, limit)?;

                    if results.is_empty() {
                        println!("No results found for: {}", query);
                    } else {
                        for (rank, result) in results.iter().enumerate() {
                            let title = result.title.as_deref().unwrap_or("Untitled");
                            println!(
                                "{}. {} ({}) [score: {:.3}]  {}",
                                rank + 1, title, result.date, result.score, result.doc_id
                            );
                        }
                    }
                }
                #[cfg(not(feature = "embeddings"))]
                {
                    // Fall back to text search when embeddings feature is not available
                    eprintln!("Note: semantic search requires the 'embeddings' feature. Falling back to text search.");
                    if !paths.index_dir.exists() {
                        eprintln!("No index found. Run 'muesli sync' first to build the index.");
                        std::process::exit(1);
                    }
                    let index = muesli::index::text::create_or_open_index(&paths.index_dir)?;
                    let results = muesli::index::text::search(&index, &query, limit)?;

                    if results.is_empty() {
                        println!("No results found for: {}", query);
                    } else {
                        for (rank, result) in results.iter().enumerate() {
                            let title = result.title.as_deref().unwrap_or("Untitled");
                            println!("{}. {} ({})  {}", rank + 1, title, result.date, result.doc_id);
                        }
                    }
                }
            }
        }
        Some(muesli::cli::Commands::Fetch { ref id }) => {
            let client = create_client(&cli)?;
            let paths = Paths::new(cli.data_dir)?;
            paths.ensure_dirs()?;

            // Fetch metadata and transcript, keeping raw responses
            let meta_resp = client.get_metadata_with_raw(id)?;
            let transcript_resp = client.get_transcript_with_raw(id)?;
            let meta = meta_resp.parsed;
            let transcript = transcript_resp.parsed;

            // Compute filename
            let created = meta.created_at.unwrap_or_else(chrono::Utc::now);
            let date = created.format("%Y-%m-%d").to_string();
            let slug = muesli::util::slugify(meta.title.as_deref().unwrap_or("untitled"));
            let base_filename = format!("{}_{}", date, slug);

            // Convert to markdown (notes/summary fetched only during sync)
            let md = muesli::convert::to_markdown(&transcript, &meta, id, None, None)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files: save verbatim API responses as raw JSON
            let transcript_json_path = paths
                .raw_dir
                .join(format!("{}_transcript.json", base_filename));
            let metadata_json_path = paths
                .raw_dir
                .join(format!("{}_metadata.json", base_filename));
            let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

            muesli::storage::write_atomic(
                &transcript_json_path,
                transcript_resp.raw.as_bytes(),
                &paths.tmp_dir,
            )?;
            muesli::storage::write_atomic(
                &metadata_json_path,
                meta_resp.raw.as_bytes(),
                &paths.tmp_dir,
            )?;
            muesli::storage::write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            // Set file modification time to meeting creation date
            muesli::storage::set_file_time(&transcript_json_path, &created)?;
            muesli::storage::set_file_time(&metadata_json_path, &created)?;
            muesli::storage::set_file_time(&md_path, &created)?;

            println!("wrote {}", transcript_json_path.display());
            println!("wrote {}", metadata_json_path.display());
            println!("wrote {}", md_path.display());
        }
        #[cfg(feature = "index")]
        Some(muesli::cli::Commands::Search {
            query,
            limit,
            #[cfg(feature = "embeddings")]
            semantic,
        }) => {
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
        Some(muesli::cli::Commands::Open) => {
            let paths = Paths::new(cli.data_dir)?;
            paths.ensure_dirs()?;

            // Open the data directory in the system file browser
            if let Err(e) = open::that(&paths.data_dir) {
                eprintln!("Failed to open data directory: {}", e);
                std::process::exit(1);
            }
            println!("Opened data directory: {}", paths.data_dir.display());
        }
        Some(muesli::cli::Commands::FixDates) => {
            let paths = Paths::new(cli.data_dir)?;
            fix_dates(&paths)?;
        }
        #[cfg(feature = "summaries")]
        Some(muesli::cli::Commands::SetApiKey { api_key }) => {
            muesli::summary::set_api_key_in_keychain(&api_key)?;
        }
        #[cfg(feature = "summaries")]
        Some(muesli::cli::Commands::SetConfig {
            model,
            context_window,
            prompt_file,
            show,
        }) => {
            let paths = Paths::new(cli.data_dir)?;
            let config_path = paths.data_dir.join("summary_config.json");

            if show {
                // Show current config
                let config = muesli::summary::SummaryConfig::load(&config_path)?;
                println!("Current summarization configuration:");
                println!("  Model: {}", config.model);
                println!(
                    "  Context window: {} characters",
                    config.context_window_chars
                );
                println!(
                    "  Custom prompt: {}",
                    if config.custom_prompt.is_some() {
                        "Yes"
                    } else {
                        "No (using default)"
                    }
                );
                if let Some(prompt) = &config.custom_prompt {
                    println!("\nCustom prompt:");
                    println!("{}", prompt);
                }
                return Ok(());
            }

            // Load existing config or create default
            let mut config = muesli::summary::SummaryConfig::load(&config_path)?;

            // Update fields if provided
            if let Some(m) = model {
                config.model = m;
            }
            if let Some(cw) = context_window {
                config.context_window_chars = cw;
            }
            if let Some(pf) = prompt_file {
                let prompt = std::fs::read_to_string(&pf)?;
                config.custom_prompt = Some(prompt);
            }

            // Save config
            config.save(&config_path, &paths.tmp_dir)?;
            println!("✅ Configuration saved");
            println!("  Model: {}", config.model);
            println!(
                "  Context window: {} characters",
                config.context_window_chars
            );
        }
        #[cfg(feature = "summaries")]
        Some(muesli::cli::Commands::Summarize { doc_id, save }) => {
            let paths = Paths::new(cli.data_dir)?;

            // Load config
            let config_path = paths.data_dir.join("summary_config.json");
            let config = muesli::summary::SummaryConfig::load(&config_path)?;

            // Find the markdown file for this doc_id
            let md_path = find_transcript_by_id(&paths, &doc_id)?;

            // Read the transcript
            let content = std::fs::read_to_string(&md_path)?;

            // Extract body (skip frontmatter; splitn limits to 3 parts
            // so body-internal "---" separators are preserved)
            let body = if content.starts_with("---\n") {
                content
                    .splitn(3, "---\n")
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
            println!(
                "Summarizing with {} (context window: {} chars)...",
                config.model, config.context_window_chars
            );
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let summary = rt.block_on(muesli::summary::summarize_transcript(
                &body, &api_key, &config,
            ))?;

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
        #[cfg(feature = "mcp")]
        Some(muesli::cli::Commands::Mcp) => {
            // Run MCP server asynchronously
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(muesli::mcp::serve_mcp(cli.data_dir))?;
        }
        #[cfg(feature = "storage")]
        Some(muesli::cli::Commands::Stats) => {
            let paths = Paths::new(cli.data_dir)?;
            let conn = muesli::db::connection::open_or_create(&paths.db_path)?;
            let stats = muesli::db::queries::get_stats(&conn)?;
            let avg_dur = muesli::db::queries::average_duration(&conn)?;

            println!("Meeting Statistics");
            println!("==================");
            println!("Total meetings:     {}", stats.total_meetings);
            println!(
                "Total duration:     {}h {}m",
                stats.total_duration_seconds / 3600,
                (stats.total_duration_seconds % 3600) / 60
            );
            println!("Average duration:   {:.0} min", avg_dur);
            println!("Unique attendees:   {}", stats.unique_attendees);
            println!("Meetings per week:  {:.1}", stats.meetings_per_week);

            let top = muesli::db::queries::top_attendees(&conn, 10)?;
            if !top.is_empty() {
                println!("\nTop Attendees");
                println!("-------------");
                for att in &top {
                    println!("  {:3}x  {}", att.count, att.name);
                }
            }

            let labels = muesli::db::queries::label_distribution(&conn)?;
            if !labels.is_empty() {
                println!("\nLabels");
                println!("------");
                for lbl in &labels {
                    println!("  {:3}x  {}", lbl.count, lbl.label);
                }
            }
        }
        #[cfg(feature = "storage")]
        Some(muesli::cli::Commands::Query {
            ref attendee,
            ref label,
            ref title,
            limit,
        }) => {
            let paths = Paths::new(cli.data_dir)?;
            let conn = muesli::db::connection::open_or_create(&paths.db_path)?;

            let docs = if let Some(name) = attendee {
                muesli::db::queries::filter_by_attendee(&conn, name)?
            } else if let Some(lbl) = label {
                muesli::db::queries::filter_by_label(&conn, lbl)?
            } else if let Some(q) = title {
                muesli::db::queries::search_documents(&conn, q, limit)?
            } else {
                muesli::db::queries::list_documents(&conn)?
            };

            let docs: Vec<_> = docs.into_iter().take(limit).collect();

            if docs.is_empty() {
                println!("No matching documents found.");
            } else {
                for doc in &docs {
                    let title = doc.title.as_deref().unwrap_or("Untitled");
                    let date = doc.created_at.format("%Y-%m-%d");
                    let dur = doc
                        .duration_seconds
                        .map(|d| format!("{}m", d / 60))
                        .unwrap_or_default();
                    println!("{}\t{}\t{}\t{}", doc.doc_id, date, dur, title);
                }
                println!("\n{} result(s)", docs.len());
            }
        }
        #[cfg(feature = "tui")]
        Some(muesli::cli::Commands::Tui) => {
            let paths = Paths::new(cli.data_dir)?;
            muesli::tui::run::run_tui(&paths)?;
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

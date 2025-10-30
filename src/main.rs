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
            let token = resolve_token(cli.token)?;
            let mut client = ApiClient::new(token, Some(cli.api_base))?;

            if cli.no_throttle {
                client = client.disable_throttle();
            }

            let docs = client.list_documents()?;

            for doc in docs {
                let date = doc.created_at.format("%Y-%m-%d");
                let title = doc.title.as_deref().unwrap_or("Untitled");
                println!("{}\t{}\t{}", doc.id, date, title);
            }
        }
        muesli::cli::Commands::Fetch { id } => {
            let token = resolve_token(cli.token)?;
            let mut client = ApiClient::new(token, Some(cli.api_base))?;

            if cli.no_throttle {
                client = client.disable_throttle();
            }

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
            let md = muesli::convert::to_markdown(&raw, &meta)?;
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
    }

    Ok(())
}

// ABOUTME: Core sync logic for fetching and storing documents
// ABOUTME: Handles update detection and progress reporting

use crate::{
    api::ApiClient,
    convert::to_markdown,
    storage::{read_frontmatter, write_atomic, Paths},
    util::slugify,
    Result,
};
use indicatif::{ProgressBar, ProgressStyle};

#[cfg(feature = "index")]
use crate::index::text;

pub fn sync_all(client: &ApiClient, paths: &Paths) -> Result<()> {
    paths.ensure_dirs()?;

    // Create or open the index and writer (feature-gated)
    #[cfg(feature = "index")]
    let (index, mut writer) = {
        let idx = text::create_or_open_index(&paths.index_dir)?;
        let wtr = idx.writer(50_000_000)
            .map_err(|e| crate::Error::Indexing(format!("Failed to create index writer: {}", e)))?;
        (idx, wtr)
    };

    println!("Fetching document list...");
    let docs = client.list_documents()?;

    let pb = ProgressBar::new(docs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} docs")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut synced = 0;
    let mut skipped = 0;

    for doc_summary in &docs {
        // Fetch metadata
        let meta = client.get_metadata(&doc_summary.id)?;

        // Compute filename
        let date = meta.created_at.format("%Y-%m-%d").to_string();
        let slug = slugify(meta.title.as_deref().unwrap_or("untitled"));
        let base_filename = format!("{}_{}", date, slug);

        // Check for existing file and update
        let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

        let should_update = if md_path.exists() {
            if let Some(fm) = read_frontmatter(&md_path)? {
                if fm.doc_id == doc_summary.id {
                    // Same doc - check if remote is newer
                    let remote_ts = meta.updated_at.unwrap_or(meta.created_at);
                    let local_ts = fm.remote_updated_at.unwrap_or(fm.created_at);
                    remote_ts > local_ts
                } else {
                    // Different doc with same filename - need collision handling
                    // For now, skip (will implement collision in next task)
                    false
                }
            } else {
                // No frontmatter - update
                true
            }
        } else {
            // New file
            true
        };

        if should_update {
            // Fetch transcript
            let raw = client.get_transcript(&doc_summary.id)?;

            // Convert to markdown
            let md = to_markdown(&raw, &meta, &doc_summary.id)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files
            let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
            let raw_json = serde_json::to_string_pretty(&raw)?;

            write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
            write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            // Index the document (feature-gated, non-fatal)
            #[cfg(feature = "index")]
            {
                let date_str = date.clone();
                if let Err(e) = text::index_markdown_batch(
                    &mut writer,
                    &index,
                    &doc_summary.id,
                    meta.title.as_deref(),
                    &date_str,
                    &md.body,
                    &md_path,
                ) {
                    eprintln!("Warning: Failed to index document {}: {}", doc_summary.id, e);
                }
            }

            synced += 1;
        } else {
            skipped += 1;
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "synced {} docs ({} new/updated, {} skipped)",
        docs.len(),
        synced,
        skipped
    ));

    // Commit all indexed documents in one batch (feature-gated)
    #[cfg(feature = "index")]
    {
        if synced > 0 {
            if let Err(e) = writer.commit() {
                eprintln!("Warning: Failed to commit index changes: {}", e);
            } else {
                println!("Indexed {} documents", synced);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::storage::Paths;
    use tempfile::TempDir;

    #[test]
    fn test_sync_creates_index_directory() {
        // Verify that sync operation creates the index directory structure
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();

        // Call ensure_dirs to set up directory structure
        paths.ensure_dirs().unwrap();

        // Verify index directory exists at the correct path
        assert!(
            paths.index_dir.exists(),
            "index_dir should exist at {}",
            paths.index_dir.display()
        );

        // Verify it's the tantivy subdirectory
        assert!(
            paths.index_dir.ends_with("index/tantivy"),
            "index_dir should end with 'index/tantivy', got {}",
            paths.index_dir.display()
        );
    }
}

#[cfg(all(test, feature = "index"))]
mod index_tests {
    use crate::index::text::create_or_open_index;
    use crate::storage::Paths;
    use tempfile::TempDir;

    #[test]
    fn test_index_integration_with_sync() {
        // Test that the index directory path works with the indexing module
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        // Verify we can create an index at the configured path
        let index = create_or_open_index(&paths.index_dir).unwrap();
        let schema = index.schema();

        // Verify schema has required fields
        assert!(schema.get_field("doc_id").is_ok());
        assert!(schema.get_field("title").is_ok());
        assert!(schema.get_field("body").is_ok());
    }
}

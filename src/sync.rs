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

pub fn sync_all(client: &ApiClient, paths: &Paths) -> Result<()> {
    paths.ensure_dirs()?;

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
                if fm.doc_id == meta.id {
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
            let raw = client.get_transcript(&meta.id)?;

            // Convert to markdown
            let md = to_markdown(&raw, &meta)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files
            let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
            let raw_json = serde_json::to_string_pretty(&raw)?;

            write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
            write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

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

    Ok(())
}

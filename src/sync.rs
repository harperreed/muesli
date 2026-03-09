// ABOUTME: Core sync logic for fetching and storing documents
// ABOUTME: Handles update detection and progress reporting

use crate::{
    api::ApiClient,
    convert::to_markdown,
    storage::{set_file_time, write_atomic, Paths},
    util::slugify,
    Result,
};

#[cfg(feature = "index")]
use crate::storage::read_frontmatter;
#[cfg(not(feature = "storage"))]
use chrono::{DateTime, Utc};
use indicatif::{ProgressBar, ProgressStyle};
#[cfg(not(feature = "storage"))]
use serde::{Deserialize, Serialize};
#[cfg(not(feature = "storage"))]
use std::collections::HashMap;

#[cfg(feature = "index")]
use crate::index::text;

#[cfg(feature = "embeddings")]
use crate::embeddings::{downloader, engine::EmbeddingEngine, vector::VectorStore};

#[cfg(not(feature = "storage"))]
#[derive(Serialize, Deserialize)]
struct CacheEntry {
    filename: String,
    updated_at: DateTime<Utc>,
}

/// Load the sync cache (doc_id -> metadata)
#[cfg(not(feature = "storage"))]
fn load_cache(cache_path: &std::path::Path) -> HashMap<String, CacheEntry> {
    if !cache_path.exists() {
        return HashMap::new();
    }

    std::fs::read_to_string(cache_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save the sync cache atomically
#[cfg(not(feature = "storage"))]
fn save_cache(
    cache_path: &std::path::Path,
    cache: &HashMap<String, CacheEntry>,
    tmp_dir: &std::path::Path,
) -> Result<()> {
    let json = serde_json::to_string_pretty(cache)?;
    write_atomic(cache_path, json.as_bytes(), tmp_dir)?;
    Ok(())
}

pub fn sync_all(
    client: &ApiClient,
    paths: &Paths,
    #[cfg_attr(not(feature = "index"), allow(unused_variables))] reindex: bool,
    force: bool,
) -> Result<()> {
    paths.ensure_dirs()?;

    // Handle reindex mode (feature-gated)
    #[cfg(feature = "index")]
    if reindex {
        return reindex_all(paths);
    }

    // Create or open the index and writer (feature-gated)
    #[cfg(feature = "index")]
    let (index, mut writer) = {
        let idx = text::create_or_open_index(&paths.index_dir)?;
        let wtr = idx
            .writer(50_000_000)
            .map_err(|e| crate::Error::Indexing(format!("Failed to create index writer: {}", e)))?;
        (idx, wtr)
    };

    // Initialize embedding engine and vector store (feature-gated)
    #[cfg(feature = "embeddings")]
    let (mut embedding_engine, mut vector_store) = {
        println!("Initializing embedding engine...");

        // Ensure model is downloaded
        let model_paths = downloader::ensure_model(&paths.models_dir)?;

        // Create embedding engine
        let engine = EmbeddingEngine::new(&model_paths.model_path, &model_paths.tokenizer_path)?;
        println!("✅ Embedding engine ready (dimension: {})", engine.dim());

        // Load or create vector store
        let vector_path = paths.index_dir.join("vectors");
        let metadata_path = paths.index_dir.join("vectors.meta.json");
        let store = if metadata_path.exists() {
            println!("Loading existing vector store...");
            VectorStore::load(&vector_path)?
        } else {
            println!("Creating new vector store");
            VectorStore::new(engine.dim())
        };

        (engine, store)
    };

    // Open DuckDB connection and migrate JSON cache if needed (feature-gated)
    #[cfg(feature = "storage")]
    let db_conn = {
        let conn = crate::db::connection::open_or_create(&paths.db_path)?;
        // One-time migration from JSON cache to DuckDB
        let json_cache_path = paths.data_dir.join(".sync_cache.json");
        if json_cache_path.exists() {
            eprintln!("Migrating sync cache from JSON to DuckDB...");
            if let Err(e) = crate::db::queries::migrate_from_json_cache(&conn, &json_cache_path) {
                eprintln!("Warning: Failed to migrate cache: {}", e);
            } else {
                let migrated_path = paths.data_dir.join(".sync_cache.json.migrated");
                let _ = std::fs::rename(&json_cache_path, &migrated_path);
                eprintln!("Migrated sync cache to DuckDB");
            }
        }
        conn
    };

    if force {
        println!("Force sync enabled — ignoring cache timestamps");
    }
    println!("Fetching document list...");
    let docs = client.list_documents_with_notes()?;

    // Diagnostic: count notes availability
    let with_enhanced = docs.iter().filter(|d| d.enhanced_notes().is_some()).count();
    let with_user = docs.iter().filter(|d| d.user_notes().is_some()).count();
    println!(
        "Notes: {} with AI summary, {} with user notes (of {} total)",
        with_enhanced,
        with_user,
        docs.len(),
    );

    // Load the sync cache
    #[cfg(not(feature = "storage"))]
    let cache_path = paths.data_dir.join(".sync_cache.json");
    #[cfg(not(feature = "storage"))]
    let mut cache = load_cache(&cache_path);

    let pb = ProgressBar::new(docs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} docs")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut synced = 0;
    let mut skipped = 0;

    #[cfg(feature = "embeddings")]
    let mut embedded = 0;

    for doc_summary in &docs {
        // Check cache for quick timestamp comparison (--force bypasses cache)
        #[cfg(feature = "storage")]
        let (should_update, cached_filename) = {
            if force {
                (true, None)
            } else {
                match crate::db::queries::get_cache_entry(&db_conn, &doc_summary.id) {
                    Ok(Some(entry)) => {
                        let remote_ts = doc_summary.updated_at.unwrap_or(doc_summary.created_at);
                        let ts_changed = remote_ts > entry.updated_at;
                        // Re-sync if the API has a summary but the DB doesn't
                        let needs_summary = doc_summary.enhanced_notes().is_some()
                            && crate::db::queries::doc_missing_summary(&db_conn, &doc_summary.id)
                                .unwrap_or(false);
                        (ts_changed || needs_summary, Some(entry.filename))
                    }
                    _ => (true, None),
                }
            }
        };
        #[cfg(not(feature = "storage"))]
        let should_update = if force {
            true
        } else if let Some(cache_entry) = cache.get(&doc_summary.id) {
            let remote_ts = doc_summary.updated_at.unwrap_or(doc_summary.created_at);
            remote_ts > cache_entry.updated_at
        } else {
            true
        };

        // Check if we need to generate embeddings (independent of sync status)
        #[cfg(feature = "embeddings")]
        let needs_embedding = !vector_store.has_document(&doc_summary.id);

        #[cfg(not(feature = "embeddings"))]
        let needs_embedding = false;

        // If nothing to do, skip
        if !should_update && !needs_embedding {
            skipped += 1;
            pb.inc(1);
            continue;
        }

        // Fetch metadata and transcript from API, keeping raw responses
        let meta_resp = client.get_metadata_with_raw(&doc_summary.id)?;
        let transcript_resp = client.get_transcript_with_raw(&doc_summary.id)?;
        let mut meta = meta_resp.parsed;
        let transcript = transcript_resp.parsed;

        // Backfill fields from doc_summary when metadata lacks them
        if meta.created_at.is_none() {
            meta.created_at = Some(doc_summary.created_at);
        }
        if meta.title.is_none() {
            meta.title = doc_summary.title.clone();
        }

        // Extract user notes from panels (my_notes → notes field → last_viewed_panel fallback)
        let notes_md = doc_summary
            .user_notes()
            .as_ref()
            .map(crate::convert::prosemirror_to_markdown)
            .filter(|s| !s.is_empty());

        // Extract AI-generated summary from panels (enhanced_notes panel)
        let summary_text = doc_summary
            .enhanced_notes()
            .as_ref()
            .map(crate::convert::prosemirror_to_markdown)
            .filter(|s| !s.is_empty());

        // Convert to markdown
        let md = to_markdown(
            &transcript,
            &meta,
            &doc_summary.id,
            notes_md.as_deref(),
            summary_text.as_deref(),
        )?;

        if should_update {
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Compute filename (may have changed if title changed)
            let created = meta.created_at.unwrap_or(doc_summary.created_at);
            let date = created.format("%Y-%m-%d").to_string();
            let slug = slugify(meta.title.as_deref().unwrap_or("untitled"));
            let base_filename = format!("{}_{}", date, slug);
            let new_md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

            // If filename changed in cache, remove old files
            #[cfg(feature = "storage")]
            if let Some(ref old_filename) = cached_filename {
                if old_filename != &base_filename {
                    let old_path = paths.transcripts_dir.join(format!("{}.md", old_filename));
                    if old_path.exists() {
                        std::fs::remove_file(&old_path)?;
                    }
                    for suffix in &["", "_transcript", "_metadata"] {
                        let old_json = paths
                            .raw_dir
                            .join(format!("{}{}.json", old_filename, suffix));
                        if old_json.exists() {
                            std::fs::remove_file(&old_json)?;
                        }
                    }
                }
            }
            #[cfg(not(feature = "storage"))]
            if let Some(old_entry) = cache.get(&doc_summary.id) {
                if old_entry.filename != base_filename {
                    let old_path = paths
                        .transcripts_dir
                        .join(format!("{}.md", old_entry.filename));
                    if old_path.exists() {
                        std::fs::remove_file(&old_path)?;
                    }
                    // Clean up all raw file variants (legacy + new naming)
                    for suffix in &["", "_transcript", "_metadata"] {
                        let old_json = paths
                            .raw_dir
                            .join(format!("{}{}.json", old_entry.filename, suffix));
                        if old_json.exists() {
                            std::fs::remove_file(&old_json)?;
                        }
                    }
                }
            }

            // Write files: save verbatim API responses as raw JSON
            let transcript_json_path = paths
                .raw_dir
                .join(format!("{}_transcript.json", base_filename));
            let metadata_json_path = paths
                .raw_dir
                .join(format!("{}_metadata.json", base_filename));

            write_atomic(
                &transcript_json_path,
                transcript_resp.raw.as_bytes(),
                &paths.tmp_dir,
            )?;
            write_atomic(
                &metadata_json_path,
                meta_resp.raw.as_bytes(),
                &paths.tmp_dir,
            )?;
            write_atomic(&new_md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            // Remove legacy single .json file if it exists
            let legacy_json = paths.raw_dir.join(format!("{}.json", base_filename));
            if legacy_json.exists() {
                std::fs::remove_file(&legacy_json)?;
            }

            // Set file modification time to meeting creation date
            set_file_time(&transcript_json_path, &created)?;
            set_file_time(&metadata_json_path, &created)?;
            set_file_time(&new_md_path, &created)?;

            // Update cache - CRITICAL: store the same timestamp we compare against
            // (doc_summary.updated_at, NOT meta.updated_at - they can differ!)
            let stored_ts = doc_summary.updated_at.unwrap_or(doc_summary.created_at);

            #[cfg(feature = "storage")]
            {
                // Store document metadata and update cache in DuckDB
                if let Err(e) = crate::db::queries::upsert_document(
                    &db_conn,
                    &meta,
                    &doc_summary.id,
                    &base_filename,
                    notes_md.as_deref(),
                    summary_text.as_deref(),
                ) {
                    eprintln!(
                        "Warning: Failed to store document {} in database: {}",
                        doc_summary.id, e
                    );
                }
                crate::db::queries::upsert_cache_entry(
                    &db_conn,
                    &doc_summary.id,
                    &base_filename,
                    &stored_ts,
                )?;
            }

            #[cfg(not(feature = "storage"))]
            {
                cache.insert(
                    doc_summary.id.clone(),
                    CacheEntry {
                        filename: base_filename.clone(),
                        updated_at: stored_ts,
                    },
                );
                save_cache(&cache_path, &cache, &paths.tmp_dir)?;
            }

            // Index the document (feature-gated, non-fatal)
            #[cfg(feature = "index")]
            {
                let date = created.format("%Y-%m-%d").to_string();
                if let Err(e) = text::index_markdown_batch(
                    &mut writer,
                    &index,
                    &doc_summary.id,
                    meta.title.as_deref(),
                    &date,
                    &md.body,
                    &new_md_path,
                ) {
                    eprintln!(
                        "Warning: Failed to index document {}: {}",
                        doc_summary.id, e
                    );
                }
            }

            synced += 1;
        }

        // Generate embeddings (feature-gated, non-fatal)
        #[cfg(feature = "embeddings")]
        {
            if needs_embedding {
                // Combine title and body for embedding
                let text_for_embedding = if let Some(title) = meta.title.as_deref() {
                    format!("{}\n\n{}", title, &md.body)
                } else {
                    md.body.clone()
                };

                // Truncate to avoid token limits (rough estimate: 1 token ≈ 4 chars)
                let max_chars = 2000; // ~500 tokens, well under 512 limit
                let text_truncated = if text_for_embedding.len() > max_chars {
                    // Find valid UTF-8 boundary
                    let mut boundary = max_chars.min(text_for_embedding.len());
                    while boundary > 0 && !text_for_embedding.is_char_boundary(boundary) {
                        boundary -= 1;
                    }
                    &text_for_embedding[..boundary]
                } else {
                    &text_for_embedding
                };

                match embedding_engine
                    .embed_passage(text_truncated)
                    .and_then(|vec| vector_store.add_document(doc_summary.id.clone(), vec))
                {
                    Ok(_) => embedded += 1,
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to embed document {}: {}",
                            doc_summary.id, e
                        );
                    }
                }
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "synced {} docs ({} new/updated, {} skipped)",
        docs.len(),
        synced,
        skipped
    ));

    // Detect and remove locally-cached documents that no longer exist on the server
    let remote_ids: std::collections::HashSet<&str> =
        docs.iter().map(|d| d.id.as_str()).collect();

    #[cfg(feature = "storage")]
    {
        let cached_entries = crate::db::queries::list_all_cached_entries(&db_conn)?;
        let mut deleted = 0;
        for (doc_id, filename) in &cached_entries {
            if remote_ids.contains(doc_id.as_str()) {
                continue;
            }
            // Remove local files
            let md_path = paths.transcripts_dir.join(format!("{}.md", filename));
            if md_path.exists() {
                std::fs::remove_file(&md_path)?;
            }
            for suffix in &["_transcript", "_metadata", ""] {
                let json_path = paths.raw_dir.join(format!("{}{}.json", filename, suffix));
                if json_path.exists() {
                    std::fs::remove_file(&json_path)?;
                }
            }
            // Remove from database
            crate::db::queries::delete_document(&db_conn, doc_id)?;
            deleted += 1;
        }
        if deleted > 0 {
            println!("Removed {} deleted documents", deleted);
        }
    }

    #[cfg(not(feature = "storage"))]
    {
        let stale_ids: Vec<String> = cache
            .keys()
            .filter(|id| !remote_ids.contains(id.as_str()))
            .cloned()
            .collect();
        let mut deleted = 0;
        for doc_id in &stale_ids {
            if let Some(entry) = cache.remove(doc_id) {
                let md_path = paths.transcripts_dir.join(format!("{}.md", entry.filename));
                if md_path.exists() {
                    std::fs::remove_file(&md_path)?;
                }
                for suffix in &["_transcript", "_metadata", ""] {
                    let json_path = paths
                        .raw_dir
                        .join(format!("{}{}.json", entry.filename, suffix));
                    if json_path.exists() {
                        std::fs::remove_file(&json_path)?;
                    }
                }
                deleted += 1;
            }
        }
        if deleted > 0 {
            save_cache(&cache_path, &cache, &paths.tmp_dir)?;
            println!("Removed {} deleted documents", deleted);
        }
    }

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

    // Save vector store (feature-gated)
    #[cfg(feature = "embeddings")]
    {
        let vector_path = paths.index_dir.join("vectors");
        if let Err(e) = vector_store.save(&vector_path) {
            eprintln!("Warning: Failed to save vector store: {}", e);
        } else if embedded > 0 {
            println!("✅ Generated embeddings for {} new documents", embedded);
        } else {
            println!("✅ All documents already have embeddings");
        }
    }

    Ok(())
}

/// Reindex all existing markdown files without re-downloading
#[cfg(feature = "index")]
fn reindex_all(paths: &Paths) -> Result<()> {
    use std::fs;

    println!("Reindexing all documents from disk...");

    // Create or open the index
    let index = text::create_or_open_index(&paths.index_dir)?;
    let mut writer = index
        .writer(50_000_000)
        .map_err(|e| crate::Error::Indexing(format!("Failed to create index writer: {}", e)))?;

    // Scan transcripts directory
    let entries = fs::read_dir(&paths.transcripts_dir).map_err(crate::Error::Filesystem)?;

    let mut indexed = 0;
    let mut failed = 0;

    for entry in entries {
        let entry = entry.map_err(crate::Error::Filesystem)?;
        let path = entry.path();

        // Only process .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read frontmatter
        let frontmatter = match read_frontmatter(&path)? {
            Some(fm) => fm,
            None => {
                eprintln!("Warning: Skipping {} (no frontmatter)", path.display());
                failed += 1;
                continue;
            }
        };

        // Read the markdown body
        let content = fs::read_to_string(&path).map_err(crate::Error::Filesystem)?;

        // Extract body after frontmatter (splitn limits to 3 parts
        // so body-internal "---" separators are preserved)
        let body = if content.starts_with("---\n") {
            content.splitn(3, "---\n").nth(2).unwrap_or(&content)
        } else {
            &content
        };

        // Index the document
        let date = frontmatter.created_at.format("%Y-%m-%d").to_string();
        match text::index_markdown_batch(
            &mut writer,
            &index,
            &frontmatter.doc_id,
            frontmatter.title.as_deref(),
            &date,
            body,
            &path,
        ) {
            Ok(_) => indexed += 1,
            Err(e) => {
                eprintln!("Warning: Failed to index {}: {}", path.display(), e);
                failed += 1;
            }
        }
    }

    // Commit the index
    writer
        .commit()
        .map_err(|e| crate::Error::Indexing(format!("Failed to commit index: {}", e)))?;

    println!("✅ Reindexed {} documents", indexed);
    if failed > 0 {
        println!("⚠️  {} documents failed to index", failed);
    }

    Ok(())
}

/// Fix file modification dates for all existing files to match meeting creation dates
pub fn fix_dates(paths: &Paths) -> Result<()> {
    use std::fs;

    println!("Fixing file modification dates...");

    let entries = fs::read_dir(&paths.transcripts_dir).map_err(crate::Error::Filesystem)?;

    let mut fixed = 0;
    let mut failed = 0;

    for entry in entries {
        let entry = entry.map_err(crate::Error::Filesystem)?;
        let path = entry.path();

        // Only process .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Read frontmatter to get the created_at date
        #[cfg(feature = "index")]
        let frontmatter = match read_frontmatter(&path)? {
            Some(fm) => fm,
            None => {
                eprintln!("Warning: Skipping {} (no frontmatter)", path.display());
                failed += 1;
                continue;
            }
        };

        #[cfg(not(feature = "index"))]
        let frontmatter = {
            // Without index feature, we need to parse frontmatter manually
            let content = fs::read_to_string(&path).map_err(crate::Error::Filesystem)?;
            if !content.starts_with("---\n") {
                eprintln!("Warning: Skipping {} (no frontmatter)", path.display());
                failed += 1;
                continue;
            }
            let rest = &content[4..];
            if let Some(end_pos) = rest.find("\n---\n") {
                let yaml = &rest[..end_pos];
                match serde_yaml::from_str::<crate::Frontmatter>(yaml) {
                    Ok(fm) => fm,
                    Err(e) => {
                        eprintln!(
                            "Warning: Skipping {} (failed to parse frontmatter: {})",
                            path.display(),
                            e
                        );
                        failed += 1;
                        continue;
                    }
                }
            } else {
                eprintln!("Warning: Skipping {} (no frontmatter)", path.display());
                failed += 1;
                continue;
            }
        };

        // Set the file time
        match set_file_time(&path, &frontmatter.created_at) {
            Ok(_) => {
                // Also fix corresponding JSON files if they exist
                let filename = path.file_stem().unwrap().to_str().unwrap();
                for suffix in &["_transcript", "_metadata", ""] {
                    let json_path = paths.raw_dir.join(format!("{}{}.json", filename, suffix));
                    if json_path.exists() {
                        if let Err(e) = set_file_time(&json_path, &frontmatter.created_at) {
                            eprintln!(
                                "Warning: Failed to set time for {}: {}",
                                json_path.display(),
                                e
                            );
                        }
                    }
                }
                fixed += 1;
            }
            Err(e) => {
                eprintln!("Warning: Failed to set time for {}: {}", path.display(), e);
                failed += 1;
            }
        }
    }

    println!("✅ Fixed dates for {} files", fixed);
    if failed > 0 {
        println!("⚠️  {} files failed", failed);
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

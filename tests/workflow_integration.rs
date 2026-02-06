// ABOUTME: Integration tests for end-to-end workflows
// ABOUTME: Tests reindex, search, semantic search, ProseMirror conversion, markdown output, and DB roundtrips

#[cfg(feature = "index")]
use muesli::Result;
use std::fs;
use tempfile::TempDir;

/// Test that raw file saving uses the correct naming convention
#[test]
fn test_raw_file_naming_convention() {
    use muesli::storage::Paths;

    let temp_dir = TempDir::new().unwrap();
    let paths = Paths::new(Some(temp_dir.path().to_path_buf())).unwrap();
    paths.ensure_dirs().unwrap();

    let base_filename = "2025-01-15_planning-meeting";

    // Simulate the new file naming by writing files
    let transcript_path = paths
        .raw_dir
        .join(format!("{}_transcript.json", base_filename));
    let metadata_path = paths
        .raw_dir
        .join(format!("{}_metadata.json", base_filename));
    let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

    fs::write(&transcript_path, r#"[{"text": "hello"}]"#).unwrap();
    fs::write(&metadata_path, r#"{"created_at": "2025-01-15T10:00:00Z"}"#).unwrap();
    fs::write(&md_path, "# Test").unwrap();

    assert!(transcript_path.exists());
    assert!(metadata_path.exists());
    assert!(md_path.exists());

    // Verify the raw dir contains both files
    let raw_files: Vec<_> = fs::read_dir(&paths.raw_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert!(
        raw_files.iter().any(|f| f.ends_with("_transcript.json")),
        "Should have transcript JSON"
    );
    assert!(
        raw_files.iter().any(|f| f.ends_with("_metadata.json")),
        "Should have metadata JSON"
    );
}

/// Helper to create a sample markdown file with frontmatter
#[cfg(feature = "index")]
fn create_sample_markdown(
    dir: &std::path::Path,
    doc_id: &str,
    title: &str,
    date: &str,
    body: &str,
) -> Result<std::path::PathBuf> {
    let filename = format!("{}_{}.md", date, title.to_lowercase().replace(' ', "-"));
    let path = dir.join(&filename);

    let content = format!(
        r#"---
doc_id: {}
source: granola
title: {}
created_at: {}T10:00:00Z
remote_updated_at: {}T10:00:00Z
generator: muesli v0.1.0
participants: []
labels: []
---

{}
"#,
        doc_id, title, date, date, body
    );

    fs::write(&path, content)?;
    Ok(path)
}

#[test]
#[cfg(feature = "index")]
fn test_reindex_workflow() -> Result<()> {
    use muesli::index::text;

    // Create temp directory structure
    let temp_dir = TempDir::new().unwrap();
    let transcripts_dir = temp_dir.path().join("transcripts");
    let index_dir = temp_dir.path().join("index");
    fs::create_dir_all(&transcripts_dir)?;
    fs::create_dir_all(&index_dir)?;

    // Create sample markdown files
    create_sample_markdown(
        &transcripts_dir,
        "doc1",
        "Product Strategy Meeting",
        "2024-01-15",
        "We discussed the product roadmap and quarterly goals for Q1.",
    )?;

    create_sample_markdown(
        &transcripts_dir,
        "doc2",
        "Engineering Standup",
        "2024-01-16",
        "Team updates on the authentication refactor and API improvements.",
    )?;

    create_sample_markdown(
        &transcripts_dir,
        "doc3",
        "Customer Feedback Review",
        "2024-01-17",
        "Analyzed user feedback from the latest product release.",
    )?;

    // Run reindex (we call the indexing logic directly since sync_all requires ApiClient)
    let index = text::create_or_open_index(&index_dir)?;
    let mut writer = index
        .writer(50_000_000)
        .map_err(|e| muesli::Error::Indexing(format!("Failed to create writer: {}", e)))?;

    // Index all markdown files
    let mut indexed_count = 0;
    for entry in fs::read_dir(&transcripts_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let frontmatter = muesli::storage::read_frontmatter(&path)?.unwrap();
        let content = fs::read_to_string(&path)?;
        let body = content.split("---\n").nth(2).unwrap_or("");

        let date = frontmatter.created_at.format("%Y-%m-%d").to_string();
        text::index_markdown_batch(
            &mut writer,
            &index,
            &frontmatter.doc_id,
            frontmatter.title.as_deref(),
            &date,
            body,
            &path,
        )?;
        indexed_count += 1;
    }

    writer
        .commit()
        .map_err(|e| muesli::Error::Indexing(format!("Failed to commit: {}", e)))?;

    // Verify indexed count
    assert_eq!(indexed_count, 3, "Should have indexed 3 documents");

    // Test search functionality
    let results = text::search(&index, "product", 10)?;
    assert!(!results.is_empty(), "Should find results for 'product'");
    assert_eq!(
        results[0].title.as_deref(),
        Some("Product Strategy Meeting"),
        "Top result should be product meeting"
    );

    // Test search for different term
    let results = text::search(&index, "authentication", 10)?;
    assert!(
        !results.is_empty(),
        "Should find results for 'authentication'"
    );
    assert_eq!(
        results[0].title.as_deref(),
        Some("Engineering Standup"),
        "Should find standup meeting"
    );

    // Test search with no results
    let results = text::search(&index, "nonexistent", 10)?;
    assert!(
        results.is_empty(),
        "Should return empty for non-existent term"
    );

    Ok(())
}

#[test]
#[cfg(feature = "index")]
fn test_markdown_index_search_roundtrip() -> Result<()> {
    use muesli::index::text;

    // Create temp directory
    let temp_dir = TempDir::new().unwrap();
    let index_dir = temp_dir.path().join("index");
    fs::create_dir_all(&index_dir)?;

    // Create sample markdown path
    let md_path = temp_dir.path().join("test.md");

    // Create and index a document
    let index = text::create_or_open_index(&index_dir)?;
    text::index_markdown(
        &index,
        "doc123",
        Some("Test Document"),
        "2024-01-15",
        "This is a test document with some searchable content about machine learning and AI.",
        &md_path,
    )?;

    // Search for content
    let results = text::search(&index, "machine learning", 10)?;
    assert_eq!(results.len(), 1, "Should find exactly one document");
    assert_eq!(results[0].title.as_deref(), Some("Test Document"));

    // Search for partial match
    let results = text::search(&index, "AI", 10)?;
    assert_eq!(results.len(), 1, "Should find document with AI");

    Ok(())
}

#[test]
#[cfg(feature = "embeddings")]
fn test_semantic_search_workflow() -> Result<()> {
    use muesli::embeddings::vector::VectorStore;

    // Create temp directory
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors");

    // Create vector store (384 dimensions for e5-small-v2)
    let mut store = VectorStore::new(384);

    // Create some sample embeddings (normalized random vectors)
    // In reality these would come from the embedding engine
    let doc1_vec: Vec<f32> = (0..384).map(|i| (i as f32 * 0.01).sin()).collect();
    let doc1_vec = normalize_vector(doc1_vec);

    let doc2_vec: Vec<f32> = (0..384).map(|i| (i as f32 * 0.01).cos()).collect();
    let doc2_vec = normalize_vector(doc2_vec);

    let doc3_vec: Vec<f32> = (0..384).map(|i| ((i as f32 * 0.01) + 1.0).sin()).collect();
    let doc3_vec = normalize_vector(doc3_vec);

    // Add documents to store
    store.add_document("doc1".to_string(), doc1_vec.clone())?;
    store.add_document("doc2".to_string(), doc2_vec)?;
    store.add_document("doc3".to_string(), doc3_vec)?;

    // Save and reload
    store.save(&vector_path)?;
    let loaded_store = VectorStore::load(&vector_path)?;

    // Search with query vector similar to doc1
    let results = loaded_store.search(&doc1_vec, 3)?;

    // Verify results
    assert_eq!(results.len(), 3, "Should return top 3 results");
    assert_eq!(results[0].0, "doc1", "Top result should be doc1");
    assert!(
        results[0].1 > 0.99,
        "Self-similarity should be very high: {}",
        results[0].1
    );

    // Verify ordering by similarity
    assert!(
        results[0].1 > results[1].1,
        "Results should be ordered by similarity"
    );
    assert!(
        results[1].1 > results[2].1,
        "Results should be ordered by similarity"
    );

    Ok(())
}

/// Helper to normalize a vector (for embedding simulation)
#[cfg(feature = "embeddings")]
fn normalize_vector(vec: Vec<f32>) -> Vec<f32> {
    let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    vec.iter().map(|x| x / magnitude).collect()
}

/// Test D: ProseMirror parsing + conversion roundtrip from realistic JSON
#[test]
fn test_prosemirror_to_markdown_roundtrip() {
    let json = r#"{
        "type": "doc",
        "content": [
            {
                "type": "heading",
                "attrs": {"level": 1},
                "content": [
                    {"type": "text", "text": "Sprint Review"}
                ]
            },
            {
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "We discussed "},
                    {"type": "text", "text": "critical", "marks": [{"type": "bold"}]},
                    {"type": "text", "text": " issues and "},
                    {"type": "text", "text": "potential", "marks": [{"type": "italic"}]},
                    {"type": "text", "text": " solutions."}
                ]
            },
            {
                "type": "heading",
                "attrs": {"level": 2},
                "content": [
                    {"type": "text", "text": "Action Items"}
                ]
            },
            {
                "type": "bulletList",
                "content": [
                    {
                        "type": "listItem",
                        "content": [
                            {
                                "type": "paragraph",
                                "content": [
                                    {"type": "text", "text": "Fix the "},
                                    {"type": "text", "text": "login bug", "marks": [{"type": "bold"}]}
                                ]
                            }
                        ]
                    },
                    {
                        "type": "listItem",
                        "content": [
                            {
                                "type": "paragraph",
                                "content": [
                                    {"type": "text", "text": "Deploy to staging"}
                                ]
                            }
                        ]
                    }
                ]
            },
            {
                "type": "paragraph",
                "content": [
                    {"type": "text", "text": "Next meeting: Friday 3pm"}
                ]
            }
        ]
    }"#;

    let doc: muesli::ProseMirrorDoc = serde_json::from_str(json).unwrap();
    let md = muesli::prosemirror_to_markdown(&doc);

    // Verify the output contains expected elements
    assert!(md.contains("# Sprint Review"), "Should have h1 heading");
    assert!(md.contains("## Action Items"), "Should have h2 heading");
    assert!(md.contains("**critical**"), "Should have bold text");
    assert!(md.contains("*potential*"), "Should have italic text");
    assert!(
        md.contains("- Fix the **login bug**"),
        "Should have bold text in list item"
    );
    assert!(
        md.contains("- Deploy to staging"),
        "Should have plain list item"
    );
    assert!(
        md.contains("Next meeting: Friday 3pm"),
        "Should have trailing paragraph"
    );

    // Verify ordering: heading before paragraph before list
    let h1_pos = md.find("# Sprint Review").unwrap();
    let h2_pos = md.find("## Action Items").unwrap();
    let list_pos = md.find("- Fix the").unwrap();
    let trailing_pos = md.find("Next meeting").unwrap();
    assert!(h1_pos < h2_pos, "h1 should come before h2");
    assert!(h2_pos < list_pos, "h2 should come before list");
    assert!(
        list_pos < trailing_pos,
        "list should come before trailing paragraph"
    );
}

/// Test E: Notes + summary appear in markdown output with correct ordering
#[test]
fn test_notes_and_summary_in_markdown() {
    use muesli::model::{DocumentMetadata, RawTranscript, TranscriptEntry};

    let raw = RawTranscript {
        entries: vec![TranscriptEntry {
            document_id: Some("doc-test".into()),
            speaker: Some("Alice".into()),
            start: Some("2025-12-01T09:00:00.000Z".into()),
            end: None,
            text: "Let's get started.".into(),
            source: None,
            id: None,
            is_final: None,
        }],
    };

    let meta = DocumentMetadata {
        id: Some("doc-test".into()),
        title: Some("Integration Test Meeting".into()),
        created_at: "2025-12-01T09:00:00Z".parse().unwrap(),
        updated_at: None,
        participants: vec!["Alice".into(), "Bob".into()],
        duration_seconds: Some(1800),
        labels: vec![],
        creator: None,
        attendees: None,
    };

    let notes_md = "- Follow up on deployment\n- Review PR #42";
    let summary = "Discussed deployment timeline and code review process.";

    let output =
        muesli::to_markdown(&raw, &meta, "doc-test", Some(notes_md), Some(summary)).unwrap();

    // Verify Summary section exists
    assert!(
        output
            .body
            .contains("## Summary\n\nDiscussed deployment timeline and code review process."),
        "Body should contain Summary section"
    );

    // Verify Notes section exists
    assert!(
        output
            .body
            .contains("## Notes\n\n- Follow up on deployment\n- Review PR #42"),
        "Body should contain Notes section"
    );

    // Verify separator exists before transcript
    assert!(
        output.body.contains("---\n"),
        "Body should contain separator"
    );

    // Verify ordering: Summary before Notes before separator before transcript
    let summary_pos = output.body.find("## Summary").unwrap();
    let notes_pos = output.body.find("## Notes").unwrap();
    let separator_pos = output.body.find("---\n").unwrap();
    let transcript_pos = output.body.find("**Alice").unwrap();

    assert!(summary_pos < notes_pos, "Summary should come before Notes");
    assert!(
        notes_pos < separator_pos,
        "Notes should come before separator"
    );
    assert!(
        separator_pos < transcript_pos,
        "Separator should come before transcript"
    );

    // Verify frontmatter contains summary_text
    assert!(
        output.frontmatter_yaml.contains("summary_text"),
        "Frontmatter should contain summary_text"
    );
}

/// Test F: DB roundtrip with notes and summary
#[test]
#[cfg(feature = "storage")]
fn test_db_notes_summary_roundtrip() {
    use muesli::db::connection::open_or_create;
    use muesli::db::queries::upsert_document;
    use muesli::model::DocumentMetadata;

    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.duckdb");
    let conn = open_or_create(&db_path).unwrap();

    let meta = DocumentMetadata {
        id: Some("doc-roundtrip".into()),
        title: Some("Roundtrip Test".into()),
        created_at: "2025-12-01T09:00:00Z".parse().unwrap(),
        updated_at: Some("2025-12-02T10:00:00Z".parse().unwrap()),
        participants: vec!["Alice".into()],
        duration_seconds: Some(600),
        labels: vec!["test".into()],
        creator: None,
        attendees: None,
    };

    let notes = "## Key Decisions\n\n- Ship by Friday\n- Use feature flags";
    let summary = "Team decided to ship by Friday using feature flags.";

    upsert_document(
        &conn,
        &meta,
        "doc-roundtrip",
        "2025-12-01_roundtrip-test",
        Some(notes),
        Some(summary),
    )
    .unwrap();

    // Query it back and verify
    let (stored_notes, stored_summary): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc-roundtrip'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(stored_notes.as_deref(), Some(notes));
    assert_eq!(stored_summary.as_deref(), Some(summary));

    // Verify other document data is also intact
    let title: Option<String> = conn
        .query_row(
            "SELECT title FROM documents WHERE doc_id = 'doc-roundtrip'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title.as_deref(), Some("Roundtrip Test"));
}

/// Test G: Empty last_viewed_panel produces no notes section in markdown
#[test]
fn test_empty_last_viewed_panel_no_notes_section() {
    use muesli::model::{DocumentMetadata, RawTranscript, TranscriptEntry};

    let raw = RawTranscript {
        entries: vec![TranscriptEntry {
            document_id: None,
            speaker: Some("Alice".into()),
            start: None,
            end: None,
            text: "Hello".into(),
            source: None,
            id: None,
            is_final: None,
        }],
    };

    let meta = DocumentMetadata {
        id: Some("doc-empty-panel".into()),
        title: Some("Meeting".into()),
        created_at: "2025-12-01T09:00:00Z".parse().unwrap(),
        updated_at: None,
        participants: vec![],
        duration_seconds: None,
        labels: vec![],
        creator: None,
        attendees: None,
    };

    // Pass None for notes (simulating empty/missing last_viewed_panel)
    let output = muesli::to_markdown(&raw, &meta, "doc-empty-panel", None, None).unwrap();
    assert!(
        !output.body.contains("## Notes"),
        "Should not have Notes section when no notes provided"
    );

    // Pass empty string for notes (simulating ProseMirror doc with no content)
    let output = muesli::to_markdown(&raw, &meta, "doc-empty-panel", Some(""), None).unwrap();
    assert!(
        !output.body.contains("## Notes"),
        "Should not have Notes section when notes is empty string"
    );
}

/// Test H: Empty summary_text produces no summary section in markdown
#[test]
fn test_empty_summary_text_no_summary_section() {
    use muesli::model::{DocumentMetadata, RawTranscript, TranscriptEntry};

    let raw = RawTranscript {
        entries: vec![TranscriptEntry {
            document_id: None,
            speaker: Some("Alice".into()),
            start: None,
            end: None,
            text: "Hello".into(),
            source: None,
            id: None,
            is_final: None,
        }],
    };

    let meta = DocumentMetadata {
        id: Some("doc-empty-summary".into()),
        title: Some("Meeting".into()),
        created_at: "2025-12-01T09:00:00Z".parse().unwrap(),
        updated_at: None,
        participants: vec![],
        duration_seconds: None,
        labels: vec![],
        creator: None,
        attendees: None,
    };

    // Pass None for summary
    let output = muesli::to_markdown(&raw, &meta, "doc-empty-summary", None, None).unwrap();
    assert!(
        !output.body.contains("## Summary"),
        "Should not have Summary section when summary is None"
    );

    // Pass empty string for summary
    let output = muesli::to_markdown(&raw, &meta, "doc-empty-summary", None, Some("")).unwrap();
    assert!(
        !output.body.contains("## Summary"),
        "Should not have Summary section when summary is empty string"
    );
}

/// Test I: list_all_attendees returns empty vec when no attendees exist
#[test]
#[cfg(feature = "storage")]
fn test_list_all_attendees_empty_db() {
    use muesli::db::connection::open_or_create;
    use muesli::db::queries::list_all_attendees;

    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.duckdb");
    let conn = open_or_create(&db_path).unwrap();

    // Verify empty database returns empty list
    let attendees = list_all_attendees(&conn).unwrap();
    assert!(
        attendees.is_empty(),
        "Should return empty vec when no attendees exist"
    );
}

/// Test I (extended): list_all_attendees returns empty when docs exist but no attendees
#[test]
#[cfg(feature = "storage")]
fn test_list_all_attendees_docs_without_attendees() {
    use duckdb::params;
    use muesli::db::connection::open_or_create;
    use muesli::db::queries::list_all_attendees;

    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.duckdb");
    let conn = open_or_create(&db_path).unwrap();

    // Insert a document without attendees
    conn.execute(
        "INSERT INTO documents (doc_id, title, created_at) VALUES (?, ?, ?)",
        params!["doc1", "Test Meeting", "2025-12-01T09:00:00Z"],
    )
    .unwrap();

    let attendees = list_all_attendees(&conn).unwrap();
    assert!(
        attendees.is_empty(),
        "Should return empty vec when documents exist but no attendees"
    );
}

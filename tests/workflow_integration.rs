// ABOUTME: Integration tests for end-to-end workflows
// ABOUTME: Tests reindex, search, and semantic search without API mocking

use muesli::Result;
use std::fs;
use tempfile::TempDir;

/// Helper to create a sample markdown file with frontmatter
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

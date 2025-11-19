// ABOUTME: Tantivy implementation for full-text search indexing
// ABOUTME: Provides schema definition and document indexing functions

use crate::error::{Error, Result};
use std::path::Path;
use tantivy::schema::{Schema, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, Term};

/// Represents a search result from the index
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub doc_id: String,
    pub title: Option<String>,
    pub date: String,
    pub path: String,
    pub score: f32,
}

/// Creates or opens a Tantivy index at the specified directory
pub fn create_or_open_index(index_dir: &Path) -> Result<Index> {
    // Create directory if it doesn't exist
    std::fs::create_dir_all(index_dir)?;

    // Try to open existing index first
    if let Ok(index) = Index::open_in_dir(index_dir) {
        return Ok(index);
    }

    // Create new index with schema
    let mut schema_builder = Schema::builder();

    // doc_id: STRING, STORED - primary key
    schema_builder.add_text_field("doc_id", STRING | STORED);

    // title: TEXT, STORED - analyzed for search and retrievable
    schema_builder.add_text_field("title", TEXT | STORED);

    // date: STRING, STORED - for sorting
    schema_builder.add_text_field("date", STRING | STORED);

    // body: TEXT - full markdown content
    schema_builder.add_text_field("body", TEXT);

    // path: STRING, STORED - absolute path to .md
    schema_builder.add_text_field("path", STRING | STORED);

    let schema = schema_builder.build();

    Index::create_in_dir(index_dir, schema)
        .map_err(|e| Error::Indexing(format!("Failed to create index: {}", e)))
}

/// Indexes a markdown document with upsert semantics (delete old + insert new)
/// This function creates its own writer and commits immediately.
/// For batch operations, use `index_markdown_batch` instead.
pub fn index_markdown(
    index: &Index,
    doc_id: &str,
    title: Option<&str>,
    date: &str,
    body: &str,
    path: &Path,
) -> Result<()> {
    let mut writer = index
        .writer(50_000_000)
        .map_err(|e| Error::Indexing(format!("Failed to create index writer: {}", e)))?;

    index_markdown_batch(&mut writer, index, doc_id, title, date, body, path)?;

    // Commit the changes
    writer
        .commit()
        .map_err(|e| Error::Indexing(format!("Failed to commit: {}", e)))?;

    Ok(())
}

/// Indexes a markdown document using an existing writer (for batch operations)
/// Does not commit - caller must call writer.commit() when ready
pub fn index_markdown_batch(
    writer: &mut tantivy::IndexWriter,
    index: &Index,
    doc_id: &str,
    title: Option<&str>,
    date: &str,
    body: &str,
    path: &Path,
) -> Result<()> {
    let schema = index.schema();

    let doc_id_field = schema
        .get_field("doc_id")
        .map_err(|e| Error::Indexing(format!("Missing doc_id field: {}", e)))?;
    let title_field = schema
        .get_field("title")
        .map_err(|e| Error::Indexing(format!("Missing title field: {}", e)))?;
    let date_field = schema
        .get_field("date")
        .map_err(|e| Error::Indexing(format!("Missing date field: {}", e)))?;
    let body_field = schema
        .get_field("body")
        .map_err(|e| Error::Indexing(format!("Missing body field: {}", e)))?;
    let path_field = schema
        .get_field("path")
        .map_err(|e| Error::Indexing(format!("Missing path field: {}", e)))?;

    // Delete any existing document with the same doc_id (upsert)
    let term = Term::from_field_text(doc_id_field, doc_id);
    writer.delete_term(term);

    // Build the new document
    let path_str = path.to_string_lossy().to_string();

    let mut document = doc!(
        doc_id_field => doc_id,
        date_field => date,
        body_field => body,
        path_field => path_str,
    );

    // Add title if present
    if let Some(t) = title {
        document.add_text(title_field, t);
    }

    // Add the document
    writer
        .add_document(document)
        .map_err(|e| Error::Indexing(format!("Failed to add document: {}", e)))?;

    Ok(())
}

/// Searches the index using BM25 ranking
///
/// Searches both title and body fields with the given query string.
/// Returns top N results sorted by relevance score (highest first).
pub fn search(index: &Index, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    use tantivy::collector::TopDocs;
    use tantivy::query::QueryParser;

    let schema = index.schema();

    // Get the fields we want to search
    let title_field = schema
        .get_field("title")
        .map_err(|e| Error::Indexing(format!("Missing title field: {}", e)))?;
    let body_field = schema
        .get_field("body")
        .map_err(|e| Error::Indexing(format!("Missing body field: {}", e)))?;

    // Get the stored fields for results
    let doc_id_field = schema
        .get_field("doc_id")
        .map_err(|e| Error::Indexing(format!("Missing doc_id field: {}", e)))?;
    let date_field = schema
        .get_field("date")
        .map_err(|e| Error::Indexing(format!("Missing date field: {}", e)))?;
    let path_field = schema
        .get_field("path")
        .map_err(|e| Error::Indexing(format!("Missing path field: {}", e)))?;

    // Create reader and searcher
    let reader = index
        .reader()
        .map_err(|e| Error::Indexing(format!("Failed to create reader: {}", e)))?;
    let searcher = reader.searcher();

    // Parse the query - search both title and body fields
    let query_parser = QueryParser::for_index(index, vec![title_field, body_field]);
    let parsed_query = query_parser
        .parse_query(query)
        .map_err(|e| Error::Indexing(format!("Failed to parse query '{}': {}", query, e)))?;

    // Execute the search with BM25 scoring (default in Tantivy)
    let top_docs = searcher
        .search(&parsed_query, &TopDocs::with_limit(limit))
        .map_err(|e| Error::Indexing(format!("Search failed: {}", e)))?;

    // Convert results to SearchResult structs
    let mut results = Vec::new();
    for (score, doc_address) in top_docs {
        let retrieved_doc = searcher
            .doc::<tantivy::TantivyDocument>(doc_address)
            .map_err(|e| Error::Indexing(format!("Failed to retrieve document: {}", e)))?;

        // Extract fields from the document
        let doc_id = retrieved_doc
            .get_first(doc_id_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Indexing("Document missing doc_id".to_string()))?
            .to_string();

        let title = retrieved_doc
            .get_first(title_field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let date = retrieved_doc
            .get_first(date_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Indexing("Document missing date".to_string()))?
            .to_string();

        let path = retrieved_doc
            .get_first(path_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Indexing("Document missing path".to_string()))?
            .to_string();

        results.push(SearchResult {
            doc_id,
            title,
            date,
            path,
            score,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Test helper: creates a temporary directory for test indexes
    fn test_index_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp dir")
    }

    #[test]
    fn test_schema_creation() {
        // Test that we can create a new index with the correct schema
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();

        let index = create_or_open_index(index_path).expect("Failed to create index");
        let schema = index.schema();

        // Verify all 5 required fields exist
        assert!(schema.get_field("doc_id").is_ok(), "doc_id field missing");
        assert!(schema.get_field("title").is_ok(), "title field missing");
        assert!(schema.get_field("date").is_ok(), "date field missing");
        assert!(schema.get_field("body").is_ok(), "body field missing");
        assert!(schema.get_field("path").is_ok(), "path field missing");
    }

    #[test]
    fn test_schema_reopening() {
        // Test that we can reopen an existing index
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();

        // Create the index
        let _index1 = create_or_open_index(index_path).expect("Failed to create index");

        // Reopen the index
        let index2 = create_or_open_index(index_path).expect("Failed to reopen index");
        let schema = index2.schema();

        // Verify fields still exist
        assert!(
            schema.get_field("doc_id").is_ok(),
            "doc_id field missing after reopen"
        );
    }

    #[test]
    fn test_index_document() {
        // Test indexing a single document
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        let doc_path = Path::new("/test/documents/test.md");
        let result = index_markdown(
            &index,
            "doc123",
            Some("Test Document"),
            "2025-10-29",
            "This is the body of the test document.",
            doc_path,
        );

        assert!(result.is_ok(), "Failed to index document: {:?}", result);
    }

    #[test]
    fn test_index_document_without_title() {
        // Test indexing a document without a title
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        let doc_path = Path::new("/test/documents/notitle.md");
        let result = index_markdown(
            &index,
            "doc456",
            None,
            "2025-10-29",
            "Document without a title.",
            doc_path,
        );

        assert!(
            result.is_ok(),
            "Failed to index document without title: {:?}",
            result
        );
    }

    #[test]
    fn test_upsert_document() {
        // Test that indexing the same doc_id twice updates (not duplicates)
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");
        let doc_path = Path::new("/test/documents/update.md");

        // Index first version
        index_markdown(
            &index,
            "doc789",
            Some("Original Title"),
            "2025-10-29",
            "Original body content.",
            doc_path,
        )
        .expect("Failed to index original document");

        // Update with same doc_id
        index_markdown(
            &index,
            "doc789",
            Some("Updated Title"),
            "2025-10-30",
            "Updated body content.",
            doc_path,
        )
        .expect("Failed to update document");

        // Search to verify only one document exists with doc_id="doc789"
        use tantivy::collector::TopDocs;
        use tantivy::query::QueryParser;

        let reader = index.reader().expect("Failed to create reader");
        let searcher = reader.searcher();
        let schema = index.schema();

        let doc_id_field = schema.get_field("doc_id").unwrap();
        let query_parser = QueryParser::for_index(&index, vec![doc_id_field]);
        let query = query_parser
            .parse_query("doc789")
            .expect("Failed to parse query");

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(10))
            .expect("Search failed");

        // Should only have 1 document (the updated one)
        assert_eq!(
            top_docs.len(),
            1,
            "Expected exactly 1 document after upsert, found {}",
            top_docs.len()
        );
    }

    #[test]
    fn test_search_indexed_content() {
        // Test that we can search and find indexed documents
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index multiple documents
        index_markdown(
            &index,
            "doc1",
            Some("Rust Programming"),
            "2025-10-29",
            "Rust is a systems programming language that runs blazingly fast.",
            Path::new("/test/rust.md"),
        )
        .expect("Failed to index doc1");

        index_markdown(
            &index,
            "doc2",
            Some("Python Basics"),
            "2025-10-28",
            "Python is a high-level programming language.",
            Path::new("/test/python.md"),
        )
        .expect("Failed to index doc2");

        // Commit and search
        use tantivy::collector::TopDocs;
        use tantivy::query::QueryParser;

        let reader = index.reader().expect("Failed to create reader");
        let searcher = reader.searcher();
        let schema = index.schema();

        let body_field = schema.get_field("body").unwrap();
        let title_field = schema.get_field("title").unwrap();

        let query_parser = QueryParser::for_index(&index, vec![title_field, body_field]);
        let query = query_parser
            .parse_query("rust")
            .expect("Failed to parse query");

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(10))
            .expect("Search failed");

        // Should find the Rust document
        assert!(
            !top_docs.is_empty(),
            "Expected to find at least one document for 'rust'"
        );
        assert_eq!(
            top_docs.len(),
            1,
            "Expected exactly 1 document matching 'rust'"
        );
    }

    #[test]
    fn test_search_single_term() {
        // Test searching with a single term
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index test documents
        index_markdown(
            &index,
            "doc1",
            Some("OKRs for Q4"),
            "2025-10-29",
            "Discussing quarterly OKRs and objectives.",
            Path::new("/test/okrs.md"),
        )
        .expect("Failed to index doc1");

        index_markdown(
            &index,
            "doc2",
            Some("Onboarding Process"),
            "2025-10-28",
            "Steps for employee onboarding.",
            Path::new("/test/onboarding.md"),
        )
        .expect("Failed to index doc2");

        index_markdown(
            &index,
            "doc3",
            Some("Team Meeting"),
            "2025-10-27",
            "General team updates.",
            Path::new("/test/team.md"),
        )
        .expect("Failed to index doc3");

        // Search for "OKRs"
        let results = super::search(&index, "OKRs", 10).expect("Search failed");

        assert!(!results.is_empty(), "Expected to find results for 'OKRs'");
        assert_eq!(results[0].doc_id, "doc1", "Expected doc1 to be top result");
        assert_eq!(results[0].title, Some("OKRs for Q4".to_string()));
        assert_eq!(results[0].date, "2025-10-29");
        assert!(results[0].score > 0.0, "Expected positive score");
    }

    #[test]
    fn test_search_multi_term() {
        // Test searching with multiple terms
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index test documents
        index_markdown(
            &index,
            "doc1",
            Some("OKRs Discussion"),
            "2025-10-29",
            "Talking about onboarding and OKRs together.",
            Path::new("/test/both.md"),
        )
        .expect("Failed to index doc1");

        index_markdown(
            &index,
            "doc2",
            Some("Onboarding Process"),
            "2025-10-28",
            "Steps for employee onboarding only.",
            Path::new("/test/onboarding.md"),
        )
        .expect("Failed to index doc2");

        index_markdown(
            &index,
            "doc3",
            Some("Team Meeting"),
            "2025-10-27",
            "General team updates, no relevant content.",
            Path::new("/test/team.md"),
        )
        .expect("Failed to index doc3");

        // Search for "OKRs onboarding"
        let results = super::search(&index, "OKRs onboarding", 10).expect("Search failed");

        assert!(!results.is_empty(), "Expected to find results");
        // doc1 should rank higher because it contains both terms
        assert_eq!(
            results[0].doc_id, "doc1",
            "Expected doc1 with both terms to rank highest"
        );
        assert!(results.len() >= 2, "Expected at least 2 results");
    }

    #[test]
    fn test_search_partial_match() {
        // Test searching with tokenized matches
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index document with "programming guide"
        index_markdown(
            &index,
            "doc1",
            Some("Programming Guide"),
            "2025-10-29",
            "A guide to programming in various languages.",
            Path::new("/test/programming.md"),
        )
        .expect("Failed to index doc1");

        // Search for just "guide" (matches "Programming Guide")
        let results = super::search(&index, "guide", 10).expect("Search failed");

        assert!(
            !results.is_empty(),
            "Expected to find results for partial match"
        );
        assert_eq!(results[0].doc_id, "doc1");
    }

    #[test]
    fn test_search_bm25_ranking() {
        // Test that BM25 ranking prioritizes more relevant documents
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Document with "rust" in title and multiple times in body
        index_markdown(
            &index,
            "doc1",
            Some("Rust Programming Language"),
            "2025-10-29",
            "Rust is great. Rust is fast. Rust is safe. All about Rust programming.",
            Path::new("/test/rust-deep.md"),
        )
        .expect("Failed to index doc1");

        // Document with "rust" only once in body
        index_markdown(
            &index,
            "doc2",
            Some("Programming Languages Overview"),
            "2025-10-28",
            "Various languages including Rust are discussed here.",
            Path::new("/test/overview.md"),
        )
        .expect("Failed to index doc2");

        // Search for "rust"
        let results = super::search(&index, "rust", 10).expect("Search failed");

        assert!(results.len() >= 2, "Expected at least 2 results");
        // doc1 should rank higher due to more occurrences and title match
        assert_eq!(results[0].doc_id, "doc1", "Expected doc1 to rank higher");
        assert!(
            results[0].score > results[1].score,
            "Expected doc1 to have higher score"
        );
    }

    #[test]
    fn test_search_limit() {
        // Test that the limit parameter works correctly
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index 5 documents
        for i in 1..=5 {
            index_markdown(
                &index,
                &format!("doc{}", i),
                Some(&format!("Document {}", i)),
                "2025-10-29",
                "This document contains the word test for searching.",
                Path::new(&format!("/test/doc{}.md", i)),
            )
            .expect(&format!("Failed to index doc{}", i));
        }

        // Search with limit 3
        let results = super::search(&index, "test", 3).expect("Search failed");

        assert_eq!(results.len(), 3, "Expected exactly 3 results with limit=3");
    }

    #[test]
    fn test_search_no_results() {
        // Test searching when no documents match
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Index a document
        index_markdown(
            &index,
            "doc1",
            Some("Rust Programming"),
            "2025-10-29",
            "All about Rust.",
            Path::new("/test/rust.md"),
        )
        .expect("Failed to index doc1");

        // Search for something that doesn't exist
        let results = super::search(&index, "xyznonexistent", 10).expect("Search failed");

        assert!(
            results.is_empty(),
            "Expected no results for non-matching query"
        );
    }

    #[test]
    fn test_search_empty_index() {
        // Test searching an empty index
        let temp_dir = test_index_dir();
        let index_path = temp_dir.path();
        let index = create_or_open_index(index_path).expect("Failed to create index");

        // Search without indexing any documents
        let results = super::search(&index, "anything", 10).expect("Search failed");

        assert!(results.is_empty(), "Expected no results from empty index");
    }
}

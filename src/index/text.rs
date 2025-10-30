// ABOUTME: Tantivy implementation for full-text search indexing
// ABOUTME: Provides schema definition and document indexing functions

use crate::error::{Error, Result};
use std::path::Path;
use tantivy::schema::{Schema, STORED, TEXT, STRING};
use tantivy::{doc, Index, Term};

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

    // title: TEXT - analyzed for search
    schema_builder.add_text_field("title", TEXT);

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
    let mut writer = index.writer(50_000_000)
        .map_err(|e| Error::Indexing(format!("Failed to create index writer: {}", e)))?;

    index_markdown_batch(&mut writer, index, doc_id, title, date, body, path)?;

    // Commit the changes
    writer.commit()
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

    let doc_id_field = schema.get_field("doc_id")
        .map_err(|e| Error::Indexing(format!("Missing doc_id field: {}", e)))?;
    let title_field = schema.get_field("title")
        .map_err(|e| Error::Indexing(format!("Missing title field: {}", e)))?;
    let date_field = schema.get_field("date")
        .map_err(|e| Error::Indexing(format!("Missing date field: {}", e)))?;
    let body_field = schema.get_field("body")
        .map_err(|e| Error::Indexing(format!("Missing body field: {}", e)))?;
    let path_field = schema.get_field("path")
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
    writer.add_document(document)
        .map_err(|e| Error::Indexing(format!("Failed to add document: {}", e)))?;

    Ok(())
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
        assert!(schema.get_field("doc_id").is_ok(), "doc_id field missing after reopen");
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

        assert!(result.is_ok(), "Failed to index document without title: {:?}", result);
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
        let query = query_parser.parse_query("doc789").expect("Failed to parse query");

        let top_docs = searcher.search(&query, &TopDocs::with_limit(10)).expect("Search failed");

        // Should only have 1 document (the updated one)
        assert_eq!(top_docs.len(), 1, "Expected exactly 1 document after upsert, found {}", top_docs.len());
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
        let query = query_parser.parse_query("rust").expect("Failed to parse query");

        let top_docs = searcher.search(&query, &TopDocs::with_limit(10)).expect("Search failed");

        // Should find the Rust document
        assert!(!top_docs.is_empty(), "Expected to find at least one document for 'rust'");
        assert_eq!(top_docs.len(), 1, "Expected exactly 1 document matching 'rust'");
    }
}

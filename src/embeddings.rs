// ABOUTME: Local embedding engine using ONNX Runtime
// ABOUTME: Implements e5-small-v2 model with query/passage prefixes

#[cfg(feature = "embeddings")]
pub mod engine;

#[cfg(feature = "embeddings")]
pub mod vector;

#[cfg(feature = "embeddings")]
pub mod downloader;

#[cfg(feature = "embeddings")]
pub use downloader::{ensure_model, ModelPaths};

#[cfg(feature = "embeddings")]
pub use engine::EmbeddingEngine;

#[cfg(feature = "embeddings")]
pub use vector::VectorStore;

#[cfg(feature = "embeddings")]
use crate::{storage::Paths, Result};

/// Search result with document metadata
#[cfg(feature = "embeddings")]
pub struct SearchResult {
    pub doc_id: String,
    pub title: Option<String>,
    pub date: String,
    pub path: String,
    pub score: f32,
}

/// Perform semantic search using embeddings
#[cfg(feature = "embeddings")]
pub fn semantic_search(
    paths: &Paths,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    use crate::storage::read_frontmatter;
    use std::fs;

    // Load the embedding engine
    let model_paths = downloader::ensure_model(&paths.models_dir)?;
    let mut engine = engine::EmbeddingEngine::new(&model_paths.model_path, &model_paths.tokenizer_path)?;

    // Generate query embedding
    let query_vec = engine.embed_query(query)?;

    // Load vector store
    let vector_path = paths.index_dir.join("vectors");
    let vector_store = vector::VectorStore::load(&vector_path)?;

    // Perform search
    let raw_results = vector_store.search(&query_vec, top_k)?;

    // Build a map of doc_id -> markdown file
    let mut results = Vec::new();

    for (doc_id, score) in raw_results {
        // Find the markdown file for this doc_id
        // Files are named: YYYY-MM-DD_slug.md
        // We need to search transcripts_dir for files containing this doc_id in frontmatter

        // For now, try to find by checking all markdown files
        let mut found = false;

        if let Ok(entries) = fs::read_dir(&paths.transcripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(Some(fm)) = read_frontmatter(&path) {
                        if fm.doc_id == doc_id {
                            results.push(SearchResult {
                                doc_id: doc_id.clone(),
                                title: fm.title,
                                date: fm.created_at.format("%Y-%m-%d").to_string(),
                                path: path.display().to_string(),
                                score,
                            });
                            found = true;
                            break;
                        }
                    }
                }
            }
        }

        // If we couldn't find the file, still include the result with minimal info
        if !found {
            results.push(SearchResult {
                doc_id: doc_id.clone(),
                title: None,
                date: "unknown".to_string(),
                path: "unknown".to_string(),
                score,
            });
        }
    }

    Ok(results)
}

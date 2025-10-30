// ABOUTME: Text search indexing module providing full-text search capabilities
// ABOUTME: Feature-gated module for Tantivy-based search indexing

#[cfg(feature = "index")]
pub mod text;

#[cfg(feature = "index")]
pub use text::{create_or_open_index, index_markdown};

// ABOUTME: Vector storage with cosine similarity search
// ABOUTME: Uses linear search for simplicity (HNSW can be added later)

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMapping {
    pub doc_id: String,
    pub offset: usize,
}

pub struct VectorStore {
    vectors: Vec<f32>,
    mapping: Vec<VectorMapping>,
    dim: usize,
}

impl VectorStore {
    pub fn new(dim: usize) -> Self {
        VectorStore {
            vectors: Vec::new(),
            mapping: Vec::new(),
            dim,
        }
    }

    pub fn has_document(&self, doc_id: &str) -> bool {
        self.mapping.iter().any(|m| m.doc_id == doc_id)
    }

    pub fn add_document(&mut self, doc_id: String, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dim {
            return Err(Error::Filesystem(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Vector dimension mismatch: expected {}, got {}",
                    self.dim,
                    vector.len()
                ),
            )));
        }

        let offset = self.vectors.len();

        self.mapping.push(VectorMapping { doc_id, offset });
        self.vectors.extend_from_slice(&vector);

        Ok(())
    }

    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Result<Vec<(String, f32)>> {
        if query_vec.len() != self.dim {
            return Err(Error::Filesystem(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Query vector dimension mismatch: expected {}, got {}",
                    self.dim,
                    query_vec.len()
                ),
            )));
        }

        let mut scores: Vec<(String, f32)> = self
            .mapping
            .iter()
            .map(|mapping| {
                let vec_start = mapping.offset;
                let vec_end = vec_start + self.dim;
                let doc_vector = &self.vectors[vec_start..vec_end];
                let similarity = cosine_similarity(query_vec, doc_vector);
                (mapping.doc_id.clone(), similarity)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        Ok(scores)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        #[derive(Serialize)]
        struct Metadata {
            dim: usize,
            mapping: Vec<VectorMapping>,
        }

        let metadata = Metadata {
            dim: self.dim,
            mapping: self.mapping.clone(),
        };

        let metadata_path = path.with_extension("meta.json");
        let vectors_path = path.with_extension("vectors.bin");

        // Save metadata
        let metadata_json = serde_json::to_string(&metadata)?;
        fs::write(&metadata_path, metadata_json)?;

        // Save vectors
        let vectors_bytes: Vec<u8> = self
            .vectors
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        fs::write(&vectors_path, vectors_bytes)?;

        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct Metadata {
            dim: usize,
            mapping: Vec<VectorMapping>,
        }

        let metadata_path = path.with_extension("meta.json");
        let vectors_path = path.with_extension("vectors.bin");

        // Load metadata
        let metadata_json = fs::read_to_string(&metadata_path)?;
        let metadata: Metadata = serde_json::from_str(&metadata_json)?;

        // Load vectors
        let vectors_bytes = fs::read(&vectors_path)?;
        let mut vectors = Vec::with_capacity(vectors_bytes.len() / 4);
        for chunk in vectors_bytes.chunks_exact(4) {
            let bytes: [u8; 4] = chunk.try_into().map_err(|_| {
                Error::Filesystem(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid vector data",
                ))
            })?;
            vectors.push(f32::from_le_bytes(bytes));
        }

        Ok(VectorStore {
            vectors,
            mapping: metadata.mapping,
            dim: metadata.dim,
        })
    }

    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_normalized_vector(values: &[f32]) -> Vec<f32> {
        let norm: f32 = values.iter().map(|x| x * x).sum::<f32>().sqrt();
        values.iter().map(|x| x / norm).collect()
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);

        let d = vec![0.7071, 0.7071, 0.0];
        assert!(cosine_similarity(&a, &d) > 0.7);
    }

    #[test]
    fn test_vector_store_creation() {
        let store = VectorStore::new(384);
        assert_eq!(store.dim, 384);
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn test_add_document() {
        let mut store = VectorStore::new(3);
        let vec = create_normalized_vector(&[1.0, 0.0, 0.0]);
        store.add_document("doc1".into(), vec).unwrap();

        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }

    #[test]
    fn test_dimension_mismatch() {
        let mut store = VectorStore::new(3);
        let vec = vec![1.0, 0.0]; // Wrong dimension

        let result = store.add_document("doc1".into(), vec);
        assert!(result.is_err());
    }

    #[test]
    fn test_search() {
        let mut store = VectorStore::new(3);

        // Add three normalized vectors
        let vec1 = create_normalized_vector(&[1.0, 0.0, 0.0]);
        let vec2 = create_normalized_vector(&[0.0, 1.0, 0.0]);
        let vec3 = create_normalized_vector(&[0.7, 0.7, 0.0]);

        store.add_document("doc1".into(), vec1.clone()).unwrap();
        store.add_document("doc2".into(), vec2).unwrap();
        store.add_document("doc3".into(), vec3).unwrap();

        // Search with query similar to doc1
        let query = create_normalized_vector(&[0.9, 0.1, 0.0]);
        let results = store.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        // doc1 should be most similar
        assert_eq!(results[0].0, "doc1");
        assert!(results[0].1 > 0.9); // High similarity
    }

    #[test]
    fn test_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store_path = temp.path().join("vectors");

        // Create and populate store
        let mut store = VectorStore::new(3);
        let vec1 = create_normalized_vector(&[1.0, 0.0, 0.0]);
        let vec2 = create_normalized_vector(&[0.0, 1.0, 0.0]);

        store.add_document("doc1".into(), vec1).unwrap();
        store.add_document("doc2".into(), vec2).unwrap();

        // Save
        store.save(&store_path).unwrap();

        // Load
        let loaded_store = VectorStore::load(&store_path).unwrap();

        assert_eq!(loaded_store.dim, 3);
        assert_eq!(loaded_store.len(), 2);

        // Verify search still works
        let query = create_normalized_vector(&[1.0, 0.0, 0.0]);
        let results = loaded_store.search(&query, 1).unwrap();
        assert_eq!(results[0].0, "doc1");
    }

    #[test]
    fn test_empty_search() {
        let store = VectorStore::new(3);
        let query = create_normalized_vector(&[1.0, 0.0, 0.0]);
        let results = store.search(&query, 10).unwrap();
        assert_eq!(results.len(), 0);
    }
}

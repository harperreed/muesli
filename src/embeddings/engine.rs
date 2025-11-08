// ABOUTME: ONNX embedding engine for e5-small-v2 model
// ABOUTME: Handles tokenization, inference, and mean pooling for sentence embeddings

use crate::{Error, Result};
use ort::{inputs, session::Session, value::Value};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

const E5_DIM: usize = 384;
const MAX_LENGTH: usize = 512;

pub struct EmbeddingEngine {
    session: Session,
    tokenizer: Arc<Tokenizer>,
}

impl EmbeddingEngine {
    pub fn new(model_path: &Path, tokenizer_path: &Path) -> Result<Self> {
        // Initialize ort globally (idempotent)
        ort::init()
            .commit()
            .map_err(|e| Error::Embedding(format!("Failed to initialize ort: {}", e)))?;

        // Load tokenizer
        let tokenizer = Arc::new(Tokenizer::from_file(tokenizer_path).map_err(|e| {
            Error::Embedding(format!("Failed to load tokenizer: {}", e))
        })?);

        // Create session - read model into memory first
        let model_bytes = std::fs::read(model_path)
            .map_err(|e| Error::Embedding(format!("Failed to read model file: {}", e)))?;

        let session = Session::builder()
            .map_err(|e| Error::Embedding(format!("Failed to create session builder: {}", e)))?
            .commit_from_memory(&model_bytes)
            .map_err(|e| Error::Embedding(format!("Failed to load ONNX model: {}", e)))?;

        Ok(EmbeddingEngine { session, tokenizer })
    }

    pub fn dim(&self) -> usize {
        E5_DIM
    }

    pub fn embed_query(&mut self, text: &str) -> Result<Vec<f32>> {
        // e5 models use "query: " prefix for queries
        let prefixed = format!("query: {}", text);
        self.embed_text(&prefixed)
    }

    pub fn embed_passage(&mut self, text: &str) -> Result<Vec<f32>> {
        // e5 models use "passage: " prefix for passages
        let prefixed = format!("passage: {}", text);
        self.embed_text(&prefixed)
    }

    fn embed_text(&mut self, text: &str) -> Result<Vec<f32>> {
        // Tokenize
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| Error::Embedding(format!("Tokenization failed: {}", e)))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        // Truncate to max length
        let len = input_ids.len().min(MAX_LENGTH);
        let input_ids = &input_ids[..len];
        let attention_mask = &attention_mask[..len];

        // Convert to i64 arrays (ONNX expects i64)
        let input_ids_i64: Vec<i64> = input_ids.iter().map(|&id| id as i64).collect();
        let attention_mask_i64: Vec<i64> =
            attention_mask.iter().map(|&mask| mask as i64).collect();

        // Create Value tensors - ort 2.0 expects (shape, data) tuple
        let input_ids_value = Value::from_array((vec![1, len], input_ids_i64))
            .map_err(|e| Error::Embedding(format!("Failed to create input_ids tensor: {}", e)))?;

        let attention_mask_value = Value::from_array((vec![1, len], attention_mask_i64.clone()))
            .map_err(|e| Error::Embedding(format!("Failed to create attention_mask tensor: {}", e)))?;

        // Create token_type_ids (all zeros for single sequence)
        let token_type_ids: Vec<i64> = vec![0; len];
        let token_type_ids_value = Value::from_array((vec![1, len], token_type_ids))
            .map_err(|e| Error::Embedding(format!("Failed to create token_type_ids tensor: {}", e)))?;

        // Run inference using ort 2.0 API
        let outputs = self
            .session
            .run(inputs![
                "input_ids" => input_ids_value,
                "attention_mask" => attention_mask_value,
                "token_type_ids" => token_type_ids_value
            ])
            .map_err(|e| Error::Embedding(format!("ONNX inference failed: {}", e)))?;

        // Extract embeddings - ort 2.0 returns (shape, data) tuple
        let (shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| Error::Embedding(format!("Failed to extract output tensor: {}", e)))?;

        // shape should be [1, seq_len, 384]
        if shape.len() != 3 {
            return Err(Error::Embedding(format!(
                "Unexpected output shape: expected 3 dimensions, got {}",
                shape.len()
            )));
        }

        let batch_size = shape[0];
        let seq_len = shape[1] as usize;
        let hidden_dim = shape[2] as usize;

        if batch_size != 1 || hidden_dim != E5_DIM {
            return Err(Error::Embedding(format!(
                "Unexpected output shape: got [{}, {}, {}], expected [1, {}, {}]",
                batch_size, seq_len, hidden_dim, seq_len, E5_DIM
            )));
        }

        // Mean pooling with attention mask
        let embedding = mean_pool(data, seq_len, hidden_dim, attention_mask)?;

        // Normalize
        Ok(normalize_vector(embedding))
    }
}

fn mean_pool(
    data: &[f32],
    seq_len: usize,
    hidden_dim: usize,
    attention_mask: &[u32],
) -> Result<Vec<f32>> {
    // data is flattened [1, seq_len, hidden_dim] tensor
    // Skip first batch dimension (always 1), start at seq_len * hidden_dim
    let mut pooled = vec![0.0f32; hidden_dim];
    let mut mask_sum = 0.0f32;

    for (i, &mask) in attention_mask.iter().enumerate().take(seq_len) {
        if mask > 0 {
            let offset = i * hidden_dim;
            for j in 0..hidden_dim {
                pooled[j] += data[offset + j];
            }
            mask_sum += 1.0;
        }
    }

    if mask_sum > 0.0 {
        for val in pooled.iter_mut() {
            *val /= mask_sum;
        }
    }

    Ok(pooled)
}

fn normalize_vector(mut vec: Vec<f32>) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in vec.iter_mut() {
            *val /= norm;
        }
    }
    vec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_vector() {
        let vec = vec![3.0, 4.0];
        let normalized = normalize_vector(vec);
        assert!((normalized[0] - 0.6).abs() < 0.001);
        assert!((normalized[1] - 0.8).abs() < 0.001);

        // Check unit length
        let length: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_engine_dimension() {
        // e5-small-v2 dimension
        assert_eq!(384, 384);
    }
}

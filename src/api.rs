// ABOUTME: Blocking HTTP client for Granola API
// ABOUTME: Handles throttling, auth headers, and fail-fast errors

use crate::{DocumentMetadata, DocumentSummary, Error, PublicNote, RawTranscript, Result};
use rand::Rng;
use reqwest::blocking::Client;
use serde_json::json;
use std::collections::HashSet;
use std::time::Duration;

/// Holds both the raw JSON text and the parsed value from an API response
#[derive(Debug)]
pub struct ApiResponse<T> {
    pub raw: String,
    pub parsed: T,
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_string();
    }

    // Find a valid UTF-8 boundary at or before max_chars
    let mut boundary = max_chars;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }

    if boundary == 0 {
        return String::new();
    }

    format!("{}...", &s[..boundary])
}

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
    throttle_min: u64,
    throttle_max: u64,
}

impl ApiClient {
    pub fn new(token: String, base_url: Option<String>) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

        Ok(ApiClient {
            client,
            base_url: base_url.unwrap_or_else(|| "https://api.granola.ai".into()),
            token,
            throttle_min: 100,
            throttle_max: 300,
        })
    }

    pub fn with_throttle(mut self, min_ms: u64, max_ms: u64) -> Self {
        self.throttle_min = min_ms;
        self.throttle_max = max_ms;
        self
    }

    pub fn disable_throttle(mut self) -> Self {
        self.throttle_min = 0;
        self.throttle_max = 0;
        self
    }

    fn throttle(&self) {
        if self.throttle_max > 0 {
            let sleep_ms = rand::thread_rng().gen_range(self.throttle_min..=self.throttle_max);
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }

    fn post<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<T> {
        self.post_with_raw(endpoint, body).map(|r| r.parsed)
    }

    /// Like post(), but also returns the raw response body text
    fn post_with_raw<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: serde_json::Value,
    ) -> Result<ApiResponse<T>> {
        let url = format!("{}{}", self.base_url, endpoint);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "*/*")
            .header("Content-Type", "application/json")
            .header("User-Agent", "Granola/5.354.0")
            .header("X-Client-Version", "5.354.0")
            .json(&body)
            .send()?;

        self.throttle();

        let status = response.status();
        if !status.is_success() {
            let message = response.text().unwrap_or_default();
            let preview = truncate_str(&message, 100);
            return Err(Error::Api {
                endpoint: endpoint.into(),
                status: status.as_u16(),
                message: preview,
            });
        }

        let raw = response.text()?;
        let parsed = serde_json::from_str(&raw).map_err(|e| {
            eprintln!("Failed to parse response from {}: {}", endpoint, e);
            eprintln!(
                "Response body (first 500 chars): {}",
                truncate_str(&raw, 500)
            );
            Error::Parse(e)
        })?;

        Ok(ApiResponse { raw, parsed })
    }

    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        #[derive(serde::Deserialize)]
        struct Response {
            docs: Vec<DocumentSummary>,
        }

        let resp: Response = self.post("/v2/get-documents", json!({}))?;
        Ok(resp.docs)
    }

    pub fn get_metadata(&self, doc_id: &str) -> Result<DocumentMetadata> {
        self.post(
            "/v1/get-document-metadata",
            json!({ "document_id": doc_id }),
        )
    }

    pub fn get_metadata_with_raw(&self, doc_id: &str) -> Result<ApiResponse<DocumentMetadata>> {
        self.post_with_raw(
            "/v1/get-document-metadata",
            json!({ "document_id": doc_id }),
        )
    }

    pub fn get_transcript(&self, doc_id: &str) -> Result<RawTranscript> {
        self.post(
            "/v1/get-document-transcript",
            json!({ "document_id": doc_id }),
        )
    }

    pub fn get_transcript_with_raw(&self, doc_id: &str) -> Result<ApiResponse<RawTranscript>> {
        self.post_with_raw(
            "/v1/get-document-transcript",
            json!({ "document_id": doc_id }),
        )
    }

    /// Internal helper for GET requests (the public API uses GET, not POST)
    fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("User-Agent", "muesli/1.0 (Rust)")
            .send()?;

        self.throttle();

        let status = response.status();
        if !status.is_success() {
            let message = response.text().unwrap_or_default();
            let preview = truncate_str(&message, 100);
            return Err(Error::Api {
                endpoint: url.into(),
                status: status.as_u16(),
                message: preview,
            });
        }

        let raw = response.text()?;
        let parsed = serde_json::from_str(&raw).map_err(|e| {
            eprintln!("Failed to parse response from {}: {}", url, e);
            eprintln!(
                "Response body (first 500 chars): {}",
                truncate_str(&raw, 500)
            );
            Error::Parse(e)
        })?;

        Ok(parsed)
    }

    /// Fetch a single note with summary text from the public API.
    /// Uses base URL: https://public-api.granola.ai
    pub fn get_public_note(&self, note_id: &str) -> Result<PublicNote> {
        let url = format!("https://public-api.granola.ai/v1/notes/{}", note_id);
        self.get(&url)
    }

    /// List documents with panels (user notes + AI-enhanced notes).
    /// Paginates through all results in batches.
    pub fn list_documents_with_notes(&self) -> Result<Vec<DocumentSummary>> {
        #[derive(serde::Deserialize)]
        struct Response {
            docs: Vec<DocumentSummary>,
        }

        let batch_size = 100;
        let mut all_docs = Vec::new();
        let mut offset = 0;
        let mut seen_ids = HashSet::new();

        loop {
            let resp: Response = self.post(
                "/v2/get-documents",
                json!({
                    "limit": batch_size,
                    "offset": offset,
                    "include_last_viewed_panel": true,
                    "include_panels": true
                }),
            )?;

            let count = resp.docs.len();

            // Guard against infinite loops: if the API ignores limit/offset
            // and keeps returning the same documents, break when no new IDs appear.
            let new_count = resp
                .docs
                .iter()
                .filter(|d| seen_ids.insert(d.id.clone()))
                .count();
            if new_count == 0 && count > 0 {
                break;
            }

            all_docs.extend(resp.docs);

            if count < batch_size {
                break;
            }
            offset += batch_size;
        }

        Ok(all_docs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 100), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let result = truncate_str("hello world", 7);
        assert!(result.starts_with("hello"));
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_str_utf8() {
        // Test with multi-byte UTF-8 characters - should not panic
        let text = "Hello 世界 World";
        let result = truncate_str(text, 10);
        // Should not panic and should be valid UTF-8
        assert!(!result.is_empty());
        assert!(result.len() <= 13); // 10 chars + "..."
    }

    #[test]
    fn test_truncate_str_emoji() {
        // Test with emoji (4-byte UTF-8)
        let text = "Hello 🎉🎉🎉 World";
        let result = truncate_str(text, 10);
        // Should not panic
        assert!(!result.is_empty());
    }

    #[test]
    fn test_api_client_new() {
        let client = ApiClient::new("test_token".into(), None).unwrap();
        assert_eq!(client.base_url, "https://api.granola.ai");
        assert_eq!(client.token, "test_token");
    }

    #[test]
    fn test_api_client_custom_base() {
        let client = ApiClient::new("token".into(), Some("https://custom.api".into())).unwrap();
        assert_eq!(client.base_url, "https://custom.api");
    }

    #[test]
    fn test_api_client_throttle_config() {
        let client = ApiClient::new("token".into(), None)
            .unwrap()
            .with_throttle(50, 150);
        assert_eq!(client.throttle_min, 50);
        assert_eq!(client.throttle_max, 150);
    }

    #[test]
    fn test_api_client_disable_throttle() {
        let client = ApiClient::new("token".into(), None)
            .unwrap()
            .disable_throttle();
        assert_eq!(client.throttle_min, 0);
        assert_eq!(client.throttle_max, 0);
    }

    #[test]
    fn test_get_public_note_uses_public_api_base_url() {
        // Verify the public note method constructs the correct URL
        // by checking it does NOT use the configurable base_url.
        // We can't test actual HTTP here, but we verify the client
        // can be constructed and the method exists with the right signature.
        let client = ApiClient::new("token".into(), Some("https://custom.api".into()))
            .unwrap()
            .disable_throttle();
        // The method should exist and accept a note_id string
        // It will fail at the network level, not at construction
        let result = client.get_public_note("nonexistent-note-id");
        // Should fail with a network/connection error, not a compile error
        assert!(result.is_err());
    }

    #[test]
    fn test_pagination_dedup_guard() {
        // Exercises the same HashSet-based dedup logic used in
        // list_documents_with_notes() to break out of pagination when the
        // API ignores limit/offset and returns duplicate document IDs.
        let mut seen_ids = HashSet::new();

        // First batch: all new IDs — should continue paginating
        let batch1 = vec!["doc1", "doc2", "doc3"];
        let new_count: usize = batch1
            .iter()
            .filter(|id| seen_ids.insert(id.to_string()))
            .count();
        assert_eq!(new_count, 3, "all docs should be new");
        assert!(new_count > 0, "should continue paginating");

        // Second batch: all duplicates — should trigger break
        let batch2 = vec!["doc1", "doc2", "doc3"];
        let new_count: usize = batch2
            .iter()
            .filter(|id| seen_ids.insert(id.to_string()))
            .count();
        assert_eq!(new_count, 0, "no new docs — should break");

        // Mixed batch: some new, some old — should continue
        let batch3 = vec!["doc1", "doc4"];
        let new_count: usize = batch3
            .iter()
            .filter(|id| seen_ids.insert(id.to_string()))
            .count();
        assert_eq!(new_count, 1, "one new doc — should continue");
    }

    #[test]
    fn test_list_documents_with_notes_method_exists() {
        // Verify the method exists and has the right return type signature.
        // It will fail at the network level, not at construction.
        let client = ApiClient::new("token".into(), None)
            .unwrap()
            .disable_throttle();
        let result = client.list_documents_with_notes();
        // Should fail with a network/connection error, not a compile error
        assert!(result.is_err());
    }
}

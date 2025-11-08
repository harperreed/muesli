// ABOUTME: Blocking HTTP client for Granola API
// ABOUTME: Handles throttling, auth headers, and fail-fast errors

use crate::{DocumentMetadata, DocumentSummary, Error, RawTranscript, Result};
use rand::Rng;
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Duration;

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
        let url = format!("{}{}", self.base_url, endpoint);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "muesli/1.0 (Rust)")
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

        // Get response text for better error messages
        let body = response.text()?;
        serde_json::from_str(&body).map_err(|e| {
            eprintln!("Failed to parse response from {}: {}", endpoint, e);
            eprintln!("Response body (first 500 chars): {}", truncate_str(&body, 500));
            Error::Parse(e)
        })
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

    pub fn get_transcript(&self, doc_id: &str) -> Result<RawTranscript> {
        self.post(
            "/v1/get-document-transcript",
            json!({ "document_id": doc_id }),
        )
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
}

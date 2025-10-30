// ABOUTME: Blocking HTTP client for Granola API
// ABOUTME: Handles throttling, auth headers, and fail-fast errors

use crate::{DocumentMetadata, DocumentSummary, Error, RawTranscript, Result};
use rand::Rng;
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Duration;

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
    throttle_min: u64,
    throttle_max: u64,
}

impl ApiClient {
    pub fn new(token: String, base_url: Option<String>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

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

    fn post<T: serde::de::DeserializeOwned>(&self, endpoint: &str, body: serde_json::Value) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);

        let response = self.client
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
            let preview = if message.len() > 100 {
                format!("{}...", &message[..100])
            } else {
                message
            };
            return Err(Error::Api {
                endpoint: endpoint.into(),
                status: status.as_u16(),
                message: preview,
            });
        }

        Ok(response.json()?)
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
        self.post("/v1/get-document-metadata", json!({ "document_id": doc_id }))
    }

    pub fn get_transcript(&self, doc_id: &str) -> Result<RawTranscript> {
        self.post("/v1/get-document-transcript", json!({ "document_id": doc_id }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

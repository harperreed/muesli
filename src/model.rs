// ABOUTME: Serde data models for Granola API responses
// ABOUTME: Tolerant parsing with optional fields and flexible timestamps

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_summary_deserialize_minimal() {
        let json = r#"{"id": "doc123", "created_at": "2025-10-28T15:04:05Z"}"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert_eq!(doc.id, "doc123");
        assert!(doc.title.is_none());
        assert!(doc.updated_at.is_none());
    }

    #[test]
    fn test_document_summary_deserialize_full() {
        let json = r#"{
            "id": "doc123",
            "title": "Planning Meeting",
            "created_at": "2025-10-28T15:04:05Z",
            "updated_at": "2025-10-29T01:23:45Z",
            "extra_field": "ignored"
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert_eq!(doc.id, "doc123");
        assert_eq!(doc.title.as_deref(), Some("Planning Meeting"));
        assert!(doc.updated_at.is_some());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn test_document_metadata_deserialize() {
        let json = r#"{
            "id": "doc123",
            "title": "Q4 Planning",
            "created_at": "2025-10-28T15:04:05Z",
            "updated_at": "2025-10-29T01:23:45Z",
            "participants": ["Alice", "Bob"],
            "duration_seconds": 3600,
            "labels": ["Planning", "Q4"]
        }"#;
        let meta: DocumentMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.participants.len(), 2);
        assert_eq!(meta.duration_seconds, Some(3600));
        assert_eq!(meta.labels.len(), 2);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTranscript {
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub monologues: Vec<Monologue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub start: Option<TimestampValue>,
    #[serde(default)]
    pub end: Option<TimestampValue>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monologue {
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub start: Option<TimestampValue>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TimestampValue {
    Seconds(f64),
    String(String),
}

#[cfg(test)]
mod transcript_tests {
    use super::*;

    #[test]
    fn test_raw_transcript_segments() {
        let json = r#"{
            "segments": [
                {"speaker": "Alice", "start": 12.34, "end": 18.90, "text": "Hello"}
            ]
        }"#;
        let transcript: RawTranscript = serde_json::from_str(json).unwrap();
        assert_eq!(transcript.segments.len(), 1);
        assert_eq!(transcript.segments[0].text, "Hello");
    }

    #[test]
    fn test_raw_transcript_monologues() {
        let json = r#"{
            "monologues": [
                {"speaker": "Bob", "start": "00:05:10", "blocks": [{"text": "First"}, {"text": "Second"}]}
            ]
        }"#;
        let transcript: RawTranscript = serde_json::from_str(json).unwrap();
        assert_eq!(transcript.monologues.len(), 1);
        assert_eq!(transcript.monologues[0].blocks.len(), 2);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub doc_id: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub remote_updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub generator: String,
}

#[cfg(test)]
mod frontmatter_tests {
    use super::*;

    #[test]
    fn test_frontmatter_roundtrip() {
        let fm = Frontmatter {
            doc_id: "doc123".into(),
            source: "granola".into(),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            remote_updated_at: Some("2025-10-29T01:23:45Z".parse().unwrap()),
            title: Some("Test Meeting".into()),
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3600),
            labels: vec!["Planning".into()],
            generator: "muesli 1.0".into(),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.doc_id, "doc123");
        assert_eq!(parsed.participants.len(), 2);
    }
}

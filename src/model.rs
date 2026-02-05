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

/// Rich attendee information from Granola API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<PersonDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<PersonInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<CompanyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<PersonName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub employment: Option<Employment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linkedin: Option<LinkedIn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonName {
    #[serde(default, rename = "fullName", skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Employment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedIn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    #[serde(default)]
    pub id: Option<String>,
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
    #[serde(default)]
    pub creator: Option<Attendee>,
    #[serde(default)]
    pub attendees: Option<Vec<Attendee>>,
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
        assert!(meta.creator.is_none());
        assert!(meta.attendees.is_none());
    }

    #[test]
    fn test_document_metadata_with_rich_attendees() {
        let json = r#"{
            "id": "doc123",
            "title": "Q4 Planning",
            "created_at": "2025-10-28T15:04:05Z",
            "participants": ["Alice Smith", "Bob Jones"],
            "creator": {
                "name": "Alice Smith",
                "email": "alice@acme.com",
                "details": {
                    "person": {
                        "name": { "fullName": "Alice Smith" },
                        "employment": { "title": "Engineering Manager" },
                        "linkedin": { "handle": "alicesmith" }
                    },
                    "company": { "name": "Acme Corp" }
                }
            },
            "attendees": [
                {
                    "name": "Alice Smith",
                    "email": "alice@acme.com",
                    "details": {
                        "person": {
                            "name": { "fullName": "Alice Smith" },
                            "employment": { "title": "Engineering Manager" },
                            "linkedin": { "handle": "alicesmith" }
                        },
                        "company": { "name": "Acme Corp" }
                    }
                },
                {
                    "name": "Bob Jones",
                    "email": "bob@acme.com"
                }
            ]
        }"#;
        let meta: DocumentMetadata = serde_json::from_str(json).unwrap();

        // Creator
        let creator = meta.creator.as_ref().unwrap();
        assert_eq!(creator.name.as_deref(), Some("Alice Smith"));
        assert_eq!(creator.email.as_deref(), Some("alice@acme.com"));
        let details = creator.details.as_ref().unwrap();
        let person = details.person.as_ref().unwrap();
        assert_eq!(
            person.name.as_ref().unwrap().full_name.as_deref(),
            Some("Alice Smith")
        );
        assert_eq!(
            person.employment.as_ref().unwrap().title.as_deref(),
            Some("Engineering Manager")
        );
        assert_eq!(
            person.linkedin.as_ref().unwrap().handle.as_deref(),
            Some("alicesmith")
        );
        let company = details.company.as_ref().unwrap();
        assert_eq!(company.name.as_deref(), Some("Acme Corp"));

        // Attendees
        let attendees = meta.attendees.as_ref().unwrap();
        assert_eq!(attendees.len(), 2);
        assert_eq!(attendees[1].name.as_deref(), Some("Bob Jones"));
        assert_eq!(attendees[1].email.as_deref(), Some("bob@acme.com"));
        assert!(attendees[1].details.is_none());
    }

    #[test]
    fn test_attendee_minimal() {
        let json = r#"{"name": "Alice"}"#;
        let attendee: Attendee = serde_json::from_str(json).unwrap();
        assert_eq!(attendee.name.as_deref(), Some("Alice"));
        assert!(attendee.email.is_none());
        assert!(attendee.details.is_none());
    }

    #[test]
    fn test_attendee_unknown_fields_tolerated() {
        let json = r#"{
            "name": "Alice",
            "email": "alice@example.com",
            "some_future_field": "should not break",
            "details": {
                "person": {
                    "name": { "fullName": "Alice" },
                    "some_other_field": 42
                }
            }
        }"#;
        let attendee: Attendee = serde_json::from_str(json).unwrap();
        assert_eq!(attendee.name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_metadata_with_unknown_api_fields() {
        let json = r#"{
            "id": "doc123",
            "created_at": "2025-10-28T15:04:05Z",
            "participants": [],
            "some_brand_new_field": {"nested": true},
            "another_future_field": [1, 2, 3]
        }"#;
        let meta: DocumentMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.id.as_deref(), Some("doc123"));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawTranscript {
    pub entries: Vec<TranscriptEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    #[serde(default)]
    pub document_id: Option<String>,
    #[serde(rename = "start_timestamp", default)]
    pub start: Option<String>,
    #[serde(rename = "end_timestamp", default)]
    pub end: Option<String>,
    pub text: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub is_final: Option<bool>,
    #[serde(default)]
    pub speaker: Option<String>,
}

// Legacy types kept for backward compatibility with tests
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
    fn test_raw_transcript_deserialize() {
        let json = r#"[
            {
                "document_id": "doc123",
                "speaker": "Alice",
                "start_timestamp": "2025-10-01T21:35:12.500Z",
                "end_timestamp": "2025-10-01T21:35:18.000Z",
                "text": "Hello",
                "source": "microphone",
                "id": "entry1",
                "is_final": true
            }
        ]"#;
        let transcript: RawTranscript = serde_json::from_str(json).unwrap();
        assert_eq!(transcript.entries.len(), 1);
        assert_eq!(transcript.entries[0].text, "Hello");
        assert_eq!(transcript.entries[0].speaker.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_raw_transcript_minimal() {
        let json = r#"[
            {"text": "Just text"}
        ]"#;
        let transcript: RawTranscript = serde_json::from_str(json).unwrap();
        assert_eq!(transcript.entries.len(), 1);
        assert_eq!(transcript.entries[0].text, "Just text");
        assert!(transcript.entries[0].speaker.is_none());
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator: Option<Attendee>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<Attendee>>,
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
            creator: None,
            attendees: None,
            generator: "muesli 1.0".into(),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.doc_id, "doc123");
        assert_eq!(parsed.participants.len(), 2);
        assert!(parsed.creator.is_none());
        assert!(parsed.attendees.is_none());
        // Verify skip_serializing_if works - no creator/attendees in YAML
        assert!(!yaml.contains("creator"));
        assert!(!yaml.contains("attendees"));
    }

    #[test]
    fn test_frontmatter_with_attendees_roundtrip() {
        let fm = Frontmatter {
            doc_id: "doc123".into(),
            source: "granola".into(),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            remote_updated_at: None,
            title: Some("Test Meeting".into()),
            participants: vec!["Alice".into()],
            duration_seconds: None,
            labels: vec![],
            creator: Some(Attendee {
                name: Some("Alice".into()),
                email: Some("alice@acme.com".into()),
                details: None,
            }),
            attendees: Some(vec![Attendee {
                name: Some("Alice".into()),
                email: Some("alice@acme.com".into()),
                details: None,
            }]),
            generator: "muesli 1.0".into(),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert!(parsed.creator.is_some());
        assert_eq!(
            parsed.creator.as_ref().unwrap().email.as_deref(),
            Some("alice@acme.com")
        );
        assert_eq!(parsed.attendees.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_frontmatter_backward_compat_no_attendee_fields() {
        // Existing YAML without creator/attendees should parse fine
        let yaml = r#"
doc_id: doc123
source: granola
created_at: 2025-10-28T15:04:05Z
title: Old Meeting
participants: [Alice]
generator: muesli 1.0
"#;
        let parsed: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.doc_id, "doc123");
        assert!(parsed.creator.is_none());
        assert!(parsed.attendees.is_none());
    }
}

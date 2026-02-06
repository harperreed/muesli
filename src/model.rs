// ABOUTME: Serde data models for Granola API responses
// ABOUTME: Tolerant parsing with optional fields and flexible timestamps

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A content panel from the Granola API (user notes or AI-enhanced notes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Panel {
    #[serde(rename = "type")]
    pub panel_type: String,
    /// ProseMirror doc — stored as raw Value to tolerate malformed structures
    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// Typed content panels (my_notes, enhanced_notes, etc.)
    #[serde(default)]
    pub panels: Option<Vec<Panel>>,
    /// ProseMirror notes from the `notes` field — stored as raw Value to tolerate
    /// malformed structures (e.g. content as map instead of array)
    #[serde(default)]
    pub notes: Option<serde_json::Value>,
    /// Fallback field for enhanced notes content
    #[serde(default)]
    pub last_viewed_panel: Option<serde_json::Value>,
}

impl DocumentSummary {
    /// Extract a specific panel type's ProseMirror content from the panels array.
    fn panel_content(&self, panel_type: &str) -> Option<ProseMirrorDoc> {
        self.panels
            .as_ref()?
            .iter()
            .find(|p| p.panel_type == panel_type)
            .and_then(|p| p.content.as_ref())
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Extract user notes (my_notes panel → notes field → last_viewed_panel fallback).
    pub fn user_notes(&self) -> Option<ProseMirrorDoc> {
        self.panel_content("my_notes").or_else(|| {
            self.notes
                .as_ref()
                .or(self.last_viewed_panel.as_ref())
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        })
    }

    /// Extract AI-generated enhanced notes from the enhanced_notes panel.
    pub fn enhanced_notes(&self) -> Option<ProseMirrorDoc> {
        self.panel_content("enhanced_notes")
    }
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

/// Response from the official Granola public API GET /v1/notes/{id}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicNote {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary_text: Option<String>,
}

/// ProseMirror document root node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseMirrorDoc {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub content: Option<Vec<ProseMirrorNode>>,
}

/// ProseMirror content node (paragraph, heading, text, list, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseMirrorNode {
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub content: Option<Vec<ProseMirrorNode>>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub attrs: Option<serde_json::Value>,
    #[serde(default)]
    pub marks: Option<Vec<ProseMirrorMark>>,
}

/// ProseMirror inline mark (bold, italic, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseMirrorMark {
    #[serde(rename = "type")]
    pub mark_type: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
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
            summary_text: None,
            generator: "muesli 1.0".into(),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.doc_id, "doc123");
        assert_eq!(parsed.participants.len(), 2);
        assert!(parsed.creator.is_none());
        assert!(parsed.attendees.is_none());
        assert!(parsed.summary_text.is_none());
        // Verify skip_serializing_if works - no creator/attendees/summary_text in YAML
        assert!(!yaml.contains("creator"));
        assert!(!yaml.contains("attendees"));
        assert!(!yaml.contains("summary_text"));
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
            summary_text: None,
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

#[cfg(test)]
mod public_note_tests {
    use super::*;

    #[test]
    fn test_public_note_deserialize_minimal() {
        let json = r#"{"id": "note-abc-123"}"#;
        let note: PublicNote = serde_json::from_str(json).unwrap();
        assert_eq!(note.id, "note-abc-123");
        assert!(note.title.is_none());
        assert!(note.summary_text.is_none());
    }

    #[test]
    fn test_public_note_deserialize_full() {
        let json = r#"{
            "id": "note-abc-123",
            "title": "Sprint Planning",
            "summary_text": "We discussed Q1 priorities and assigned tasks."
        }"#;
        let note: PublicNote = serde_json::from_str(json).unwrap();
        assert_eq!(note.id, "note-abc-123");
        assert_eq!(note.title.as_deref(), Some("Sprint Planning"));
        assert_eq!(
            note.summary_text.as_deref(),
            Some("We discussed Q1 priorities and assigned tasks.")
        );
    }

    #[test]
    fn test_public_note_unknown_fields_tolerated() {
        let json = r#"{
            "id": "note-abc-123",
            "title": "Sprint Planning",
            "summary_text": "Summary here",
            "some_future_field": "should not break",
            "nested_future": {"key": "value"}
        }"#;
        let note: PublicNote = serde_json::from_str(json).unwrap();
        assert_eq!(note.id, "note-abc-123");
        assert_eq!(note.title.as_deref(), Some("Sprint Planning"));
        assert_eq!(note.summary_text.as_deref(), Some("Summary here"));
    }
}

#[cfg(test)]
mod prosemirror_tests {
    use super::*;

    #[test]
    fn test_prosemirror_paragraph() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "Hello world"}
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        assert_eq!(doc.node_type, "doc");
        let content = doc.content.as_ref().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0].node_type, "paragraph");
        let para_content = content[0].content.as_ref().unwrap();
        assert_eq!(para_content[0].text.as_deref(), Some("Hello world"));
    }

    #[test]
    fn test_prosemirror_heading() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {"level": 2},
                    "content": [
                        {"type": "text", "text": "My Heading"}
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        let content = doc.content.as_ref().unwrap();
        assert_eq!(content[0].node_type, "heading");
        let level = content[0].attrs.as_ref().unwrap()["level"]
            .as_u64()
            .unwrap();
        assert_eq!(level, 2);
        let heading_content = content[0].content.as_ref().unwrap();
        assert_eq!(heading_content[0].text.as_deref(), Some("My Heading"));
    }

    #[test]
    fn test_prosemirror_bullet_list() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "bulletList",
                    "content": [
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {"type": "text", "text": "Item one"}
                                    ]
                                }
                            ]
                        },
                        {
                            "type": "listItem",
                            "content": [
                                {
                                    "type": "paragraph",
                                    "content": [
                                        {"type": "text", "text": "Item two"}
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        let content = doc.content.as_ref().unwrap();
        assert_eq!(content[0].node_type, "bulletList");
        let items = content[0].content.as_ref().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].node_type, "listItem");
    }

    #[test]
    fn test_prosemirror_marks_bold_italic() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "bold text", "marks": [{"type": "bold"}]},
                        {"type": "text", "text": " and "},
                        {"type": "text", "text": "italic text", "marks": [{"type": "italic"}]}
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        let content = doc.content.as_ref().unwrap();
        let para_content = content[0].content.as_ref().unwrap();

        // Bold text node
        let bold_node = &para_content[0];
        assert_eq!(bold_node.text.as_deref(), Some("bold text"));
        let marks = bold_node.marks.as_ref().unwrap();
        assert_eq!(marks[0].mark_type, "bold");

        // Plain text node
        let plain_node = &para_content[1];
        assert_eq!(plain_node.text.as_deref(), Some(" and "));
        assert!(plain_node.marks.is_none());

        // Italic text node
        let italic_node = &para_content[2];
        assert_eq!(italic_node.text.as_deref(), Some("italic text"));
        let marks = italic_node.marks.as_ref().unwrap();
        assert_eq!(marks[0].mark_type, "italic");
    }

    #[test]
    fn test_prosemirror_empty_doc() {
        let json = r#"{"type": "doc"}"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        assert_eq!(doc.node_type, "doc");
        assert!(doc.content.is_none());
    }

    #[test]
    fn test_prosemirror_unknown_node_types() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "customWidget",
                    "attrs": {"widgetId": "abc123"},
                    "content": [
                        {"type": "text", "text": "inside widget"}
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        let content = doc.content.as_ref().unwrap();
        assert_eq!(content[0].node_type, "customWidget");
        assert!(content[0].attrs.is_some());
    }

    #[test]
    fn test_document_summary_with_panels() {
        let json = r#"{
            "id": "doc123",
            "created_at": "2025-10-28T15:04:05Z",
            "panels": [
                {
                    "type": "my_notes",
                    "content": {
                        "type": "doc",
                        "content": [{"type": "paragraph", "content": [{"type": "text", "text": "User notes"}]}]
                    }
                },
                {
                    "type": "enhanced_notes",
                    "content": {
                        "type": "doc",
                        "content": [{"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Key Points"}]}]
                    }
                }
            ]
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();

        let user = doc.user_notes().unwrap();
        assert_eq!(user.node_type, "doc");
        let content = user.content.as_ref().unwrap();
        assert_eq!(content[0].node_type, "paragraph");

        let enhanced = doc.enhanced_notes().unwrap();
        assert_eq!(enhanced.node_type, "doc");
        let content = enhanced.content.as_ref().unwrap();
        assert_eq!(content[0].node_type, "heading");
    }

    #[test]
    fn test_document_summary_user_notes_fallback_to_notes_field() {
        // When no panels, user_notes() falls back to the `notes` field
        let json = r#"{
            "id": "doc456",
            "created_at": "2025-10-28T15:04:05Z",
            "notes": {
                "type": "doc",
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": "from notes field"}]}]
            }
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        let pm = doc.user_notes().unwrap();
        assert_eq!(pm.node_type, "doc");
    }

    #[test]
    fn test_document_summary_user_notes_fallback_to_last_viewed_panel() {
        // When no panels or notes field, user_notes() falls back to last_viewed_panel
        let json = r#"{
            "id": "doc789",
            "created_at": "2025-10-28T15:04:05Z",
            "last_viewed_panel": {
                "type": "doc",
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": "from panel"}]}]
            }
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        let pm = doc.user_notes().unwrap();
        assert_eq!(pm.node_type, "doc");
    }

    #[test]
    fn test_document_summary_panels_preferred_over_fallback() {
        // my_notes panel takes precedence over notes/last_viewed_panel fields
        let json = r#"{
            "id": "doc789",
            "created_at": "2025-10-28T15:04:05Z",
            "panels": [
                {
                    "type": "my_notes",
                    "content": {
                        "type": "doc",
                        "content": [{"type": "paragraph", "content": [{"type": "text", "text": "from panel"}]}]
                    }
                }
            ],
            "notes": {
                "type": "doc",
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": "from notes field"}]}]
            }
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        let pm = doc.user_notes().unwrap();
        let content = pm.content.unwrap();
        let para = content[0].content.as_ref().unwrap();
        assert_eq!(para[0].text.as_deref(), Some("from panel"));
    }

    #[test]
    fn test_document_summary_no_enhanced_notes_without_panel() {
        // enhanced_notes only comes from panels, not from fallback fields
        let json = r#"{
            "id": "doc-no-ai",
            "created_at": "2025-10-28T15:04:05Z",
            "notes": {
                "type": "doc",
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": "user notes"}]}]
            }
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert!(doc.enhanced_notes().is_none());
        assert!(doc.user_notes().is_some());
    }

    #[test]
    fn test_document_summary_malformed_panel_content_tolerated() {
        // Panel with content as {} instead of valid ProseMirror
        let json = r#"{
            "id": "doc-malformed",
            "created_at": "2025-10-28T15:04:05Z",
            "panels": [
                {
                    "type": "enhanced_notes",
                    "content": {"not": "valid prosemirror"}
                }
            ]
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert!(doc.enhanced_notes().is_none());
    }

    #[test]
    fn test_document_summary_malformed_notes_field_tolerated() {
        // notes field with content as {} instead of []
        let json = r#"{
            "id": "doc-malformed",
            "created_at": "2025-10-28T15:04:05Z",
            "notes": {
                "type": "doc",
                "content": {"not": "an array"}
            }
        }"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert!(doc.user_notes().is_none());
    }

    #[test]
    fn test_document_summary_without_any_notes() {
        let json = r#"{"id": "doc123", "created_at": "2025-10-28T15:04:05Z"}"#;
        let doc: DocumentSummary = serde_json::from_str(json).unwrap();
        assert!(doc.panels.is_none());
        assert!(doc.notes.is_none());
        assert!(doc.last_viewed_panel.is_none());
        assert!(doc.user_notes().is_none());
        assert!(doc.enhanced_notes().is_none());
    }

    #[test]
    fn test_document_summary_list_with_panels() {
        // Reproduces the actual API response structure with panels
        let json = r#"{"docs":[
            {
                "id": "doc1",
                "created_at": "2024-07-17T14:29:30.559Z",
                "title": "Meeting One",
                "panels": [
                    {"type": "my_notes", "content": {"type": "doc", "content": [{"type": "paragraph"}]}},
                    {"type": "enhanced_notes", "content": {"type": "doc", "content": [{"type": "heading", "attrs": {"level": 2}}]}}
                ]
            },
            {
                "id": "doc2",
                "created_at": "2024-07-18T10:00:00.000Z",
                "title": "Meeting Two"
            }
        ]}"#;

        #[derive(serde::Deserialize)]
        struct Response {
            docs: Vec<DocumentSummary>,
        }

        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.docs.len(), 2);
        assert!(resp.docs[0].user_notes().is_some());
        assert!(resp.docs[0].enhanced_notes().is_some());
        assert!(resp.docs[1].user_notes().is_none());
        assert!(resp.docs[1].enhanced_notes().is_none());
    }
}

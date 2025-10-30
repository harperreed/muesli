// ABOUTME: Converts raw transcript JSON to structured Markdown
// ABOUTME: Supports both segment and monologue formats with frontmatter

use crate::util::normalize_timestamp;
use crate::{DocumentMetadata, Frontmatter, RawTranscript, Result};

pub struct MarkdownOutput {
    pub frontmatter_yaml: String,
    pub body: String,
}

pub fn to_markdown(
    raw: &RawTranscript,
    meta: &DocumentMetadata,
    doc_id: &str,
) -> Result<MarkdownOutput> {
    // Build frontmatter
    let frontmatter = Frontmatter {
        doc_id: doc_id.to_string(),
        source: "granola".into(),
        created_at: meta.created_at,
        remote_updated_at: meta.updated_at,
        title: meta.title.clone(),
        participants: meta.participants.clone(),
        duration_seconds: meta.duration_seconds,
        labels: meta.labels.clone(),
        generator: "muesli 1.0".into(),
    };

    let frontmatter_yaml = serde_yaml::to_string(&frontmatter).map_err(|e| {
        crate::Error::Filesystem(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to serialize frontmatter: {}", e),
        ))
    })?;

    // Build body
    let title = meta.title.as_deref().unwrap_or("Untitled Meeting");
    let mut body = format!("# {}\n\n", title);

    // Metadata line
    let date = meta.created_at.format("%Y-%m-%d");
    let mut meta_parts = vec![format!("Date: {}", date)];

    if let Some(duration) = meta.duration_seconds {
        let minutes = duration / 60;
        meta_parts.push(format!("Duration: {}m", minutes));
    }

    if !meta.participants.is_empty() {
        meta_parts.push(format!("Participants: {}", meta.participants.join(", ")));
    }

    body.push_str(&format!("_{}_\n\n", meta_parts.join(" · ")));

    // Transcript content
    if raw.entries.is_empty() {
        body.push_str("_No transcript content available._\n");
    } else {
        for entry in &raw.entries {
            let speaker = entry.speaker.as_deref().unwrap_or("Speaker");
            let timestamp = entry
                .start
                .as_deref()
                .and_then(normalize_timestamp)
                .map(|ts| format!(" ({})", ts))
                .unwrap_or_default();
            body.push_str(&format!("**{}{}:** {}\n", speaker, timestamp, entry.text));
        }
    }

    Ok(MarkdownOutput {
        frontmatter_yaml,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TranscriptEntry;

    #[test]
    fn test_to_markdown_entries() {
        let raw = RawTranscript {
            entries: vec![
                TranscriptEntry {
                    document_id: Some("doc123".into()),
                    speaker: Some("Alice".into()),
                    start: Some("2025-10-01T21:35:12.500Z".into()),
                    end: Some("2025-10-01T21:35:18.000Z".into()),
                    text: "Hello everyone".into(),
                    source: Some("microphone".into()),
                    id: Some("entry1".into()),
                    is_final: Some(true),
                },
                TranscriptEntry {
                    document_id: Some("doc123".into()),
                    speaker: Some("Bob".into()),
                    start: Some("2025-10-01T21:35:20.000Z".into()),
                    end: Some("2025-10-01T21:35:22.000Z".into()),
                    text: "Hi there".into(),
                    source: Some("microphone".into()),
                    id: Some("entry2".into()),
                    is_final: Some(true),
                },
            ],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Test Meeting".into()),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: None,
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3600),
            labels: vec![],
        };

        let output = to_markdown(&raw, &meta, "doc123").unwrap();

        assert!(output.body.contains("# Test Meeting"));
        assert!(output.body.contains("**Alice"));
        assert!(output.body.contains("Hello everyone"));
        assert!(output.body.contains("**Bob"));
        assert!(output.body.contains("Hi there"));
        assert!(output.body.contains("Duration: 60m"));
        assert!(output.frontmatter_yaml.contains("doc123"));
    }

    #[test]
    fn test_to_markdown_empty_transcript() {
        let raw = RawTranscript { entries: vec![] };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: None,
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
        };

        let output = to_markdown(&raw, &meta, "doc123").unwrap();

        assert!(output.body.contains("# Untitled Meeting"));
        assert!(output.body.contains("_No transcript content available._"));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::model::TranscriptEntry;

    #[test]
    fn test_markdown_output_snapshot() {
        let raw = RawTranscript {
            entries: vec![
                TranscriptEntry {
                    document_id: Some("doc456".into()),
                    speaker: Some("Alice".into()),
                    start: Some("2025-10-28T15:05:10.000Z".into()),
                    end: Some("2025-10-28T15:05:15.000Z".into()),
                    text: "First thought.".into(),
                    source: Some("microphone".into()),
                    id: Some("entry1".into()),
                    is_final: Some(true),
                },
                TranscriptEntry {
                    document_id: Some("doc456".into()),
                    speaker: Some("Alice".into()),
                    start: Some("2025-10-28T15:05:16.000Z".into()),
                    end: Some("2025-10-28T15:05:20.000Z".into()),
                    text: "Second thought.".into(),
                    source: Some("microphone".into()),
                    id: Some("entry2".into()),
                    is_final: Some(true),
                },
            ],
        };

        let meta = DocumentMetadata {
            id: Some("doc456".into()),
            title: Some("Planning Session".into()),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: Some("2025-10-29T01:23:45Z".parse().unwrap()),
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3170),
            labels: vec!["Planning".into()],
        };

        let output = to_markdown(&raw, &meta, "doc456").unwrap();
        let full = format!("---\n{}---\n\n{}", output.frontmatter_yaml, output.body);

        insta::assert_snapshot!(full);
    }
}

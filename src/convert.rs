// ABOUTME: Converts raw transcript JSON to structured Markdown
// ABOUTME: Supports both segment and monologue formats with frontmatter

use crate::{DocumentMetadata, Frontmatter, RawTranscript, Result};
use crate::util::normalize_timestamp;

pub struct MarkdownOutput {
    pub frontmatter_yaml: String,
    pub body: String,
}

pub fn to_markdown(raw: &RawTranscript, meta: &DocumentMetadata) -> Result<MarkdownOutput> {
    // Build frontmatter
    let frontmatter = Frontmatter {
        doc_id: meta.id.clone(),
        source: "granola".into(),
        created_at: meta.created_at,
        remote_updated_at: meta.updated_at,
        title: meta.title.clone(),
        participants: meta.participants.clone(),
        duration_seconds: meta.duration_seconds,
        labels: meta.labels.clone(),
        generator: "muesli 1.0".into(),
    };

    let frontmatter_yaml = serde_yaml::to_string(&frontmatter)
        .map_err(|e| crate::Error::Filesystem(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to serialize frontmatter: {}", e)
        )))?;

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
    if !raw.segments.is_empty() {
        for segment in &raw.segments {
            let speaker = segment.speaker.as_deref().unwrap_or("Speaker");
            let timestamp = segment.start.as_ref()
                .and_then(normalize_timestamp)
                .map(|ts| format!(" ({})", ts))
                .unwrap_or_default();
            body.push_str(&format!("**{}{}:** {}\n", speaker, timestamp, segment.text));
        }
    } else if !raw.monologues.is_empty() {
        for monologue in &raw.monologues {
            let speaker = monologue.speaker.as_deref().unwrap_or("Speaker");
            let timestamp = monologue.start.as_ref()
                .and_then(normalize_timestamp)
                .map(|ts| format!(" ({})", ts))
                .unwrap_or_default();

            for block in &monologue.blocks {
                body.push_str(&format!("**{}{}:** {}\n", speaker, timestamp, block.text));
            }
        }
    } else {
        body.push_str("_No transcript content available._\n");
    }

    Ok(MarkdownOutput {
        frontmatter_yaml,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Segment, TimestampValue};

    #[test]
    fn test_to_markdown_segments() {
        let raw = RawTranscript {
            segments: vec![
                Segment {
                    speaker: Some("Alice".into()),
                    start: Some(TimestampValue::Seconds(12.5)),
                    end: Some(TimestampValue::Seconds(18.0)),
                    text: "Hello everyone".into(),
                },
                Segment {
                    speaker: Some("Bob".into()),
                    start: Some(TimestampValue::String("00:00:20".into())),
                    end: None,
                    text: "Hi there".into(),
                },
            ],
            monologues: vec![],
        };

        let meta = DocumentMetadata {
            id: "doc123".into(),
            title: Some("Test Meeting".into()),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: None,
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3600),
            labels: vec![],
        };

        let output = to_markdown(&raw, &meta).unwrap();

        assert!(output.body.contains("# Test Meeting"));
        assert!(output.body.contains("**Alice (00:00:12):** Hello everyone"));
        assert!(output.body.contains("**Bob (00:00:20):** Hi there"));
        assert!(output.body.contains("Duration: 60m"));
        assert!(output.frontmatter_yaml.contains("doc123"));
    }

    #[test]
    fn test_to_markdown_empty_transcript() {
        let raw = RawTranscript {
            segments: vec![],
            monologues: vec![],
        };

        let meta = DocumentMetadata {
            id: "doc123".into(),
            title: None,
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
        };

        let output = to_markdown(&raw, &meta).unwrap();

        assert!(output.body.contains("# Untitled Meeting"));
        assert!(output.body.contains("_No transcript content available._"));
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::model::{Monologue, Block, TimestampValue};

    #[test]
    fn test_markdown_output_snapshot() {
        let raw = RawTranscript {
            segments: vec![],
            monologues: vec![
                Monologue {
                    speaker: Some("Alice".into()),
                    start: Some(TimestampValue::String("00:05:10".into())),
                    blocks: vec![
                        Block { text: "First thought.".into() },
                        Block { text: "Second thought.".into() },
                    ],
                },
            ],
        };

        let meta = DocumentMetadata {
            id: "doc456".into(),
            title: Some("Planning Session".into()),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: Some("2025-10-29T01:23:45Z".parse().unwrap()),
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3170),
            labels: vec!["Planning".into()],
        };

        let output = to_markdown(&raw, &meta).unwrap();
        let full = format!("---\n{}---\n\n{}", output.frontmatter_yaml, output.body);

        insta::assert_snapshot!(full);
    }
}

// ABOUTME: Converts raw transcript JSON to structured Markdown
// ABOUTME: Supports both segment and monologue formats with frontmatter and ProseMirror conversion

use chrono::Utc;

use crate::model::{Attendee, ProseMirrorDoc, ProseMirrorNode};
use crate::util::normalize_timestamp;
use crate::{DocumentMetadata, Frontmatter, RawTranscript, Result};

/// Formats a single attendee line for the Participants section.
/// Returns None if the attendee has no displayable name.
fn format_attendee_line(attendee: &Attendee) -> Option<String> {
    let name = attendee.name.as_deref()?;
    let mut parts = vec![format!("**{}**", name)];

    if let Some(ref details) = attendee.details {
        if let Some(ref person) = details.person {
            if let Some(ref emp) = person.employment {
                if let Some(ref title) = emp.title {
                    parts.push(title.clone());
                }
            }
        }
        if let Some(ref company) = details.company {
            if let Some(ref company_name) = company.name {
                parts.push(company_name.clone());
            }
        }
    }

    if let Some(ref email) = attendee.email {
        parts.push(format!("({})", email));
    }

    Some(parts.join(", "))
}

pub struct MarkdownOutput {
    pub frontmatter_yaml: String,
    pub body: String,
}

pub fn to_markdown(
    raw: &RawTranscript,
    meta: &DocumentMetadata,
    doc_id: &str,
    notes: Option<&str>,
    summary_text: Option<&str>,
) -> Result<MarkdownOutput> {
    // Build frontmatter
    let frontmatter = Frontmatter {
        doc_id: doc_id.to_string(),
        source: "granola".into(),
        created_at: meta.created_at.unwrap_or_else(Utc::now),
        remote_updated_at: meta.updated_at,
        title: meta.title.clone(),
        participants: if meta.participants.is_empty() {
            // Derive participants from attendees display names
            meta.attendees
                .as_ref()
                .map(|atts| {
                    atts.iter()
                        .filter(|a| a.is_person())
                        .filter_map(|a| a.display_name())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            meta.participants.clone()
        },
        duration_seconds: meta.duration_seconds,
        labels: meta.labels.clone(),
        creator: meta
            .creator
            .as_ref()
            .and_then(|c| c.display_name().map(|n| format!("[[{}]]", n))),
        attendees: meta.attendees.as_ref().map(|atts| {
            atts.iter()
                .filter(|a| a.is_person())
                .filter_map(|a| a.display_name().map(|n| format!("[[{}]]", n)))
                .collect()
        }),
        summary_text: summary_text.map(|s| s.to_string()),
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
    let date = meta.created_at.unwrap_or_else(Utc::now).format("%Y-%m-%d");
    let mut meta_parts = vec![format!("Date: {}", date)];

    if let Some(duration) = meta.duration_seconds {
        let minutes = duration / 60;
        meta_parts.push(format!("Duration: {}m", minutes));
    }

    if !meta.participants.is_empty() {
        meta_parts.push(format!("Participants: {}", meta.participants.join(", ")));
    }

    body.push_str(&format!("_{}_\n\n", meta_parts.join(" · ")));

    // Rich participants section when attendee data is available
    if let Some(ref attendees) = meta.attendees {
        let rich_lines: Vec<String> = attendees.iter().filter_map(format_attendee_line).collect();
        if !rich_lines.is_empty() {
            body.push_str("## Participants\n\n");
            for line in &rich_lines {
                body.push_str(&format!("- {}\n", line));
            }
            body.push('\n');
        }
    }

    // AI-generated summary section
    if let Some(summary) = summary_text {
        if !summary.is_empty() {
            body.push_str("## Summary\n\n");
            body.push_str(summary);
            body.push_str("\n\n");
        }
    }

    // User ProseMirror notes section (already converted to markdown)
    if let Some(notes_md) = notes {
        if !notes_md.is_empty() {
            body.push_str("## Notes\n\n");
            body.push_str(notes_md);
            body.push_str("\n\n");
        }
    }

    // Separator before transcript
    body.push_str("---\n\n");

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
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3600),
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();

        assert!(output.body.contains("# Test Meeting"));
        assert!(output.body.contains("**Alice"));
        assert!(output.body.contains("Hello everyone"));
        assert!(output.body.contains("**Bob"));
        assert!(output.body.contains("Hi there"));
        assert!(output.body.contains("Duration: 60m"));
        assert!(output.frontmatter_yaml.contains("doc123"));
    }

    #[test]
    fn test_to_markdown_with_rich_attendees() {
        use crate::model::{CompanyInfo, Employment, PersonDetails, PersonInfo, PersonName};

        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: Some("doc123".into()),
                speaker: Some("Alice".into()),
                start: Some("2025-10-01T21:35:12.500Z".into()),
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Team Standup".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec!["Alice Smith".into(), "Bob Jones".into()],
            duration_seconds: Some(900),
            labels: vec![],
            creator: Some(crate::model::Attendee {
                name: Some("Alice Smith".into()),
                email: Some("alice@acme.com".into()),
                details: Some(PersonDetails {
                    person: Some(PersonInfo {
                        name: Some(PersonName {
                            full_name: Some("Alice Smith".into()),
                        }),
                        employment: Some(Employment {
                            title: Some("Engineering Manager".into()),
                        }),
                        linkedin: None,
                    }),
                    company: Some(CompanyInfo {
                        name: Some("Acme Corp".into()),
                    }),
                }),
            }),
            attendees: Some(vec![
                crate::model::Attendee {
                    name: Some("Alice Smith".into()),
                    email: Some("alice@acme.com".into()),
                    details: Some(PersonDetails {
                        person: Some(PersonInfo {
                            name: Some(PersonName {
                                full_name: Some("Alice Smith".into()),
                            }),
                            employment: Some(Employment {
                                title: Some("Engineering Manager".into()),
                            }),
                            linkedin: None,
                        }),
                        company: Some(CompanyInfo {
                            name: Some("Acme Corp".into()),
                        }),
                    }),
                },
                crate::model::Attendee {
                    name: Some("Bob Jones".into()),
                    email: Some("bob@acme.com".into()),
                    details: None,
                },
            ]),
        };

        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();

        assert!(output.body.contains("## Participants"));
        assert!(output
            .body
            .contains("**Alice Smith**, Engineering Manager, Acme Corp, (alice@acme.com)"));
        assert!(output.body.contains("**Bob Jones**, (bob@acme.com)"));

        // Frontmatter should have creator/attendees as wiki-links
        assert!(output.frontmatter_yaml.contains("[[Alice Smith]]"));
        assert!(output.frontmatter_yaml.contains("[[Bob Jones]]"));
    }

    #[test]
    fn test_to_markdown_no_participants_section_without_attendees() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: None,
                speaker: Some("Alice".into()),
                start: None,
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec!["Alice".into()],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();
        assert!(!output.body.contains("## Participants"));
    }

    #[test]
    fn test_to_markdown_attendees_with_no_names_suppresses_section() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: None,
                speaker: Some("Unknown".into()),
                start: None,
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: Some(vec![crate::model::Attendee {
                name: None,
                email: Some("anon@example.com".into()),
                details: None,
            }]),
        };

        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();
        // Attendees exist but none have names, so section should be suppressed
        assert!(!output.body.contains("## Participants"));
    }

    #[test]
    fn test_to_markdown_empty_transcript() {
        let raw = RawTranscript { entries: vec![] };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: None,
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();

        assert!(output.body.contains("# Untitled Meeting"));
        assert!(output.body.contains("_No transcript content available._"));
    }

    #[test]
    fn test_to_markdown_with_summary_and_notes() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: Some("doc123".into()),
                speaker: Some("Alice".into()),
                start: Some("2025-10-01T21:35:12.500Z".into()),
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec!["Alice".into()],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(
            &raw,
            &meta,
            "doc123",
            Some("- Action item 1\n- Action item 2"),
            Some("We discussed project priorities."),
        )
        .unwrap();

        // Summary section should appear before Notes
        assert!(output
            .body
            .contains("## Summary\n\nWe discussed project priorities."));
        assert!(output
            .body
            .contains("## Notes\n\n- Action item 1\n- Action item 2"));
        // Separator should appear before transcript
        assert!(output.body.contains("---\n"));
        // Summary should come before Notes
        let summary_pos = output.body.find("## Summary").unwrap();
        let notes_pos = output.body.find("## Notes").unwrap();
        let separator_pos = output.body.find("---\n").unwrap();
        assert!(summary_pos < notes_pos);
        assert!(notes_pos < separator_pos);
        // Frontmatter should contain summary_text
        assert!(output.frontmatter_yaml.contains("summary_text"));
    }

    #[test]
    fn test_to_markdown_summary_only_no_notes() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: None,
                speaker: Some("Alice".into()),
                start: None,
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123", None, Some("AI summary here.")).unwrap();

        assert!(output.body.contains("## Summary\n\nAI summary here."));
        assert!(!output.body.contains("## Notes"));
        assert!(output.body.contains("---\n"));
    }

    #[test]
    fn test_to_markdown_notes_only_no_summary() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: None,
                speaker: Some("Alice".into()),
                start: None,
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123", Some("User notes here"), None).unwrap();

        assert!(!output.body.contains("## Summary"));
        assert!(output.body.contains("## Notes\n\nUser notes here"));
        assert!(output.body.contains("---\n"));
    }

    #[test]
    fn test_to_markdown_separator_always_present() {
        let raw = RawTranscript {
            entries: vec![TranscriptEntry {
                document_id: None,
                speaker: Some("Alice".into()),
                start: None,
                end: None,
                text: "Hello".into(),
                source: None,
                id: None,
                is_final: None,
            }],
        };

        let meta = DocumentMetadata {
            id: Some("doc123".into()),
            title: Some("Meeting".into()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        // Even with no notes or summary, separator should be present
        let output = to_markdown(&raw, &meta, "doc123", None, None).unwrap();
        assert!(output.body.contains("---\n"));
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
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: Some("2025-10-29T01:23:45Z".parse().unwrap()),
            participants: vec!["Alice".into(), "Bob".into()],
            duration_seconds: Some(3170),
            labels: vec!["Planning".into()],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc456", None, None).unwrap();
        let full = format!("---\n{}---\n\n{}", output.frontmatter_yaml, output.body);

        insta::assert_snapshot!(full);
    }

    #[test]
    fn test_markdown_with_rich_attendees_snapshot() {
        use crate::model::{
            Attendee, CompanyInfo, Employment, PersonDetails, PersonInfo, PersonName,
        };

        let raw = RawTranscript {
            entries: vec![
                TranscriptEntry {
                    document_id: Some("doc789".into()),
                    speaker: Some("Alice Smith".into()),
                    start: Some("2025-11-01T10:00:05.000Z".into()),
                    end: Some("2025-11-01T10:00:10.000Z".into()),
                    text: "Welcome to the planning session.".into(),
                    source: Some("microphone".into()),
                    id: Some("entry1".into()),
                    is_final: Some(true),
                },
                TranscriptEntry {
                    document_id: Some("doc789".into()),
                    speaker: Some("Bob Jones".into()),
                    start: Some("2025-11-01T10:00:12.000Z".into()),
                    end: Some("2025-11-01T10:00:18.000Z".into()),
                    text: "Thanks, let's get started.".into(),
                    source: Some("microphone".into()),
                    id: Some("entry2".into()),
                    is_final: Some(true),
                },
            ],
        };

        let meta = DocumentMetadata {
            id: Some("doc789".into()),
            title: Some("Q1 Planning".into()),
            created_at: Some("2025-11-01T10:00:00Z".parse().unwrap()),
            updated_at: Some("2025-11-02T08:00:00Z".parse().unwrap()),
            participants: vec!["Alice Smith".into(), "Bob Jones".into()],
            duration_seconds: Some(1800),
            labels: vec!["Planning".into()],
            creator: Some(Attendee {
                name: Some("Alice Smith".into()),
                email: Some("alice@acme.com".into()),
                details: Some(PersonDetails {
                    person: Some(PersonInfo {
                        name: Some(PersonName {
                            full_name: Some("Alice Smith".into()),
                        }),
                        employment: Some(Employment {
                            title: Some("Engineering Manager".into()),
                        }),
                        linkedin: None,
                    }),
                    company: Some(CompanyInfo {
                        name: Some("Acme Corp".into()),
                    }),
                }),
            }),
            attendees: Some(vec![
                Attendee {
                    name: Some("Alice Smith".into()),
                    email: Some("alice@acme.com".into()),
                    details: Some(PersonDetails {
                        person: Some(PersonInfo {
                            name: Some(PersonName {
                                full_name: Some("Alice Smith".into()),
                            }),
                            employment: Some(Employment {
                                title: Some("Engineering Manager".into()),
                            }),
                            linkedin: None,
                        }),
                        company: Some(CompanyInfo {
                            name: Some("Acme Corp".into()),
                        }),
                    }),
                },
                Attendee {
                    name: Some("Bob Jones".into()),
                    email: Some("bob@acme.com".into()),
                    details: None,
                },
            ]),
        };

        let output = to_markdown(&raw, &meta, "doc789", None, None).unwrap();
        let full = format!("---\n{}---\n\n{}", output.frontmatter_yaml, output.body);

        insta::assert_snapshot!(full);
    }
}

/// Converts a ProseMirror document to Markdown text.
///
/// Handles doc, heading, paragraph, bulletList, listItem, and text nodes.
/// Applies bold and italic marks. Unknown node types are skipped gracefully.
pub fn prosemirror_to_markdown(doc: &ProseMirrorDoc) -> String {
    let mut output = String::new();
    if let Some(ref content) = doc.content {
        for node in content {
            convert_node(node, &mut output);
        }
    }
    // Trim trailing whitespace but preserve the final structure
    output.trim_end().to_string()
}

/// Recursively converts a ProseMirror node to Markdown, appending to output.
fn convert_node(node: &ProseMirrorNode, output: &mut String) {
    match node.node_type.as_str() {
        "heading" => {
            let level = node
                .attrs
                .as_ref()
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let prefix = "#".repeat(level);
            output.push_str(&prefix);
            output.push(' ');
            if let Some(ref content) = node.content {
                for child in content {
                    render_inline(child, output);
                }
            }
            output.push_str("\n\n");
        }
        "paragraph" => {
            if let Some(ref content) = node.content {
                for child in content {
                    render_inline(child, output);
                }
            }
            output.push_str("\n\n");
        }
        "bulletList" => {
            if let Some(ref content) = node.content {
                for child in content {
                    convert_node(child, output);
                }
            }
        }
        "listItem" => {
            output.push_str("- ");
            if let Some(ref content) = node.content {
                for (i, child) in content.iter().enumerate() {
                    if child.node_type == "paragraph" {
                        // Inline the paragraph content for list items
                        if let Some(ref para_content) = child.content {
                            for inline_child in para_content {
                                render_inline(inline_child, output);
                            }
                        }
                        if i < content.len() - 1 {
                            output.push('\n');
                        }
                    } else {
                        convert_node(child, output);
                    }
                }
            }
            output.push('\n');
        }
        "text" => {
            render_inline(node, output);
        }
        _ => {
            // Unknown node types: skip gracefully
        }
    }
}

/// Renders an inline node (text with optional marks) to the output string.
fn render_inline(node: &ProseMirrorNode, output: &mut String) {
    if node.node_type == "text" {
        let text = node.text.as_deref().unwrap_or("");
        if let Some(ref marks) = node.marks {
            let has_bold = marks.iter().any(|m| m.mark_type == "bold");
            let has_italic = marks.iter().any(|m| m.mark_type == "italic");
            if has_bold && has_italic {
                output.push_str("***");
                output.push_str(text);
                output.push_str("***");
            } else if has_bold {
                output.push_str("**");
                output.push_str(text);
                output.push_str("**");
            } else if has_italic {
                output.push('*');
                output.push_str(text);
                output.push('*');
            } else {
                output.push_str(text);
            }
        } else {
            output.push_str(text);
        }
    }
}

#[cfg(test)]
mod prosemirror_convert_tests {
    use super::*;
    use crate::model::{ProseMirrorDoc, ProseMirrorMark, ProseMirrorNode};

    #[test]
    fn test_paragraph_to_text() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![ProseMirrorNode {
                node_type: "paragraph".into(),
                content: Some(vec![ProseMirrorNode {
                    node_type: "text".into(),
                    content: None,
                    text: Some("Hello world".into()),
                    attrs: None,
                    marks: None,
                }]),
                text: None,
                attrs: None,
                marks: None,
            }]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "Hello world");
    }

    #[test]
    fn test_heading_levels() {
        for level in 1..=3 {
            let doc = ProseMirrorDoc {
                node_type: "doc".into(),
                content: Some(vec![ProseMirrorNode {
                    node_type: "heading".into(),
                    content: Some(vec![ProseMirrorNode {
                        node_type: "text".into(),
                        content: None,
                        text: Some("Title".into()),
                        attrs: None,
                        marks: None,
                    }]),
                    text: None,
                    attrs: Some(serde_json::json!({"level": level})),
                    marks: None,
                }]),
            };
            let md = prosemirror_to_markdown(&doc);
            let prefix = "#".repeat(level as usize);
            assert_eq!(md, format!("{} Title", prefix));
        }
    }

    #[test]
    fn test_bullet_list() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![ProseMirrorNode {
                node_type: "bulletList".into(),
                content: Some(vec![
                    ProseMirrorNode {
                        node_type: "listItem".into(),
                        content: Some(vec![ProseMirrorNode {
                            node_type: "paragraph".into(),
                            content: Some(vec![ProseMirrorNode {
                                node_type: "text".into(),
                                content: None,
                                text: Some("First item".into()),
                                attrs: None,
                                marks: None,
                            }]),
                            text: None,
                            attrs: None,
                            marks: None,
                        }]),
                        text: None,
                        attrs: None,
                        marks: None,
                    },
                    ProseMirrorNode {
                        node_type: "listItem".into(),
                        content: Some(vec![ProseMirrorNode {
                            node_type: "paragraph".into(),
                            content: Some(vec![ProseMirrorNode {
                                node_type: "text".into(),
                                content: None,
                                text: Some("Second item".into()),
                                attrs: None,
                                marks: None,
                            }]),
                            text: None,
                            attrs: None,
                            marks: None,
                        }]),
                        text: None,
                        attrs: None,
                        marks: None,
                    },
                ]),
                text: None,
                attrs: None,
                marks: None,
            }]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "- First item\n- Second item");
    }

    #[test]
    fn test_bold_text() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![ProseMirrorNode {
                node_type: "paragraph".into(),
                content: Some(vec![ProseMirrorNode {
                    node_type: "text".into(),
                    content: None,
                    text: Some("important".into()),
                    attrs: None,
                    marks: Some(vec![ProseMirrorMark {
                        mark_type: "bold".into(),
                    }]),
                }]),
                text: None,
                attrs: None,
                marks: None,
            }]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "**important**");
    }

    #[test]
    fn test_italic_text() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![ProseMirrorNode {
                node_type: "paragraph".into(),
                content: Some(vec![ProseMirrorNode {
                    node_type: "text".into(),
                    content: None,
                    text: Some("emphasis".into()),
                    attrs: None,
                    marks: Some(vec![ProseMirrorMark {
                        mark_type: "italic".into(),
                    }]),
                }]),
                text: None,
                attrs: None,
                marks: None,
            }]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "*emphasis*");
    }

    #[test]
    fn test_bold_italic_text() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![ProseMirrorNode {
                node_type: "paragraph".into(),
                content: Some(vec![ProseMirrorNode {
                    node_type: "text".into(),
                    content: None,
                    text: Some("both".into()),
                    attrs: None,
                    marks: Some(vec![
                        ProseMirrorMark {
                            mark_type: "bold".into(),
                        },
                        ProseMirrorMark {
                            mark_type: "italic".into(),
                        },
                    ]),
                }]),
                text: None,
                attrs: None,
                marks: None,
            }]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "***both***");
    }

    #[test]
    fn test_mixed_content() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![
                ProseMirrorNode {
                    node_type: "heading".into(),
                    content: Some(vec![ProseMirrorNode {
                        node_type: "text".into(),
                        content: None,
                        text: Some("Meeting Notes".into()),
                        attrs: None,
                        marks: None,
                    }]),
                    text: None,
                    attrs: Some(serde_json::json!({"level": 1})),
                    marks: None,
                },
                ProseMirrorNode {
                    node_type: "paragraph".into(),
                    content: Some(vec![
                        ProseMirrorNode {
                            node_type: "text".into(),
                            content: None,
                            text: Some("Some ".into()),
                            attrs: None,
                            marks: None,
                        },
                        ProseMirrorNode {
                            node_type: "text".into(),
                            content: None,
                            text: Some("bold".into()),
                            attrs: None,
                            marks: Some(vec![ProseMirrorMark {
                                mark_type: "bold".into(),
                            }]),
                        },
                        ProseMirrorNode {
                            node_type: "text".into(),
                            content: None,
                            text: Some(" text".into()),
                            attrs: None,
                            marks: None,
                        },
                    ]),
                    text: None,
                    attrs: None,
                    marks: None,
                },
            ]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "# Meeting Notes\n\nSome **bold** text");
    }

    #[test]
    fn test_empty_doc() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: None,
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "");
    }

    #[test]
    fn test_empty_doc_with_empty_content() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "");
    }

    #[test]
    fn test_unknown_node_types_skipped() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![
                ProseMirrorNode {
                    node_type: "customWidget".into(),
                    content: None,
                    text: None,
                    attrs: None,
                    marks: None,
                },
                ProseMirrorNode {
                    node_type: "paragraph".into(),
                    content: Some(vec![ProseMirrorNode {
                        node_type: "text".into(),
                        content: None,
                        text: Some("visible".into()),
                        attrs: None,
                        marks: None,
                    }]),
                    text: None,
                    attrs: None,
                    marks: None,
                },
            ]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "visible");
    }

    #[test]
    fn test_nested_structure_heading_then_list() {
        let doc = ProseMirrorDoc {
            node_type: "doc".into(),
            content: Some(vec![
                ProseMirrorNode {
                    node_type: "heading".into(),
                    content: Some(vec![ProseMirrorNode {
                        node_type: "text".into(),
                        content: None,
                        text: Some("Action Items".into()),
                        attrs: None,
                        marks: None,
                    }]),
                    text: None,
                    attrs: Some(serde_json::json!({"level": 2})),
                    marks: None,
                },
                ProseMirrorNode {
                    node_type: "bulletList".into(),
                    content: Some(vec![
                        ProseMirrorNode {
                            node_type: "listItem".into(),
                            content: Some(vec![ProseMirrorNode {
                                node_type: "paragraph".into(),
                                content: Some(vec![ProseMirrorNode {
                                    node_type: "text".into(),
                                    content: None,
                                    text: Some("Do the thing".into()),
                                    attrs: None,
                                    marks: None,
                                }]),
                                text: None,
                                attrs: None,
                                marks: None,
                            }]),
                            text: None,
                            attrs: None,
                            marks: None,
                        },
                        ProseMirrorNode {
                            node_type: "listItem".into(),
                            content: Some(vec![ProseMirrorNode {
                                node_type: "paragraph".into(),
                                content: Some(vec![ProseMirrorNode {
                                    node_type: "text".into(),
                                    content: None,
                                    text: Some("Follow up".into()),
                                    attrs: None,
                                    marks: None,
                                }]),
                                text: None,
                                attrs: None,
                                marks: None,
                            }]),
                            text: None,
                            attrs: None,
                            marks: None,
                        },
                    ]),
                    text: None,
                    attrs: None,
                    marks: None,
                },
            ]),
        };
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "## Action Items\n\n- Do the thing\n- Follow up");
    }

    #[test]
    fn test_prosemirror_from_json_roundtrip() {
        let json = r#"{
            "type": "doc",
            "content": [
                {
                    "type": "heading",
                    "attrs": {"level": 1},
                    "content": [{"type": "text", "text": "Notes"}]
                },
                {
                    "type": "paragraph",
                    "content": [
                        {"type": "text", "text": "Plain "},
                        {"type": "text", "text": "bold", "marks": [{"type": "bold"}]},
                        {"type": "text", "text": " end"}
                    ]
                }
            ]
        }"#;
        let doc: ProseMirrorDoc = serde_json::from_str(json).unwrap();
        let md = prosemirror_to_markdown(&doc);
        assert_eq!(md, "# Notes\n\nPlain **bold** end");
    }
}

// ABOUTME: Converts raw transcript JSON to structured Markdown
// ABOUTME: Supports both segment and monologue formats with frontmatter

use crate::model::Attendee;
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
        creator: meta.creator.clone(),
        attendees: meta.attendees.clone(),
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
            creator: None,
            attendees: None,
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
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
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

        let output = to_markdown(&raw, &meta, "doc123").unwrap();

        assert!(output.body.contains("## Participants"));
        assert!(output
            .body
            .contains("**Alice Smith**, Engineering Manager, Acme Corp, (alice@acme.com)"));
        assert!(output.body.contains("**Bob Jones**, (bob@acme.com)"));

        // Frontmatter should have creator/attendees
        assert!(output.frontmatter_yaml.contains("alice@acme.com"));
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
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
            updated_at: None,
            participants: vec!["Alice".into()],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc123").unwrap();
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
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
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

        let output = to_markdown(&raw, &meta, "doc123").unwrap();
        // Attendees exist but none have names, so section should be suppressed
        assert!(!output.body.contains("## Participants"));
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
            creator: None,
            attendees: None,
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
            creator: None,
            attendees: None,
        };

        let output = to_markdown(&raw, &meta, "doc456").unwrap();
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
            created_at: "2025-11-01T10:00:00Z".parse().unwrap(),
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

        let output = to_markdown(&raw, &meta, "doc789").unwrap();
        let full = format!("---\n{}---\n\n{}", output.frontmatter_yaml, output.body);

        insta::assert_snapshot!(full);
    }
}

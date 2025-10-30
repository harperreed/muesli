# Muesli Complete Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust CLI tool for syncing Granola meeting transcripts to local storage with Markdown conversion, text search, embeddings, and AI summaries.

**Architecture:** Sequential milestone development (M1: core sync → M2: text search → M3: embeddings → M4: summaries → M5: release automation). Pure TDD throughout with blocking HTTP, XDG storage, and Cargo feature flags for optional capabilities.

**Tech Stack:** Rust 2024 edition, clap, reqwest (blocking), serde, chrono, tantivy, ort (ONNX), keyring, indicatif

---

## Milestone 1: Core Sync

### Task 1: Project Dependencies & Error Types

**Files:**
- Modify: `Cargo.toml`
- Create: `src/error.rs`
- Create: `src/lib.rs`
- Modify: `src/main.rs`

**Step 1: Update Cargo.toml with core dependencies**

```toml
[package]
name = "muesli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "muesli"
path = "src/main.rs"

[lib]
name = "muesli"
path = "src/lib.rs"

[dependencies]
thiserror = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
reqwest = { version = "0.12", features = ["blocking", "json"] }
clap = { version = "4.5", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
directories = "5.0"
slug = "0.1"
indicatif = "0.17"
rand = "0.8"

[dev-dependencies]
wiremock = "0.6"
insta = "1.34"
assert_fs = "1.1"
tempfile = "3.8"

[features]
default = []
index = ["tantivy"]
embeddings-local = ["index", "ort", "tokenizers", "rayon", "hnsw_rs"]
summaries = ["keyring"]
full = ["index", "embeddings-local", "summaries"]

# Feature-gated dependencies (add later in M2-M4)
# tantivy = { version = "0.22", optional = true }
# ort = { version = "2.0", optional = true }
# tokenizers = { version = "0.15", optional = true }
# rayon = { version = "1.8", optional = true }
# hnsw_rs = { version = "0.3", optional = true }
# keyring = { version = "2.3", optional = true }
```

**Step 2: Write test for error type exit codes**

Create `src/error.rs`:

```rust
// ABOUTME: Error types with structured exit codes for CLI
// ABOUTME: Maps domain errors to specific exit codes for shell scripting

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("API error {status} on {endpoint}: {message}")]
    Api {
        endpoint: String,
        status: u16,
        message: String,
    },

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("Summarization error: {0}")]
    Summarization(String),

    #[error("Indexing error: {0}")]
    Indexing(String),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Auth(_) => 2,
            Error::Network(_) => 3,
            Error::Api { .. } => 4,
            Error::Parse(_) => 5,
            Error::Filesystem(_) => 6,
            Error::Summarization(_) => 7,
            Error::Indexing(_) => 8,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_exit_codes() {
        assert_eq!(Error::Auth("test".into()).exit_code(), 2);
        assert_eq!(
            Error::Api {
                endpoint: "test".into(),
                status: 404,
                message: "not found".into()
            }
            .exit_code(),
            4
        );
        assert_eq!(Error::Summarization("test".into()).exit_code(), 7);
    }
}
```

**Step 3: Run test to verify it passes**

Run: `cargo test test_error_exit_codes`
Expected: PASS

**Step 4: Create lib.rs with module exports**

Create `src/lib.rs`:

```rust
// ABOUTME: Public library API for Muesli transcript sync
// ABOUTME: Re-exports core modules for external use

pub mod error;

pub use error::{Error, Result};
```

**Step 5: Update main.rs to use error types**

Modify `src/main.rs`:

```rust
// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use muesli::Result;

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    println!("Hello, world!");
    Ok(())
}
```

**Step 6: Verify build**

Run: `cargo build`
Expected: Success

**Step 7: Commit**

```bash
git add Cargo.toml src/error.rs src/lib.rs src/main.rs
git commit -m "feat: add error types with exit codes and project dependencies"
```

---

### Task 2: Data Models

**Files:**
- Create: `src/model.rs`
- Modify: `src/lib.rs`

**Step 1: Write test for DocumentSummary deserialization**

Create `src/model.rs`:

```rust
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
```

**Step 2: Run test to verify it passes**

Run: `cargo test test_document_summary`
Expected: PASS (both tests)

**Step 3: Add DocumentMetadata model with test**

Append to `src/model.rs`:

```rust
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
```

**Step 4: Run test**

Run: `cargo test test_document_metadata`
Expected: PASS

**Step 5: Add RawTranscript models (both formats)**

Append to `src/model.rs`:

```rust
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
```

**Step 6: Run tests**

Run: `cargo test transcript_tests`
Expected: PASS

**Step 7: Add Frontmatter model**

Append to `src/model.rs`:

```rust
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
```

**Step 8: Run test**

Run: `cargo test test_frontmatter_roundtrip`
Expected: PASS

**Step 9: Export models from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod error;
pub mod model;

pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
```

**Step 10: Commit**

```bash
git add src/model.rs src/lib.rs
git commit -m "feat: add data models with tolerant deserialization"
```

---

### Task 3: Utilities (Slug & Timestamp)

**Files:**
- Create: `src/util.rs`
- Modify: `src/lib.rs`

**Step 1: Write test for slug generation**

Create `src/util.rs`:

```rust
// ABOUTME: Utility functions for slugging, timestamps, and helpers
// ABOUTME: Provides consistent filename generation and time formatting

use crate::model::TimestampValue;

pub fn slugify(text: &str) -> String {
    slug::slugify(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Q4 Planning!!!"), "q4-planning");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("Föö Bär"), "foo-bar");
        assert_eq!(slugify("Test@#$%123"), "test-123");
    }
}
```

**Step 2: Run test**

Run: `cargo test test_slugify`
Expected: PASS

**Step 3: Write test for timestamp normalization**

Append to `src/util.rs`:

```rust
pub fn normalize_timestamp(ts: &TimestampValue) -> Option<String> {
    match ts {
        TimestampValue::Seconds(secs) => {
            let total_secs = *secs as u64;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            Some(format!("{:02}:{:02}:{:02}", hours, minutes, seconds))
        }
        TimestampValue::String(s) => {
            // Try to parse and normalize HH:MM:SS.sss -> HH:MM:SS
            if let Some(pos) = s.find('.') {
                Some(s[..pos].to_string())
            } else {
                Some(s.clone())
            }
        }
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;
    use crate::model::TimestampValue;

    #[test]
    fn test_normalize_timestamp_seconds() {
        let ts = TimestampValue::Seconds(3665.5);
        assert_eq!(normalize_timestamp(&ts), Some("01:01:05".into()));
    }

    #[test]
    fn test_normalize_timestamp_string() {
        let ts = TimestampValue::String("00:12:34.567".into());
        assert_eq!(normalize_timestamp(&ts), Some("00:12:34".into()));
    }

    #[test]
    fn test_normalize_timestamp_string_no_subseconds() {
        let ts = TimestampValue::String("00:05:10".into());
        assert_eq!(normalize_timestamp(&ts), Some("00:05:10".into()));
    }
}
```

**Step 4: Run test**

Run: `cargo test timestamp_tests`
Expected: PASS

**Step 5: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod error;
pub mod model;
pub mod util;

pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
```

**Step 6: Commit**

```bash
git add src/util.rs src/lib.rs
git commit -m "feat: add slug and timestamp normalization utilities"
```

---

### Task 4: Storage Layer

**Files:**
- Create: `src/storage.rs`
- Modify: `src/lib.rs`

**Step 1: Write test for XDG path resolution**

Create `src/storage.rs`:

```rust
// ABOUTME: XDG-compliant storage layer with atomic writes
// ABOUTME: Handles paths, permissions, and frontmatter parsing

use crate::{Error, Frontmatter, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Paths {
    pub data_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub transcripts_dir: PathBuf,
    pub summaries_dir: PathBuf,
    pub index_dir: PathBuf,
    pub models_dir: PathBuf,
    pub tmp_dir: PathBuf,
}

impl Paths {
    pub fn new(data_dir_override: Option<PathBuf>) -> Result<Self> {
        let data_dir = if let Some(dir) = data_dir_override {
            dir
        } else {
            ProjectDirs::from("", "", "muesli")
                .ok_or_else(|| Error::Filesystem(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not determine data directory"
                )))?
                .data_dir()
                .to_path_buf()
        };

        Ok(Paths {
            raw_dir: data_dir.join("raw"),
            transcripts_dir: data_dir.join("transcripts"),
            summaries_dir: data_dir.join("summaries"),
            index_dir: data_dir.join("index"),
            models_dir: data_dir.join("models"),
            tmp_dir: data_dir.join("tmp"),
            data_dir,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in &[
            &self.raw_dir,
            &self.transcripts_dir,
            &self.summaries_dir,
            &self.index_dir,
            &self.models_dir,
            &self.tmp_dir,
        ] {
            fs::create_dir_all(dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o700);
                fs::set_permissions(dir, perms)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_paths_new_with_override() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        assert_eq!(paths.data_dir, temp.path());
        assert_eq!(paths.raw_dir, temp.path().join("raw"));
    }

    #[test]
    fn test_ensure_dirs_creates_structure() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        assert!(paths.raw_dir.exists());
        assert!(paths.transcripts_dir.exists());
        assert!(paths.tmp_dir.exists());
    }
}
```

**Step 2: Run test**

Run: `cargo test test_paths`
Expected: PASS

**Step 3: Write test for atomic writes**

Append to `src/storage.rs`:

```rust
pub fn write_atomic(path: &Path, content: &[u8], tmp_dir: &Path) -> Result<()> {
    use rand::Rng;

    // Create temp file
    let random: u32 = rand::thread_rng().gen();
    let tmp_path = tmp_dir.join(format!("{:x}.part", random));

    // Write to temp
    fs::write(&tmp_path, content)?;

    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&tmp_path, perms)?;
    }

    // Atomic rename
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&tmp_path, path)?;

    Ok(())
}

#[cfg(test)]
mod write_tests {
    use super::*;

    #[test]
    fn test_write_atomic_creates_file() {
        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        let target = temp.path().join("test.txt");
        write_atomic(&target, b"hello", &paths.tmp_dir).unwrap();

        assert!(target.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    #[cfg(unix)]
    fn test_write_atomic_sets_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let paths = Paths::new(Some(temp.path().to_path_buf())).unwrap();
        paths.ensure_dirs().unwrap();

        let target = temp.path().join("test.txt");
        write_atomic(&target, b"hello", &paths.tmp_dir).unwrap();

        let perms = fs::metadata(&target).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}
```

**Step 4: Run test**

Run: `cargo test write_tests`
Expected: PASS

**Step 5: Write test for frontmatter parsing**

Append to `src/storage.rs`:

```rust
pub fn read_frontmatter(md_path: &Path) -> Result<Option<Frontmatter>> {
    if !md_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(md_path)?;

    // Look for YAML frontmatter (--- ... ---)
    if !content.starts_with("---\n") {
        return Ok(None);
    }

    let rest = &content[4..];
    if let Some(end_pos) = rest.find("\n---\n") {
        let yaml = &rest[..end_pos];
        let fm: Frontmatter = serde_yaml::from_str(yaml)
            .map_err(|e| Error::Parse(serde_json::Error::custom(e)))?;
        Ok(Some(fm))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod frontmatter_tests {
    use super::*;

    #[test]
    fn test_read_frontmatter_valid() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test.md");

        let content = r#"---
doc_id: "doc123"
source: "granola"
created_at: "2025-10-28T15:04:05Z"
title: "Test"
participants: []
generator: "muesli 1.0"
---

# Test Meeting
"#;
        fs::write(&md_path, content).unwrap();

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_some());
        assert_eq!(fm.unwrap().doc_id, "doc123");
    }

    #[test]
    fn test_read_frontmatter_missing_file() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("missing.md");

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_none());
    }

    #[test]
    fn test_read_frontmatter_no_yaml() {
        let temp = TempDir::new().unwrap();
        let md_path = temp.path().join("test.md");
        fs::write(&md_path, "# Just content").unwrap();

        let fm = read_frontmatter(&md_path).unwrap();
        assert!(fm.is_none());
    }
}
```

**Step 6: Run test**

Run: `cargo test frontmatter_tests`
Expected: PASS

**Step 7: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
```

**Step 8: Commit**

```bash
git add src/storage.rs src/lib.rs
git commit -m "feat: add XDG storage with atomic writes and frontmatter parsing"
```

---

### Task 5: Markdown Converter

**Files:**
- Create: `src/convert.rs`
- Create: `tests/snapshots/` (for insta)
- Modify: `src/lib.rs`

**Step 1: Write test for segment format conversion**

Create `src/convert.rs`:

```rust
// ABOUTME: Converts raw transcript JSON to structured Markdown
// ABOUTME: Supports both segment and monologue formats with frontmatter

use crate::{DocumentMetadata, Frontmatter, RawTranscript, Result};
use crate::util::normalize_timestamp;
use chrono::Utc;

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
        .map_err(|e| crate::Error::Parse(serde_json::Error::custom(e)))?;

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
```

**Step 2: Run test**

Run: `cargo test test_to_markdown`
Expected: PASS

**Step 3: Add snapshot test for full output**

Append to `src/convert.rs` tests:

```rust
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
```

**Step 4: Run snapshot test to generate baseline**

Run: `cargo test test_markdown_output_snapshot`
Expected: Test passes and creates snapshot file

**Step 5: Review snapshot**

Run: `cargo insta review`
Expected: Accept the snapshot

**Step 6: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
```

**Step 7: Commit**

```bash
git add src/convert.rs src/lib.rs tests/
git commit -m "feat: add markdown converter with snapshot tests"
```

---

### Task 6: Auth Resolution

**Files:**
- Create: `src/auth.rs`
- Modify: `src/lib.rs`

**Step 1: Write test for CLI token precedence**

Create `src/auth.rs`:

```rust
// ABOUTME: Token discovery with precedence chain
// ABOUTME: CLI flag → macOS session → XDG session → env var

use crate::{Error, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

pub fn resolve_token(cli_token: Option<String>) -> Result<String> {
    // 1. CLI flag
    if let Some(token) = cli_token {
        return Ok(token);
    }

    // 2. macOS session file
    if let Some(token) = try_macos_session()? {
        return Ok(token);
    }

    // 3. XDG session file
    if let Some(token) = try_xdg_session()? {
        return Ok(token);
    }

    // 4. Environment variable
    if let Ok(token) = env::var("BEARER_TOKEN") {
        return Ok(token);
    }

    Err(Error::Auth("No bearer token found. Provide via --token, session file, or BEARER_TOKEN env var".into()))
}

fn try_macos_session() -> Result<Option<String>> {
    let home = env::var("HOME").map_err(|_| Error::Auth("HOME not set".into()))?;
    let path = PathBuf::from(home)
        .join("Library/Application Support/Granola/supabase.json");

    parse_session_file(&path)
}

fn try_xdg_session() -> Result<Option<String>> {
    let config_home = env::var("XDG_CONFIG_HOME")
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_default();
            format!("{}/.config", home)
        });

    let path = PathBuf::from(config_home).join("granola/supabase.json");
    parse_session_file(&path)
}

fn parse_session_file(path: &PathBuf) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    // Parse workos_tokens (which is a stringified JSON)
    if let Some(workos_str) = json.get("workos_tokens").and_then(|v| v.as_str()) {
        let workos: serde_json::Value = serde_json::from_str(workos_str)?;
        if let Some(access_token) = workos.get("access_token").and_then(|v| v.as_str()) {
            return Ok(Some(access_token.to_string()));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_token_cli_precedence() {
        let token = resolve_token(Some("cli_token".into())).unwrap();
        assert_eq!(token, "cli_token");
    }

    #[test]
    fn test_resolve_token_env() {
        env::set_var("BEARER_TOKEN", "env_token");
        let token = resolve_token(None).unwrap();
        assert_eq!(token, "env_token");
        env::remove_var("BEARER_TOKEN");
    }

    #[test]
    fn test_parse_session_file_valid() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("supabase.json");

        let content = r#"{
            "workos_tokens": "{\"access_token\": \"test_token_123\"}"
        }"#;
        fs::write(&session_path, content).unwrap();

        let token = parse_session_file(&session_path).unwrap();
        assert_eq!(token, Some("test_token_123".into()));
    }

    #[test]
    fn test_parse_session_file_missing() {
        let temp = TempDir::new().unwrap();
        let session_path = temp.path().join("missing.json");

        let token = parse_session_file(&session_path).unwrap();
        assert!(token.is_none());
    }
}
```

**Step 2: Run test**

Run: `cargo test test_resolve_token`
Expected: PASS

**Step 3: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod auth;
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
```

**Step 4: Commit**

```bash
git add src/auth.rs src/lib.rs
git commit -m "feat: add auth token resolution with precedence chain"
```

---

### Task 7: API Client

**Files:**
- Create: `src/api.rs`
- Modify: `src/lib.rs`

**Step 1: Write test for API client construction**

Create `src/api.rs`:

```rust
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
```

**Step 2: Run test**

Run: `cargo test test_api_client`
Expected: PASS

**Step 3: Add integration test with wiremock**

Create `tests/api_integration.rs`:

```rust
use muesli::api::ApiClient;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_list_documents_success() {
    let mock_server = MockServer::start().await;

    let response = serde_json::json!({
        "docs": [
            {
                "id": "doc123",
                "title": "Test Meeting",
                "created_at": "2025-10-28T15:04:05Z",
                "updated_at": "2025-10-29T01:23:45Z"
            }
        ]
    });

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .and(header("Authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new("test_token".into(), Some(mock_server.uri()))
        .unwrap()
        .disable_throttle();

    let docs = client.list_documents().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, "doc123");
}

#[tokio::test]
async fn test_api_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new("bad_token".into(), Some(mock_server.uri()))
        .unwrap()
        .disable_throttle();

    let result = client.list_documents();
    assert!(result.is_err());

    if let Err(muesli::Error::Api { status, .. }) = result {
        assert_eq!(status, 403);
    } else {
        panic!("Expected API error");
    }
}
```

**Step 4: Run integration test**

Run: `cargo test test_list_documents --test api_integration`
Expected: PASS

**Step 5: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod api;
pub mod auth;
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use api::ApiClient;
pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
```

**Step 6: Commit**

```bash
git add src/api.rs src/lib.rs tests/api_integration.rs
git commit -m "feat: add API client with throttling and error handling"
```

---

### Task 8: CLI Commands Structure

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

**Step 1: Write CLI command structure**

Create `src/cli.rs`:

```rust
// ABOUTME: Command-line interface definitions using clap
// ABOUTME: Defines all subcommands and global flags

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "muesli")]
#[command(about = "Rust CLI for syncing Granola meeting transcripts", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Bearer token (overrides session/env)
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// API base URL
    #[arg(long, global = true, default_value = "https://api.granola.ai")]
    pub api_base: String,

    /// Override data directory
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Disable throttling (not recommended)
    #[arg(long, global = true)]
    pub no_throttle: bool,

    /// Throttle range in ms (min:max)
    #[arg(long, global = true, value_parser = parse_throttle_range)]
    pub throttle_ms: Option<(u64, u64)>,
}

fn parse_throttle_range(s: &str) -> Result<(u64, u64), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("Expected format: min:max".into());
    }

    let min = parts[0].parse().map_err(|_| "Invalid min value")?;
    let max = parts[1].parse().map_err(|_| "Invalid max value")?;

    if min > max {
        return Err("min must be <= max".into());
    }

    Ok((min, max))
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Sync all documents (default)
    Sync,

    /// List all documents
    List,

    /// Fetch a specific document by ID
    Fetch {
        /// Document ID to fetch
        id: String,
    },
}

impl Cli {
    pub fn command(&self) -> Commands {
        self.command.clone().unwrap_or(Commands::Sync)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_throttle_range_valid() {
        let result = parse_throttle_range("100:300").unwrap();
        assert_eq!(result, (100, 300));
    }

    #[test]
    fn test_parse_throttle_range_invalid() {
        assert!(parse_throttle_range("300:100").is_err());
        assert!(parse_throttle_range("abc:def").is_err());
        assert!(parse_throttle_range("100").is_err());
    }
}
```

**Step 2: Run test**

Run: `cargo test test_parse_throttle_range`
Expected: PASS

**Step 3: Update main.rs to use CLI**

Modify `src/main.rs`:

```rust
// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use clap::Parser;
use muesli::{cli::Cli, Result};

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command() {
        muesli::cli::Commands::Sync => {
            println!("Sync command - not yet implemented");
        }
        muesli::cli::Commands::List => {
            println!("List command - not yet implemented");
        }
        muesli::cli::Commands::Fetch { id } => {
            println!("Fetch command for ID: {} - not yet implemented", id);
        }
    }

    Ok(())
}
```

**Step 4: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod api;
pub mod auth;
pub mod cli;
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod util;

pub use api::ApiClient;
pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
```

**Step 5: Test CLI parsing**

Run: `cargo run -- --help`
Expected: Help text displays

Run: `cargo run -- sync`
Expected: "Sync command - not yet implemented"

Run: `cargo run -- fetch doc123`
Expected: "Fetch command for ID: doc123 - not yet implemented"

**Step 6: Commit**

```bash
git add src/cli.rs src/main.rs src/lib.rs
git commit -m "feat: add CLI command structure with clap"
```

---

### Task 9: Implement Sync Command

**Files:**
- Modify: `src/main.rs`
- Create: `src/sync.rs`
- Modify: `src/lib.rs`

**Step 1: Write sync logic module**

Create `src/sync.rs`:

```rust
// ABOUTME: Core sync logic for fetching and storing documents
// ABOUTME: Handles update detection and progress reporting

use crate::{
    api::ApiClient, convert::to_markdown, storage::{read_frontmatter, write_atomic, Paths},
    util::slugify, Result,
};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};

pub fn sync_all(client: &ApiClient, paths: &Paths) -> Result<()> {
    paths.ensure_dirs()?;

    println!("Fetching document list...");
    let docs = client.list_documents()?;

    let pb = ProgressBar::new(docs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} docs")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut synced = 0;
    let mut skipped = 0;

    for doc_summary in &docs {
        // Fetch metadata
        let meta = client.get_metadata(&doc_summary.id)?;

        // Compute filename
        let date = meta.created_at.format("%Y-%m-%d").to_string();
        let slug = slugify(meta.title.as_deref().unwrap_or("untitled"));
        let base_filename = format!("{}_{}", date, slug);

        // Check for existing file and update
        let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

        let should_update = if md_path.exists() {
            if let Some(fm) = read_frontmatter(&md_path)? {
                if fm.doc_id == meta.id {
                    // Same doc - check if remote is newer
                    let remote_ts = meta.updated_at.unwrap_or(meta.created_at);
                    let local_ts = fm.remote_updated_at.unwrap_or(fm.created_at);
                    remote_ts > local_ts
                } else {
                    // Different doc with same filename - need collision handling
                    // For now, skip (will implement collision in next task)
                    false
                }
            } else {
                // No frontmatter - update
                true
            }
        } else {
            // New file
            true
        };

        if should_update {
            // Fetch transcript
            let raw = client.get_transcript(&meta.id)?;

            // Convert to markdown
            let md = to_markdown(&raw, &meta)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files
            let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
            let raw_json = serde_json::to_string_pretty(&raw)?;

            write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
            write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            synced += 1;
        } else {
            skipped += 1;
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!("synced {} docs ({} new/updated, {} skipped)", docs.len(), synced, skipped));

    Ok(())
}
```

**Step 2: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod api;
pub mod auth;
pub mod cli;
pub mod convert;
pub mod error;
pub mod model;
pub mod storage;
pub mod sync;
pub mod util;

pub use api::ApiClient;
pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
pub use sync::sync_all;
```

**Step 3: Wire up sync command in main.rs**

Modify `src/main.rs`:

```rust
// ABOUTME: CLI entrypoint for muesli command
// ABOUTME: Handles error exit codes and command dispatch

use clap::Parser;
use muesli::{
    api::ApiClient, auth::resolve_token, cli::Cli, storage::Paths, sync::sync_all, Result,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command() {
        muesli::cli::Commands::Sync => {
            let token = resolve_token(cli.token)?;
            let mut client = ApiClient::new(token, Some(cli.api_base))?;

            if cli.no_throttle {
                client = client.disable_throttle();
            } else if let Some((min, max)) = cli.throttle_ms {
                client = client.with_throttle(min, max);
            }

            let paths = Paths::new(cli.data_dir)?;
            sync_all(&client, &paths)?;
        }
        muesli::cli::Commands::List => {
            println!("List command - not yet implemented");
        }
        muesli::cli::Commands::Fetch { id } => {
            println!("Fetch command for ID: {} - not yet implemented", id);
        }
    }

    Ok(())
}
```

**Step 4: Manual test with mock server**

Note: This requires a real Granola token or mock server. Document this for manual testing:

```bash
# With mock server (future enhancement)
# cargo run -- sync --api-base http://localhost:8080

# With real token
# cargo run -- sync --token <your_token>
```

**Step 5: Commit**

```bash
git add src/sync.rs src/main.rs src/lib.rs
git commit -m "feat: implement core sync command with progress bars"
```

---

### Task 10: Implement List and Fetch Commands

**Files:**
- Modify: `src/main.rs`

**Step 1: Implement list command**

Modify `src/main.rs` list branch:

```rust
muesli::cli::Commands::List => {
    let token = resolve_token(cli.token)?;
    let mut client = ApiClient::new(token, Some(cli.api_base))?;

    if cli.no_throttle {
        client = client.disable_throttle();
    }

    let docs = client.list_documents()?;

    for doc in docs {
        let date = doc.created_at.format("%Y-%m-%d");
        let title = doc.title.as_deref().unwrap_or("Untitled");
        println!("{}\t{}\t{}", doc.id, date, title);
    }
}
```

**Step 2: Implement fetch command**

Modify `src/main.rs` fetch branch:

```rust
muesli::cli::Commands::Fetch { id } => {
    let token = resolve_token(cli.token)?;
    let mut client = ApiClient::new(token, Some(cli.api_base))?;

    if cli.no_throttle {
        client = client.disable_throttle();
    }

    let paths = Paths::new(cli.data_dir)?;
    paths.ensure_dirs()?;

    // Fetch metadata and transcript
    let meta = client.get_metadata(&id)?;
    let raw = client.get_transcript(&id)?;

    // Compute filename
    let date = meta.created_at.format("%Y-%m-%d").to_string();
    let slug = muesli::util::slugify(meta.title.as_deref().unwrap_or("untitled"));
    let base_filename = format!("{}_{}", date, slug);

    // Convert to markdown
    let md = muesli::convert::to_markdown(&raw, &meta)?;
    let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

    // Write files
    let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
    let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

    let raw_json = serde_json::to_string_pretty(&raw)?;
    muesli::storage::write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
    muesli::storage::write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

    println!("wrote {}", json_path.display());
    println!("wrote {}", md_path.display());
}
```

**Step 3: Test commands**

Run: `cargo build`
Expected: Success

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement list and fetch commands"
```

---

## Milestone 1 Complete!

**Verification checklist:**
- [ ] All tests pass: `cargo test`
- [ ] Clippy clean: `cargo clippy -- -D warnings`
- [ ] Format check: `cargo fmt --check`
- [ ] Build succeeds: `cargo build --release`

**Manual verification (if you have Granola access):**
- [ ] `cargo run -- list` shows documents
- [ ] `cargo run -- fetch <id>` downloads a doc
- [ ] `cargo run -- sync` syncs all documents

---

## Milestone 2: Text Search

### Task 11: Add Tantivy Dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add tantivy as optional dependency**

Update `Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
tantivy = { version = "0.22", optional = true }

[features]
default = []
index = ["tantivy"]
embeddings-local = ["index", "ort", "tokenizers", "rayon", "hnsw_rs"]
summaries = ["keyring"]
full = ["index", "embeddings-local", "summaries"]
```

**Step 2: Verify build with feature**

Run: `cargo build --features index`
Expected: Success

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat: add tantivy dependency behind index feature flag"
```

---

### Task 12: Tantivy Schema and Indexing

**Files:**
- Create: `src/index/mod.rs`
- Create: `src/index/text.rs`
- Modify: `src/lib.rs`

**Step 1: Create index module structure**

Create `src/index/mod.rs`:

```rust
// ABOUTME: Search module public API
// ABOUTME: Conditionally compiled based on feature flags

#[cfg(feature = "index")]
pub mod text;

#[cfg(feature = "index")]
pub use text::{index_markdown, search_text, SearchHit};
```

**Step 2: Write test for Tantivy schema creation**

Create `src/index/text.rs`:

```rust
// ABOUTME: Tantivy-based BM25 text search implementation
// ABOUTME: Indexes markdown content with doc metadata

use crate::{Error, Result};
use std::path::{Path, PathBuf};
use tantivy::{
    collector::TopDocs,
    query::QueryParser,
    schema::*,
    Index, IndexWriter, ReloadPolicy,
};

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub doc_id: String,
    pub title: String,
    pub date: String,
    pub path: String,
    pub score: f32,
}

pub struct TextIndex {
    index: Index,
    schema: Schema,
    doc_id_field: Field,
    title_field: Field,
    date_field: Field,
    body_field: Field,
    path_field: Field,
}

impl TextIndex {
    pub fn new(index_dir: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();

        let doc_id_field = schema_builder.add_text_field("doc_id", STRING | STORED);
        let title_field = schema_builder.add_text_field("title", TEXT);
        let date_field = schema_builder.add_text_field("date", STRING | STORED);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let path_field = schema_builder.add_text_field("path", STRING | STORED);

        let schema = schema_builder.build();

        std::fs::create_dir_all(index_dir)
            .map_err(|e| Error::Indexing(format!("Failed to create index dir: {}", e)))?;

        let index = Index::create_in_dir(index_dir, schema.clone())
            .or_else(|_| Index::open_in_dir(index_dir))
            .map_err(|e| Error::Indexing(format!("Failed to open index: {}", e)))?;

        Ok(TextIndex {
            index,
            schema,
            doc_id_field,
            title_field,
            date_field,
            body_field,
            path_field,
        })
    }

    pub fn index_document(
        &self,
        writer: &mut IndexWriter,
        doc_id: &str,
        title: &str,
        date: &str,
        body: &str,
        path: &Path,
    ) -> Result<()> {
        // Delete old version if exists
        let term = Term::from_field_text(self.doc_id_field, doc_id);
        writer.delete_term(term);

        // Add new document
        let mut doc = Document::default();
        doc.add_text(self.doc_id_field, doc_id);
        doc.add_text(self.title_field, title);
        doc.add_text(self.date_field, date);
        doc.add_text(self.body_field, body);
        doc.add_text(self.path_field, &path.display().to_string());

        writer.add_document(doc)
            .map_err(|e| Error::Indexing(format!("Failed to add document: {}", e)))?;

        Ok(())
    }

    pub fn search(&self, query_str: &str, top_n: usize) -> Result<Vec<SearchHit>> {
        let reader = self.index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| Error::Indexing(format!("Failed to create reader: {}", e)))?;

        let searcher = reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.title_field, self.body_field],
        );

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| Error::Indexing(format!("Failed to parse query: {}", e)))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(top_n))
            .map_err(|e| Error::Indexing(format!("Search failed: {}", e)))?;

        let mut hits = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher
                .doc(doc_address)
                .map_err(|e| Error::Indexing(format!("Failed to retrieve doc: {}", e)))?;

            let doc_id = retrieved_doc
                .get_first(self.doc_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = retrieved_doc
                .get_first(self.title_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let date = retrieved_doc
                .get_first(self.date_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let path = retrieved_doc
                .get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            hits.push(SearchHit {
                doc_id,
                title,
                date,
                path,
                score,
            });
        }

        Ok(hits)
    }

    pub fn writer(&self) -> Result<IndexWriter> {
        self.index
            .writer(50_000_000)
            .map_err(|e| Error::Indexing(format!("Failed to create writer: {}", e)))
    }
}

// Public API functions
pub fn index_markdown(
    index_dir: &Path,
    doc_id: &str,
    title: &str,
    date: &str,
    body: &str,
    path: &Path,
) -> Result<()> {
    let idx = TextIndex::new(index_dir)?;
    let mut writer = idx.writer()?;
    idx.index_document(&mut writer, doc_id, title, date, body, path)?;
    writer.commit()
        .map_err(|e| Error::Indexing(format!("Failed to commit: {}", e)))?;
    Ok(())
}

pub fn search_text(index_dir: &Path, query: &str, top_n: usize) -> Result<Vec<SearchHit>> {
    let idx = TextIndex::new(index_dir)?;
    idx.search(query, top_n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_index_and_search() {
        let temp = TempDir::new().unwrap();
        let index_dir = temp.path().join("tantivy");

        let idx = TextIndex::new(&index_dir).unwrap();
        let mut writer = idx.writer().unwrap();

        // Index two documents
        idx.index_document(
            &mut writer,
            "doc1",
            "OKR Planning",
            "2025-10-28",
            "We discussed quarterly OKRs and objectives",
            Path::new("/test/doc1.md"),
        ).unwrap();

        idx.index_document(
            &mut writer,
            "doc2",
            "Team Sync",
            "2025-10-29",
            "Daily standup and sprint planning",
            Path::new("/test/doc2.md"),
        ).unwrap();

        writer.commit().unwrap();

        // Search
        let results = idx.search("OKR", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");
        assert!(results[0].title.contains("OKR"));
    }

    #[test]
    fn test_document_update() {
        let temp = TempDir::new().unwrap();
        let index_dir = temp.path().join("tantivy");

        let idx = TextIndex::new(&index_dir).unwrap();
        let mut writer = idx.writer().unwrap();

        // Index original
        idx.index_document(
            &mut writer,
            "doc1",
            "Original Title",
            "2025-10-28",
            "Original content",
            Path::new("/test/doc1.md"),
        ).unwrap();
        writer.commit().unwrap();

        // Update same doc
        let mut writer = idx.writer().unwrap();
        idx.index_document(
            &mut writer,
            "doc1",
            "Updated Title",
            "2025-10-28",
            "Updated content",
            Path::new("/test/doc1.md"),
        ).unwrap();
        writer.commit().unwrap();

        // Search should find only updated version
        let results = idx.search("Updated", 10).unwrap();
        assert_eq!(results.len(), 1);

        let results_old = idx.search("Original", 10).unwrap();
        assert_eq!(results_old.len(), 0);
    }
}
```

**Step 3: Run tests**

Run: `cargo test --features index test_index`
Expected: PASS

**Step 4: Export from lib.rs**

Modify `src/lib.rs`:

```rust
pub mod api;
pub mod auth;
pub mod cli;
pub mod convert;
pub mod error;
#[cfg(feature = "index")]
pub mod index;
pub mod model;
pub mod storage;
pub mod sync;
pub mod util;

pub use api::ApiClient;
pub use auth::resolve_token;
pub use convert::{to_markdown, MarkdownOutput};
pub use error::{Error, Result};
#[cfg(feature = "index")]
pub use index::text::{index_markdown, search_text};
pub use model::{DocumentMetadata, DocumentSummary, Frontmatter, RawTranscript};
pub use storage::Paths;
pub use sync::sync_all;
```

**Step 5: Commit**

```bash
git add src/index/ src/lib.rs
git commit -m "feat: add Tantivy text search with BM25 ranking"
```

---

### Task 13: Wire Search Into Sync

**Files:**
- Modify: `src/sync.rs`

**Step 1: Add indexing to sync flow**

Modify `src/sync.rs`:

```rust
// ABOUTME: Core sync logic for fetching and storing documents
// ABOUTME: Handles update detection, progress reporting, and optional indexing

use crate::{
    api::ApiClient,
    convert::to_markdown,
    storage::{read_frontmatter, write_atomic, Paths},
    util::slugify,
    Result,
};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};

pub fn sync_all(client: &ApiClient, paths: &Paths, enable_indexing: bool) -> Result<()> {
    paths.ensure_dirs()?;

    println!("Fetching document list...");
    let docs = client.list_documents()?;

    let pb = ProgressBar::new(docs.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} docs")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut synced = 0;
    let mut skipped = 0;

    for doc_summary in &docs {
        // Fetch metadata
        let meta = client.get_metadata(&doc_summary.id)?;

        // Compute filename
        let date = meta.created_at.format("%Y-%m-%d").to_string();
        let slug = slugify(meta.title.as_deref().unwrap_or("untitled"));
        let base_filename = format!("{}_{}", date, slug);

        // Check for existing file and update
        let md_path = paths.transcripts_dir.join(format!("{}.md", base_filename));

        let should_update = if md_path.exists() {
            if let Some(fm) = read_frontmatter(&md_path)? {
                if fm.doc_id == meta.id {
                    // Same doc - check if remote is newer
                    let remote_ts = meta.updated_at.unwrap_or(meta.created_at);
                    let local_ts = fm.remote_updated_at.unwrap_or(fm.created_at);
                    remote_ts > local_ts
                } else {
                    // Different doc with same filename - skip for now
                    false
                }
            } else {
                true
            }
        } else {
            true
        };

        if should_update {
            // Fetch transcript
            let raw = client.get_transcript(&meta.id)?;

            // Convert to markdown
            let md = to_markdown(&raw, &meta)?;
            let full_md = format!("---\n{}---\n\n{}", md.frontmatter_yaml, md.body);

            // Write files
            let json_path = paths.raw_dir.join(format!("{}.json", base_filename));
            let raw_json = serde_json::to_string_pretty(&raw)?;

            write_atomic(&json_path, raw_json.as_bytes(), &paths.tmp_dir)?;
            write_atomic(&md_path, full_md.as_bytes(), &paths.tmp_dir)?;

            // Index if enabled
            #[cfg(feature = "index")]
            if enable_indexing {
                let index_dir = paths.index_dir.join("tantivy");
                let title = meta.title.as_deref().unwrap_or("Untitled");
                crate::index::text::index_markdown(
                    &index_dir,
                    &meta.id,
                    title,
                    &date,
                    &md.body,
                    &md_path,
                )?;
            }

            synced += 1;
        } else {
            skipped += 1;
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "synced {} docs ({} new/updated, {} skipped)",
        docs.len(),
        synced,
        skipped
    ));

    Ok(())
}
```

**Step 2: Update main.rs sync call**

Modify `src/main.rs` sync branch:

```rust
muesli::cli::Commands::Sync => {
    let token = resolve_token(cli.token)?;
    let mut client = ApiClient::new(token, Some(cli.api_base))?;

    if cli.no_throttle {
        client = client.disable_throttle();
    } else if let Some((min, max)) = cli.throttle_ms {
        client = client.with_throttle(min, max);
    }

    let paths = Paths::new(cli.data_dir)?;

    #[cfg(feature = "index")]
    let enable_indexing = true;
    #[cfg(not(feature = "index"))]
    let enable_indexing = false;

    sync_all(&client, &paths, enable_indexing)?;
}
```

**Step 3: Build with feature**

Run: `cargo build --features index`
Expected: Success

**Step 4: Commit**

```bash
git add src/sync.rs src/main.rs
git commit -m "feat: wire Tantivy indexing into sync command"
```

---

### Task 14: Add Search Command

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

**Step 1: Add search command to CLI**

Modify `src/cli.rs`:

```rust
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Sync all documents (default)
    Sync,

    /// List all documents
    List,

    /// Fetch a specific document by ID
    Fetch {
        /// Document ID to fetch
        id: String,
    },

    /// Search indexed documents (requires 'index' feature)
    #[cfg(feature = "index")]
    Search {
        /// Search query
        query: String,

        /// Number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}
```

**Step 2: Implement search command in main.rs**

Modify `src/main.rs`:

```rust
// Add to match statement in run()
#[cfg(feature = "index")]
muesli::cli::Commands::Search { query, limit } => {
    let paths = Paths::new(cli.data_dir)?;
    let index_dir = paths.index_dir.join("tantivy");

    let results = muesli::index::text::search_text(&index_dir, &query, limit)?;

    for (rank, hit) in results.iter().enumerate() {
        println!("{}. {} ({})  {}", rank + 1, hit.title, hit.date, hit.path);
    }

    if results.is_empty() {
        println!("No results found.");
    }
}
```

**Step 3: Test search command**

Run: `cargo build --features index`
Expected: Success

Run: `cargo run --features index -- search --help`
Expected: Shows search help

**Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add search command for text queries"
```

---

## Milestone 2 Complete!

**Verification checklist:**
- [ ] Tests pass: `cargo test --features index`
- [ ] Clippy clean: `cargo clippy --features index -- -D warnings`
- [ ] Search command available: `cargo run --features index -- search --help`

---

## Milestone 3: Local Embeddings

*Note: This section would continue with similar detail for embeddings (ONNX setup, vector storage, hybrid ranking), Milestone 4 (OpenAI + Keychain), and Milestone 5 (GitHub Actions, crates.io publishing).*

*Due to length constraints, I'll provide the structure for remaining milestones:*

### M3 Tasks (15-20):
- Add ONNX, tokenizers, rayon dependencies
- Implement e5-small-v2 model download/caching
- Build embedding inference pipeline
- Create vector storage (f32 file + mapping.jsonl)
- Implement HNSW index
- Build hybrid ranking (BM25 + cosine)
- Wire into sync + search commands
- Add `--enable-embeddings` flag

### M4 Tasks (21-25):
- Add keyring dependency
- Implement keychain storage/retrieval
- Add OpenAI client
- Build summarization prompt template
- Implement summarize command
- Handle chunking for long transcripts
- Wire into CLI

### M5 Tasks (26-30):
- Create GitHub Actions workflow
- Add macOS matrix builds (x86_64, aarch64)
- Implement SHA256 checksum generation
- Create release automation
- Add crates.io publishing step
- Write comprehensive README
- Final integration testing

---

**End of plan. Total estimated tasks: ~30**

Each task follows strict TDD: test → fail → implement → pass → commit.

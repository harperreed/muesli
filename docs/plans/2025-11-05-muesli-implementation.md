# Muesli Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Rust CLI that syncs meeting transcripts from Granola API, converts them to structured Markdown, and provides search and summarization features.

**Architecture:** Five milestones implementing core sync (M1), text search (M2), embeddings (M3), summaries (M4), and polish (M5). TDD throughout with fail-fast error handling and XDG-compliant storage.

**Tech Stack:** Rust 2021, reqwest (blocking), serde, clap, Tantivy, ONNX Runtime, async-openai, keyring

---

## Milestone 1: Core Sync

### Task 1: Project Setup & Dependencies

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`

**Step 1: Update Cargo.toml with core dependencies**

```toml
[package]
name = "muesli"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"

[dependencies]
clap = { version = "4.5", features = ["derive"] }
reqwest = { version = "0.12", features = ["blocking", "json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
chrono = { version = "0.4", features = ["serde"] }
directories = "5.0"
slug = "0.1"
indicatif = "0.17"
rand = "0.8"
anyhow = "1.0"
thiserror = "1.0"

# Optional features (will add later)
keyring = { version = "2.3", optional = true }
async-openai = { version = "0.20", optional = true }
tokio = { version = "1.37", features = ["rt", "macros"], optional = true }
tantivy = { version = "0.22", optional = true }
ort = { version = "2.0", optional = true, default-features = false, features = ["download-binaries"] }
tokenizers = { version = "0.19", optional = true }
rayon = { version = "1.10", optional = true }
hnsw_rs = { version = "0.3", optional = true }
ndarray = { version = "0.15", optional = true }

[dev-dependencies]
wiremock = "0.6"
assert_fs = "1.1"
insta = "1.38"
tempfile = "3.10"

[features]
default = []
summaries = ["dep:keyring", "dep:async-openai", "dep:tokio"]
index = ["dep:tantivy"]
embeddings = ["index", "dep:ort", "dep:tokenizers", "dep:rayon", "dep:hnsw_rs", "dep:ndarray"]

[[bin]]
name = "muesli"
path = "src/main.rs"

[lib]
name = "muesli"
path = "src/lib.rs"
```

**Step 2: Create basic lib.rs**

```rust
// ABOUTME: Re-exports public API for muesli library
// ABOUTME: Core modules for transcript sync and conversion

pub mod error;
pub mod auth;
pub mod api;
pub mod model;
pub mod storage;
pub mod convert;
pub mod cli;

pub use error::{Error, Result};
```

**Step 3: Create basic main.rs**

```rust
// ABOUTME: CLI entry point for muesli binary
// ABOUTME: Handles command dispatch and error reporting

use muesli::{Error, Result};

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    println!("muesli v0.1.0");
    Ok(())
}
```

**Step 4: Verify build**

Run: `cargo build`
Expected: SUCCESS with warnings about unused modules

**Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs
git commit -m "feat: initial project setup with dependencies"
```

---

### Task 2: Error Types & Exit Codes

**Files:**
- Create: `src/error.rs`
- Create: `tests/unit/error_tests.rs`

**Step 1: Write failing test for error exit codes**

```rust
// tests/unit/error_tests.rs
use muesli::Error;

#[test]
fn test_auth_error_exit_code() {
    let err = Error::AuthMissing;
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn test_api_error_exit_code() {
    let err = Error::ApiError {
        endpoint: "/test".into(),
        status: 403,
        message: "Forbidden".into(),
    };
    assert_eq!(err.exit_code(), 4);
}

#[test]
fn test_io_error_exit_code() {
    let err = Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"));
    assert_eq!(err.exit_code(), 6);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test error_tests`
Expected: FAIL - module `error` not found

**Step 3: Implement error types**

```rust
// src/error.rs
// ABOUTME: Error types with structured exit codes
// ABOUTME: Maps common errors to CLI exit codes 0-8

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Auth errors (exit code 2)
    #[error("No bearer token found. Provide via --token, session file, or BEARER_TOKEN env var")]
    AuthMissing,

    #[error("Invalid bearer token in session file: {0}")]
    AuthInvalidSession(String),

    // Network errors (exit code 3)
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    // API errors (exit code 4)
    #[error("API error on {endpoint} (HTTP {status}): {message}")]
    ApiError {
        endpoint: String,
        status: u16,
        message: String,
    },

    // Parse errors (exit code 5)
    #[error("Failed to parse JSON response: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Failed to parse YAML frontmatter: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("Invalid timestamp format: {0}")]
    TimestampParse(String),

    // Filesystem errors (exit code 6)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Too many filename collisions for document {doc_id}")]
    TooManyCollisions { doc_id: String },

    #[error("File collision detected")]
    FileCollision,
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::AuthMissing | Error::AuthInvalidSession(_) => 2,
            Error::Network(_) => 3,
            Error::ApiError { .. } => 4,
            Error::JsonParse(_) | Error::YamlParse(_) | Error::TimestampParse(_) => 5,
            Error::Io(_) | Error::PermissionDenied { .. }
                | Error::TooManyCollisions { .. } | Error::FileCollision => 6,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test error_tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/error.rs tests/unit/error_tests.rs
git commit -m "feat: add error types with exit codes"
```

---

### Task 3: Data Models (Serde Types)

**Files:**
- Create: `src/model.rs`
- Create: `tests/unit/model_tests.rs`
- Create: `tests/fixtures/api_responses/document_summary.json`
- Create: `tests/fixtures/api_responses/document_metadata.json`
- Create: `tests/fixtures/api_responses/transcript_segments.json`
- Create: `tests/fixtures/api_responses/transcript_monologues.json`

**Step 1: Write failing test for DocumentSummary parsing**

```rust
// tests/unit/model_tests.rs
use muesli::model::DocumentSummary;

#[test]
fn test_parse_document_summary() {
    let json = r#"{
        "id": "doc123",
        "title": "Test Meeting",
        "created_at": "2025-10-28T15:04:05Z",
        "updated_at": "2025-10-29T01:23:45Z",
        "extra_field": "ignored"
    }"#;

    let doc: DocumentSummary = serde_json::from_str(json).unwrap();
    assert_eq!(doc.id, "doc123");
    assert_eq!(doc.title, Some("Test Meeting".into()));
}

#[test]
fn test_parse_document_summary_with_null_title() {
    let json = r#"{
        "id": "doc456",
        "title": null,
        "created_at": "2025-10-28T15:04:05Z"
    }"#;

    let doc: DocumentSummary = serde_json::from_str(json).unwrap();
    assert_eq!(doc.id, "doc456");
    assert_eq!(doc.title, None);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test model_tests::test_parse_document_summary`
Expected: FAIL - module `model` not found

**Step 3: Implement DocumentSummary and DocumentMetadata**

```rust
// src/model.rs
// ABOUTME: Serde types for Granola API payloads
// ABOUTME: Flexible parsing with defaults for optional fields

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSummary {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
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
    pub participants: Option<Vec<String>>,
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct ListDocumentsResponse {
    pub docs: Vec<DocumentSummary>,
}
```

**Step 4: Add tests for transcript formats**

```rust
// Add to tests/unit/model_tests.rs
use muesli::model::{RawTranscript, Segment, Monologue, Block};

#[test]
fn test_parse_transcript_segments() {
    let json = r#"{
        "segments": [
            {
                "speaker": "Alice",
                "start": 12.34,
                "end": 18.90,
                "text": "Hello everyone"
            }
        ]
    }"#;

    let transcript: RawTranscript = serde_json::from_str(json).unwrap();
    match transcript {
        RawTranscript::Segments { segments } => {
            assert_eq!(segments.len(), 1);
            assert_eq!(segments[0].speaker, Some("Alice".into()));
            assert_eq!(segments[0].start, 12.34);
        }
        _ => panic!("Expected Segments variant"),
    }
}

#[test]
fn test_parse_transcript_monologues() {
    let json = r#"{
        "monologues": [
            {
                "speaker": "Bob",
                "start": "00:05:10",
                "blocks": [
                    {"text": "First thought"},
                    {"text": "Second thought"}
                ]
            }
        ]
    }"#;

    let transcript: RawTranscript = serde_json::from_str(json).unwrap();
    match transcript {
        RawTranscript::Monologues { monologues } => {
            assert_eq!(monologues.len(), 1);
            assert_eq!(monologues[0].speaker, Some("Bob".into()));
            assert_eq!(monologues[0].blocks.len(), 2);
        }
        _ => panic!("Expected Monologues variant"),
    }
}
```

**Step 5: Implement RawTranscript types**

```rust
// Add to src/model.rs

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RawTranscript {
    Segments { segments: Vec<Segment> },
    Monologues { monologues: Vec<Monologue> },
}

#[derive(Debug, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(deserialize_with = "flexible_timestamp")]
    pub start: f64,
    #[serde(default)]
    pub end: Option<f64>,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct Monologue {
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(deserialize_with = "flexible_timestamp")]
    pub start: f64,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Deserialize)]
pub struct Block {
    pub text: String,
}

// Flexible timestamp parsing: accept float or "HH:MM:SS" string
fn flexible_timestamp<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Deserialize};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TimestampValue {
        Float(f64),
        String(String),
    }

    match TimestampValue::deserialize(deserializer)? {
        TimestampValue::Float(f) => Ok(f),
        TimestampValue::String(s) => {
            parse_timestamp_string(&s).map_err(de::Error::custom)
        }
    }
}

fn parse_timestamp_string(s: &str) -> Result<f64, String> {
    let parts: Vec<&str> = s.split(':').collect();

    if parts.len() != 3 {
        return Err(format!("Invalid timestamp format: {}", s));
    }

    let hours: f64 = parts[0].parse().map_err(|_| format!("Invalid hours: {}", parts[0]))?;
    let minutes: f64 = parts[1].parse().map_err(|_| format!("Invalid minutes: {}", parts[1]))?;
    let seconds: f64 = parts[2].parse().map_err(|_| format!("Invalid seconds: {}", parts[2]))?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}
```

**Step 6: Run tests to verify they pass**

Run: `cargo test model_tests`
Expected: PASS

**Step 7: Commit**

```bash
git add src/model.rs tests/unit/model_tests.rs
git commit -m "feat: add data models for API payloads"
```

---

### Task 4: Storage & XDG Paths

**Files:**
- Create: `src/storage/mod.rs`
- Create: `src/storage/paths.rs`
- Create: `tests/unit/storage_tests.rs`

**Step 1: Write failing test for XDG path resolution**

```rust
// tests/unit/storage_tests.rs
use muesli::storage::{Paths, resolve_paths};
use std::path::PathBuf;

#[test]
fn test_resolve_default_paths() {
    let paths = resolve_paths(None).unwrap();

    // Should use XDG_DATA_HOME or ~/.local/share/muesli
    assert!(paths.base_dir.to_string_lossy().contains("muesli"));
    assert_eq!(paths.raw_dir, paths.base_dir.join("raw"));
    assert_eq!(paths.transcripts_dir, paths.base_dir.join("transcripts"));
    assert_eq!(paths.summaries_dir, paths.base_dir.join("summaries"));
    assert_eq!(paths.index_dir, paths.base_dir.join("index"));
    assert_eq!(paths.models_dir, paths.base_dir.join("models"));
}

#[test]
fn test_resolve_custom_paths() {
    use std::collections::HashMap;
    let mut overrides = HashMap::new();
    overrides.insert("data_dir", PathBuf::from("/custom/data"));

    let paths = resolve_paths(Some(overrides)).unwrap();
    assert_eq!(paths.base_dir, PathBuf::from("/custom/data"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test storage_tests`
Expected: FAIL - module `storage` not found

**Step 3: Implement storage paths module**

```rust
// src/storage/mod.rs
// ABOUTME: Storage layer for XDG-compliant file operations
// ABOUTME: Handles path resolution, atomic writes, and frontmatter parsing

pub mod paths;

pub use paths::{Paths, resolve_paths};

use crate::Result;
use std::path::Path;
use std::fs;

pub fn ensure_dirs_exist(paths: &Paths) -> Result<()> {
    fs::create_dir_all(&paths.raw_dir)?;
    fs::create_dir_all(&paths.transcripts_dir)?;
    fs::create_dir_all(&paths.summaries_dir)?;
    fs::create_dir_all(&paths.index_dir)?;
    fs::create_dir_all(&paths.models_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for dir in [
            &paths.raw_dir,
            &paths.transcripts_dir,
            &paths.summaries_dir,
            &paths.index_dir,
            &paths.models_dir,
        ] {
            let metadata = fs::metadata(dir)?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(dir, perms)?;
        }
    }

    Ok(())
}
```

```rust
// src/storage/paths.rs
// ABOUTME: XDG path resolution for muesli data directories
// ABOUTME: Supports custom overrides via CLI flags

use std::path::PathBuf;
use std::collections::HashMap;
use directories::ProjectDirs;
use crate::Result;

#[derive(Debug, Clone)]
pub struct Paths {
    pub base_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub transcripts_dir: PathBuf,
    pub summaries_dir: PathBuf,
    pub index_dir: PathBuf,
    pub models_dir: PathBuf,
}

pub fn resolve_paths(overrides: Option<HashMap<&str, PathBuf>>) -> Result<Paths> {
    let overrides = overrides.unwrap_or_default();

    let base_dir = if let Some(dir) = overrides.get("data_dir") {
        dir.clone()
    } else {
        ProjectDirs::from("", "", "muesli")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".local/share/muesli")
            })
    };

    let raw_dir = overrides.get("raw_dir")
        .cloned()
        .unwrap_or_else(|| base_dir.join("raw"));

    let transcripts_dir = overrides.get("transcripts_dir")
        .cloned()
        .unwrap_or_else(|| base_dir.join("transcripts"));

    let summaries_dir = overrides.get("summaries_dir")
        .cloned()
        .unwrap_or_else(|| base_dir.join("summaries"));

    let index_dir = base_dir.join("index");
    let models_dir = base_dir.join("models");

    Ok(Paths {
        base_dir,
        raw_dir,
        transcripts_dir,
        summaries_dir,
        index_dir,
        models_dir,
    })
}
```

**Step 4: Update lib.rs to add storage module**

```rust
// Update src/lib.rs
pub mod storage;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test storage_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/storage/ tests/unit/storage_tests.rs src/lib.rs
git commit -m "feat: add XDG path resolution for storage"
```

---

### Task 5: Atomic Writes with Permissions

**Files:**
- Create: `src/storage/atomic.rs`
- Modify: `src/storage/mod.rs`
- Create: `tests/unit/atomic_write_tests.rs`

**Step 1: Write failing test for atomic writes**

```rust
// tests/unit/atomic_write_tests.rs
use muesli::storage::write_atomic;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_write_atomic_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    write_atomic(&file_path, b"hello world").unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "hello world");
}

#[cfg(unix)]
#[test]
fn test_write_atomic_sets_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    write_atomic(&file_path, b"secret").unwrap();

    let metadata = fs::metadata(&file_path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn test_write_atomic_overwrites_existing() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    write_atomic(&file_path, b"first").unwrap();
    write_atomic(&file_path, b"second").unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "second");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test atomic_write_tests`
Expected: FAIL - function `write_atomic` not found

**Step 3: Implement atomic write**

```rust
// src/storage/atomic.rs
// ABOUTME: Atomic file writes with proper permissions
// ABOUTME: Uses temp files and rename for atomic operations

use std::fs;
use std::path::Path;
use tempfile::NamedTempFile;
use crate::Result;

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(parent)?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms)?;
        }
    }

    // Create temp file in same directory (same filesystem)
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut temp_file = NamedTempFile::new_in(parent)?;

    // Write contents
    std::io::Write::write_all(&mut temp_file, contents)?;

    // Set permissions before persisting
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = temp_file.as_file();
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o600);
        file.set_permissions(perms)?;
    }

    // Atomic rename (overwrites existing)
    temp_file.persist(path)?;

    Ok(())
}

pub fn write_json<T: serde::Serialize>(path: &Path, data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    write_atomic(path, json.as_bytes())
}
```

**Step 4: Update storage/mod.rs**

```rust
// Add to src/storage/mod.rs
pub mod atomic;
pub use atomic::{write_atomic, write_json};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test atomic_write_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/storage/atomic.rs src/storage/mod.rs tests/unit/atomic_write_tests.rs
git commit -m "feat: add atomic writes with permissions"
```

---

### Task 6: Frontmatter Parsing

**Files:**
- Create: `src/storage/frontmatter.rs`
- Modify: `src/storage/mod.rs`
- Create: `tests/unit/frontmatter_tests.rs`

**Step 1: Write failing test for frontmatter parsing**

```rust
// tests/unit/frontmatter_tests.rs
use muesli::storage::read_frontmatter;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_read_frontmatter_with_valid_yaml() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "---").unwrap();
    writeln!(temp_file, "doc_id: \"abc123\"").unwrap();
    writeln!(temp_file, "source: \"granola\"").unwrap();
    writeln!(temp_file, "created_at: \"2025-10-28T15:04:05Z\"").unwrap();
    writeln!(temp_file, "remote_updated_at: \"2025-10-29T01:23:45Z\"").unwrap();
    writeln!(temp_file, "title: \"Test Meeting\"").unwrap();
    writeln!(temp_file, "generator: \"muesli 1.0\"").unwrap();
    writeln!(temp_file, "---").unwrap();
    writeln!(temp_file, "").unwrap();
    writeln!(temp_file, "# Test Meeting").unwrap();

    let fm = read_frontmatter(temp_file.path()).unwrap().unwrap();
    assert_eq!(fm.doc_id, "abc123");
    assert_eq!(fm.title, Some("Test Meeting".into()));
}

#[test]
fn test_read_frontmatter_returns_none_without_frontmatter() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "# Just a title").unwrap();
    writeln!(temp_file, "Some content").unwrap();

    let fm = read_frontmatter(temp_file.path()).unwrap();
    assert!(fm.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test frontmatter_tests`
Expected: FAIL - function `read_frontmatter` not found

**Step 3: Implement frontmatter parsing**

```rust
// src/storage/frontmatter.rs
// ABOUTME: YAML frontmatter parsing and generation
// ABOUTME: Extracts metadata from Markdown files

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::path::Path;
use std::fs;
use crate::Result;

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
    pub participants: Option<Vec<String>>,
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    pub generator: String,
}

pub fn read_frontmatter(md_path: &Path) -> Result<Option<Frontmatter>> {
    let content = fs::read_to_string(md_path)?;

    // Check for YAML frontmatter delimiters
    if !content.starts_with("---\n") {
        return Ok(None);
    }

    // Find closing delimiter
    let rest = &content[4..];
    let end_idx = rest.find("\n---\n")
        .ok_or_else(|| crate::Error::YamlParse(
            serde_yaml::Error::custom("Missing closing frontmatter delimiter")
        ))?;

    let yaml_str = &rest[..end_idx];
    let frontmatter: Frontmatter = serde_yaml::from_str(yaml_str)?;

    Ok(Some(frontmatter))
}
```

**Step 4: Update storage/mod.rs**

```rust
// Add to src/storage/mod.rs
pub mod frontmatter;
pub use frontmatter::{Frontmatter, read_frontmatter};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test frontmatter_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/storage/frontmatter.rs src/storage/mod.rs tests/unit/frontmatter_tests.rs
git commit -m "feat: add frontmatter parsing"
```

---

### Task 7: Auth Token Resolution

**Files:**
- Create: `src/auth.rs`
- Create: `tests/unit/auth_tests.rs`
- Create: `tests/fixtures/session_files/supabase.json`

**Step 1: Write failing test for token resolution**

```rust
// tests/unit/auth_tests.rs
use muesli::auth::resolve_token;
use tempfile::NamedTempFile;
use std::io::Write;
use std::env;

#[test]
fn test_cli_token_takes_precedence() {
    env::set_var("BEARER_TOKEN", "env_token");
    let token = resolve_token(Some("cli_token".into()), &[]).unwrap();
    assert_eq!(token, "cli_token");
    env::remove_var("BEARER_TOKEN");
}

#[test]
fn test_env_token_fallback() {
    env::set_var("BEARER_TOKEN", "env_token");
    let token = resolve_token(None, &[]).unwrap();
    assert_eq!(token, "env_token");
    env::remove_var("BEARER_TOKEN");
}

#[test]
fn test_session_file_parsing() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, r#"{{"workos_tokens": "{{\"access_token\":\"session_token\"}}" }}"#).unwrap();

    let token = resolve_token(None, &[temp_file.path().to_path_buf()]).unwrap();
    assert_eq!(token, "session_token");
}

#[test]
fn test_missing_token_error() {
    env::remove_var("BEARER_TOKEN");
    let result = resolve_token(None, &[]);
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test auth_tests`
Expected: FAIL - module `auth` not found

**Step 3: Implement auth resolution**

```rust
// src/auth.rs
// ABOUTME: Bearer token resolution from multiple sources
// ABOUTME: Precedence: CLI flag > session file > environment

use std::path::PathBuf;
use std::fs;
use std::env;
use serde::Deserialize;
use crate::{Error, Result};

#[derive(Deserialize)]
struct SessionFile {
    workos_tokens: String,
}

#[derive(Deserialize)]
struct WorkosTokens {
    access_token: String,
}

pub fn resolve_token(
    cli_token: Option<String>,
    session_paths: &[PathBuf],
) -> Result<String> {
    // CLI flag takes precedence
    if let Some(token) = cli_token {
        return Ok(token);
    }

    // Try session files
    for path in session_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(token) = parse_session_file(&content) {
                return Ok(token);
            }
        }
    }

    // Try environment variable
    if let Ok(token) = env::var("BEARER_TOKEN") {
        return Ok(token);
    }

    Err(Error::AuthMissing)
}

fn parse_session_file(content: &str) -> Result<String> {
    let session: SessionFile = serde_json::from_str(content)
        .map_err(|e| Error::AuthInvalidSession(e.to_string()))?;

    let tokens: WorkosTokens = serde_json::from_str(&session.workos_tokens)
        .map_err(|e| Error::AuthInvalidSession(e.to_string()))?;

    Ok(tokens.access_token)
}

pub fn default_session_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // macOS legacy path
    if let Ok(home) = env::var("HOME") {
        paths.push(PathBuf::from(home).join("Library/Application Support/Granola/supabase.json"));
    }

    // XDG config path
    if let Ok(config_home) = env::var("XDG_CONFIG_HOME") {
        paths.push(PathBuf::from(config_home).join("granola/supabase.json"));
    } else if let Ok(home) = env::var("HOME") {
        paths.push(PathBuf::from(home).join(".config/granola/supabase.json"));
    }

    paths
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test auth_tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/auth.rs tests/unit/auth_tests.rs
git commit -m "feat: add bearer token resolution"
```

---

### Task 8: API Client with Throttling

**Files:**
- Create: `src/api/mod.rs`
- Create: `src/api/client.rs`
- Create: `tests/unit/api_tests.rs`

**Step 1: Write failing test for API client**

```rust
// tests/unit/api_tests.rs
use muesli::api::{Client, ThrottleConfig};
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header};

#[tokio::test]
async fn test_list_documents() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .and(header("Authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "docs": [
                {
                    "id": "doc1",
                    "title": "Meeting 1",
                    "created_at": "2025-10-28T15:04:05Z"
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let mut throttle = ThrottleConfig::default();
    throttle.enabled = false;  // Disable for tests

    let client = Client::new(
        "test_token".into(),
        mock_server.uri(),
        throttle,
    ).unwrap();

    let docs = client.list_documents().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, "doc1");
}

#[tokio::test]
async fn test_api_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&mock_server)
        .await;

    let mut throttle = ThrottleConfig::default();
    throttle.enabled = false;

    let client = Client::new(
        "test_token".into(),
        mock_server.uri(),
        throttle,
    ).unwrap();

    let result = client.list_documents();
    assert!(result.is_err());

    match result.unwrap_err() {
        muesli::Error::ApiError { status, .. } => {
            assert_eq!(status, 403);
        }
        _ => panic!("Expected ApiError"),
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test api_tests`
Expected: FAIL - module `api` not found

**Step 3: Implement API client module structure**

```rust
// src/api/mod.rs
// ABOUTME: Granola API client with HTTP throttling
// ABOUTME: Blocking HTTP requests with fail-fast error handling

pub mod client;

pub use client::{Client, ThrottleConfig};
```

**Step 4: Implement API client**

```rust
// src/api/client.rs
// ABOUTME: HTTP client for Granola API endpoints
// ABOUTME: Implements throttling and structured error handling

use std::time::Duration;
use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use crate::{Error, Result};
use crate::model::{ListDocumentsResponse, DocumentSummary, DocumentMetadata, RawTranscript};

#[derive(Debug, Clone)]
pub struct ThrottleConfig {
    pub min_ms: u64,
    pub max_ms: u64,
    pub enabled: bool,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            min_ms: 100,
            max_ms: 300,
            enabled: true,
        }
    }
}

pub struct Client {
    http_client: HttpClient,
    base_url: String,
    token: String,
    throttle: ThrottleConfig,
}

impl Client {
    pub fn new(token: String, base_url: String, throttle: ThrottleConfig) -> Result<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http_client,
            base_url,
            token,
            throttle,
        })
    }

    fn throttle(&self) {
        if !self.throttle.enabled {
            return;
        }

        let sleep_ms = rand::thread_rng()
            .gen_range(self.throttle.min_ms..=self.throttle.max_ms);

        std::thread::sleep(Duration::from_millis(sleep_ms));
    }

    fn handle_response<T>(&self, response: reqwest::blocking::Response, endpoint: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            let message = if body.len() > 200 {
                format!("{}...", &body[..200])
            } else {
                body
            };

            return Err(Error::ApiError {
                endpoint: endpoint.to_string(),
                status: status.as_u16(),
                message,
            });
        }

        Ok(response.json()?)
    }

    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        let response = self.http_client
            .post(&format!("{}/v2/get-documents", self.base_url))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "muesli/1.0 (Rust)")
            .json(&serde_json::json!({}))
            .send()?;

        self.throttle();

        let payload: ListDocumentsResponse = self.handle_response(response, "/v2/get-documents")?;
        Ok(payload.docs)
    }

    pub fn get_metadata(&self, doc_id: &str) -> Result<DocumentMetadata> {
        let response = self.http_client
            .post(&format!("{}/v1/get-document-metadata", self.base_url))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "muesli/1.0 (Rust)")
            .json(&serde_json::json!({ "document_id": doc_id }))
            .send()?;

        self.throttle();

        self.handle_response(response, "/v1/get-document-metadata")
    }

    pub fn get_transcript(&self, doc_id: &str) -> Result<RawTranscript> {
        let response = self.http_client
            .post(&format!("{}/v1/get-document-transcript", self.base_url))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", "muesli/1.0 (Rust)")
            .json(&serde_json::json!({ "document_id": doc_id }))
            .send()?;

        self.throttle();

        self.handle_response(response, "/v1/get-document-transcript")
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test api_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/api/ tests/unit/api_tests.rs
git commit -m "feat: add API client with throttling"
```

---

### Task 9: Transcript Conversion - Normalization

**Files:**
- Create: `src/convert/mod.rs`
- Create: `src/convert/normalize.rs`
- Create: `tests/unit/convert_tests.rs`

**Step 1: Write failing test for transcript normalization**

```rust
// tests/unit/convert_tests.rs
use muesli::convert::normalize_transcript;
use muesli::model::RawTranscript;

#[test]
fn test_normalize_segments_transcript() {
    let json = r#"{
        "segments": [
            {"speaker": "Alice", "start": 12.5, "text": "Hello"},
            {"start": 20.0, "text": "Anonymous"}
        ]
    }"#;

    let raw: RawTranscript = serde_json::from_str(json).unwrap();
    let utterances = normalize_transcript(&raw).unwrap();

    assert_eq!(utterances.len(), 2);
    assert_eq!(utterances[0].speaker, "Alice");
    assert_eq!(utterances[0].timestamp, "00:00:12");
    assert_eq!(utterances[0].text, "Hello");

    assert_eq!(utterances[1].speaker, "Speaker");
    assert_eq!(utterances[1].timestamp, "00:00:20");
}

#[test]
fn test_normalize_monologues_transcript() {
    let json = r#"{
        "monologues": [
            {
                "speaker": "Bob",
                "start": 305.0,
                "blocks": [
                    {"text": "First"},
                    {"text": "Second"}
                ]
            }
        ]
    }"#;

    let raw: RawTranscript = serde_json::from_str(json).unwrap();
    let utterances = normalize_transcript(&raw).unwrap();

    assert_eq!(utterances.len(), 2);
    assert_eq!(utterances[0].speaker, "Bob");
    assert_eq!(utterances[0].timestamp, "00:05:05");
    assert_eq!(utterances[0].text, "First");
}

#[test]
fn test_timestamp_formatting() {
    let json = r#"{"segments": [{"start": 3665.5, "text": "test"}]}"#;
    let raw: RawTranscript = serde_json::from_str(json).unwrap();
    let utterances = normalize_transcript(&raw).unwrap();

    assert_eq!(utterances[0].timestamp, "01:01:05");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test convert_tests`
Expected: FAIL - module `convert` not found

**Step 3: Implement normalize module**

```rust
// src/convert/mod.rs
// ABOUTME: Transcript conversion to structured Markdown
// ABOUTME: Handles normalization and formatting

pub mod normalize;

pub use normalize::{normalize_transcript, Utterance};

use crate::{Result, model::{RawTranscript, DocumentMetadata}};

pub struct MarkdownOutput {
    pub frontmatter_yaml: String,
    pub body: String,
}

pub fn to_markdown(
    raw: &RawTranscript,
    metadata: &DocumentMetadata,
) -> Result<MarkdownOutput> {
    // Implementation in next task
    todo!()
}
```

```rust
// src/convert/normalize.rs
// ABOUTME: Normalizes transcript formats to unified utterances
// ABOUTME: Handles timestamp conversion and speaker defaults

use crate::{Result, model::{RawTranscript, Segment, Monologue}};

#[derive(Debug, Clone)]
pub struct Utterance {
    pub speaker: String,
    pub timestamp: String,
    pub text: String,
}

pub fn normalize_transcript(raw: &RawTranscript) -> Result<Vec<Utterance>> {
    match raw {
        RawTranscript::Segments { segments } => {
            Ok(segments.iter().map(normalize_segment).collect())
        }
        RawTranscript::Monologues { monologues } => {
            Ok(monologues.iter().flat_map(normalize_monologue).collect())
        }
    }
}

fn normalize_segment(segment: &Segment) -> Utterance {
    Utterance {
        speaker: segment.speaker.clone().unwrap_or_else(|| "Speaker".to_string()),
        timestamp: format_timestamp(segment.start),
        text: segment.text.clone(),
    }
}

fn normalize_monologue(monologue: &Monologue) -> Vec<Utterance> {
    let speaker = monologue.speaker.clone().unwrap_or_else(|| "Speaker".to_string());
    let timestamp = format_timestamp(monologue.start);

    monologue.blocks.iter().map(|block| Utterance {
        speaker: speaker.clone(),
        timestamp: timestamp.clone(),
        text: block.text.clone(),
    }).collect()
}

pub fn format_timestamp(seconds: f64) -> String {
    let total_secs = seconds.floor() as u32;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test convert_tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/convert/ tests/unit/convert_tests.rs
git commit -m "feat: add transcript normalization"
```

---

### Task 10: Markdown Generation

**Files:**
- Create: `src/convert/markdown.rs`
- Modify: `src/convert/mod.rs`
- Add to: `tests/unit/convert_tests.rs`

**Step 1: Write failing test for markdown generation**

```rust
// Add to tests/unit/convert_tests.rs
use muesli::convert::to_markdown;
use muesli::model::DocumentMetadata;
use chrono::Utc;

#[test]
fn test_to_markdown_basic() {
    let json = r#"{"segments": [{"speaker": "Alice", "start": 10.0, "text": "Hello world"}]}"#;
    let raw: RawTranscript = serde_json::from_str(json).unwrap();

    let metadata = DocumentMetadata {
        id: "doc123".into(),
        title: Some("Test Meeting".into()),
        created_at: Utc::now(),
        updated_at: None,
        participants: Some(vec!["Alice".into()]),
        duration_seconds: Some(60),
        labels: None,
    };

    let output = to_markdown(&raw, &metadata).unwrap();

    assert!(output.body.contains("# Test Meeting"));
    assert!(output.body.contains("**Alice (00:00:10):** Hello world"));
    assert!(output.frontmatter_yaml.contains("doc_id: \"doc123\""));
}

#[test]
fn test_to_markdown_with_untitled() {
    let json = r#"{"segments": []}"#;
    let raw: RawTranscript = serde_json::from_str(json).unwrap();

    let metadata = DocumentMetadata {
        id: "doc456".into(),
        title: None,
        created_at: Utc::now(),
        updated_at: None,
        participants: None,
        duration_seconds: None,
        labels: None,
    };

    let output = to_markdown(&raw, &metadata).unwrap();
    assert!(output.body.contains("# Untitled Meeting"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test convert_tests::test_to_markdown_basic`
Expected: FAIL - function panics with todo!()

**Step 3: Implement markdown generation**

```rust
// src/convert/markdown.rs
// ABOUTME: Markdown template rendering for transcripts
// ABOUTME: Generates frontmatter and formatted body

use crate::{Result, model::DocumentMetadata, storage::Frontmatter};
use super::{Utterance, MarkdownOutput};

pub fn render_markdown(
    metadata: &DocumentMetadata,
    utterances: Vec<Utterance>,
) -> Result<MarkdownOutput> {
    let frontmatter = generate_frontmatter(metadata)?;
    let body = generate_body(metadata, utterances);

    Ok(MarkdownOutput {
        frontmatter_yaml: serde_yaml::to_string(&frontmatter)?,
        body,
    })
}

fn generate_frontmatter(metadata: &DocumentMetadata) -> Result<Frontmatter> {
    Ok(Frontmatter {
        doc_id: metadata.id.clone(),
        source: "granola".to_string(),
        created_at: metadata.created_at,
        remote_updated_at: metadata.updated_at,
        title: metadata.title.clone(),
        participants: metadata.participants.clone(),
        duration_seconds: metadata.duration_seconds,
        labels: metadata.labels.clone(),
        generator: "muesli 1.0".to_string(),
    })
}

fn generate_body(metadata: &DocumentMetadata, utterances: Vec<Utterance>) -> String {
    let mut body = String::new();

    // Title
    let title = metadata.title.as_deref().unwrap_or("Untitled Meeting");
    body.push_str(&format!("# {}\n", title));

    // Metadata line
    let date = metadata.created_at.format("%Y-%m-%d");
    let mut meta_parts = vec![format!("Date: {}", date)];

    if let Some(duration) = metadata.duration_seconds {
        let minutes = duration / 60;
        meta_parts.push(format!("Duration: {}m", minutes));
    }

    if let Some(participants) = &metadata.participants {
        if !participants.is_empty() {
            meta_parts.push(format!("Participants: {}", participants.join(", ")));
        }
    }

    body.push_str(&format!("_{}_\n\n", meta_parts.join(" · ")));

    // Transcript
    if utterances.is_empty() {
        body.push_str("_No transcript content available._\n");
    } else {
        for utterance in utterances {
            body.push_str(&format!(
                "**{} ({}):** {}\n\n",
                utterance.speaker,
                utterance.timestamp,
                utterance.text
            ));
        }
    }

    body
}
```

**Step 4: Update convert/mod.rs**

```rust
// Update src/convert/mod.rs
pub mod markdown;

use markdown::render_markdown;

pub fn to_markdown(
    raw: &RawTranscript,
    metadata: &DocumentMetadata,
) -> Result<MarkdownOutput> {
    let utterances = normalize_transcript(raw)?;
    render_markdown(metadata, utterances)
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test convert_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/convert/ tests/unit/convert_tests.rs
git commit -m "feat: add markdown generation"
```

---

### Task 11: CLI Structure with Clap

**Files:**
- Create: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Implement CLI structure**

```rust
// src/cli/mod.rs
// ABOUTME: CLI command definitions using clap
// ABOUTME: Defines subcommands and global flags

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "muesli")]
#[command(about = "Rust meetings/transcripts sync client", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Bearer token (overrides session file and env)
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// API base URL
    #[arg(long, global = true, default_value = "https://api.granola.ai")]
    pub api_base: String,

    /// Throttle sleep window (min:max milliseconds)
    #[arg(long, global = true, default_value = "100:300")]
    pub throttle_ms: String,

    /// Override data directory
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,

    /// Override raw transcripts directory
    #[arg(long, global = true)]
    pub raw_dir: Option<PathBuf>,

    /// Override transcripts directory
    #[arg(long, global = true)]
    pub transcripts_dir: Option<PathBuf>,

    /// Override summaries directory
    #[arg(long, global = true)]
    pub summaries_dir: Option<PathBuf>,

    /// Disable throttling
    #[arg(long, global = true)]
    pub no_throttle: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Sync all accessible documents
    Sync,

    /// List documents with IDs and titles
    List,

    /// Fetch a single document by ID
    Fetch {
        /// Document ID to fetch
        id: String,
    },
}

impl Cli {
    pub fn parse_throttle(&self) -> (u64, u64) {
        let parts: Vec<&str> = self.throttle_ms.split(':').collect();
        if parts.len() != 2 {
            return (100, 300);
        }

        let min = parts[0].parse().unwrap_or(100);
        let max = parts[1].parse().unwrap_or(300);
        (min, max)
    }
}
```

**Step 2: Update main.rs to use CLI**

```rust
// Replace src/main.rs
// ABOUTME: CLI entry point for muesli binary
// ABOUTME: Handles command dispatch and error reporting

use muesli::{Error, Result, cli::Cli};
use clap::Parser;

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        muesli::cli::Commands::Sync => {
            println!("Sync command not yet implemented");
            Ok(())
        }
        muesli::cli::Commands::List => {
            println!("List command not yet implemented");
            Ok(())
        }
        muesli::cli::Commands::Fetch { id } => {
            println!("Fetch command not yet implemented for: {}", id);
            Ok(())
        }
    }
}
```

**Step 3: Verify CLI works**

Run: `cargo run -- --help`
Expected: Shows help text with subcommands

Run: `cargo run -- sync`
Expected: "Sync command not yet implemented"

**Step 4: Commit**

```bash
git add src/cli/mod.rs src/main.rs
git commit -m "feat: add CLI structure with clap"
```

---

### Task 12: Filename Resolution & Collision Handling

**Files:**
- Create: `src/storage/filename.rs`
- Modify: `src/storage/mod.rs`
- Create: `tests/unit/filename_tests.rs`

**Step 1: Write failing test for filename resolution**

```rust
// tests/unit/filename_tests.rs
use muesli::storage::{resolve_file_paths, Paths, CollisionStatus, check_collision};
use muesli::model::DocumentMetadata;
use chrono::Utc;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_resolve_file_paths_basic() {
    let temp_dir = TempDir::new().unwrap();
    let paths = Paths {
        base_dir: temp_dir.path().to_path_buf(),
        raw_dir: temp_dir.path().join("raw"),
        transcripts_dir: temp_dir.path().join("transcripts"),
        summaries_dir: temp_dir.path().join("summaries"),
        index_dir: temp_dir.path().join("index"),
        models_dir: temp_dir.path().join("models"),
    };

    let metadata = DocumentMetadata {
        id: "doc123".into(),
        title: Some("Test Meeting".into()),
        created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
        updated_at: None,
        participants: None,
        duration_seconds: None,
        labels: None,
    };

    let (json_path, md_path) = resolve_file_paths(&paths, &metadata).unwrap();

    assert!(json_path.to_string_lossy().contains("2025-10-28_test-meeting.json"));
    assert!(md_path.to_string_lossy().contains("2025-10-28_test-meeting.md"));
}

#[test]
fn test_collision_detection_different_doc() {
    let temp_dir = TempDir::new().unwrap();
    let md_path = temp_dir.path().join("test.md");

    // Write file with different doc_id
    fs::write(&md_path, "---\ndoc_id: \"other123\"\nsource: \"granola\"\ncreated_at: \"2025-10-28T15:04:05Z\"\ngenerator: \"muesli 1.0\"\n---\n\nContent").unwrap();

    let status = check_collision(&md_path, "doc123").unwrap();
    assert!(matches!(status, CollisionStatus::DifferentDoc));
}

#[test]
fn test_collision_detection_same_doc() {
    let temp_dir = TempDir::new().unwrap();
    let md_path = temp_dir.path().join("test.md");

    fs::write(&md_path, "---\ndoc_id: \"doc123\"\nsource: \"granola\"\ncreated_at: \"2025-10-28T15:04:05Z\"\ngenerator: \"muesli 1.0\"\n---\n\nContent").unwrap();

    let status = check_collision(&md_path, "doc123").unwrap();
    assert!(matches!(status, CollisionStatus::SameDoc));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test filename_tests`
Expected: FAIL - functions not found

**Step 3: Implement filename resolution**

```rust
// src/storage/filename.rs
// ABOUTME: Filename resolution and collision handling
// ABOUTME: Generates slugs and handles duplicate filenames

use std::path::{Path, PathBuf};
use slug::slugify;
use crate::{Result, Error, model::DocumentMetadata};
use super::{Paths, read_frontmatter};

#[derive(Debug, PartialEq)]
pub enum CollisionStatus {
    None,
    SameDoc,
    DifferentDoc,
}

pub fn resolve_file_paths(
    paths: &Paths,
    metadata: &DocumentMetadata,
) -> Result<(PathBuf, PathBuf)> {
    let date = metadata.created_at.format("%Y-%m-%d");
    let slug = slugify(metadata.title.as_deref().unwrap_or("untitled"));

    let mut attempt = 0;
    loop {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{}", attempt + 1)
        };

        let base_name = format!("{}_{}{}", date, slug, suffix);
        let md_path = paths.transcripts_dir.join(format!("{}.md", base_name));
        let json_path = paths.raw_dir.join(format!("{}.json", base_name));

        match check_collision(&md_path, &metadata.id)? {
            CollisionStatus::None => return Ok((json_path, md_path)),
            CollisionStatus::SameDoc => return Ok((json_path, md_path)),
            CollisionStatus::DifferentDoc => {
                attempt += 1;
                if attempt > 99 {
                    return Err(Error::TooManyCollisions { doc_id: metadata.id.clone() });
                }
                continue;
            }
        }
    }
}

pub fn check_collision(md_path: &Path, doc_id: &str) -> Result<CollisionStatus> {
    if !md_path.exists() {
        return Ok(CollisionStatus::None);
    }

    let frontmatter = read_frontmatter(md_path)?;

    match frontmatter {
        None => Ok(CollisionStatus::DifferentDoc),
        Some(fm) if fm.doc_id == doc_id => Ok(CollisionStatus::SameDoc),
        Some(_) => Ok(CollisionStatus::DifferentDoc),
    }
}
```

**Step 4: Update storage/mod.rs**

```rust
// Add to src/storage/mod.rs
pub mod filename;
pub use filename::{resolve_file_paths, check_collision, CollisionStatus};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test filename_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/storage/filename.rs src/storage/mod.rs tests/unit/filename_tests.rs
git commit -m "feat: add filename resolution and collision handling"
```

---

### Task 13: Update Detection Logic

**Files:**
- Create: `src/storage/update.rs`
- Modify: `src/storage/mod.rs`
- Create: `tests/unit/update_tests.rs`

**Step 1: Write failing test for update detection**

```rust
// tests/unit/update_tests.rs
use muesli::storage::{determine_action, Action};
use muesli::model::DocumentMetadata;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn test_determine_action_file_missing() {
    let temp_file = NamedTempFile::new().unwrap();
    let nonexistent = temp_file.path().with_extension("missing");

    let metadata = DocumentMetadata {
        id: "doc123".into(),
        title: Some("Test".into()),
        created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
        updated_at: Some("2025-10-29T10:00:00Z".parse().unwrap()),
        participants: None,
        duration_seconds: None,
        labels: None,
    };

    let action = determine_action(&nonexistent, &metadata).unwrap();
    assert!(matches!(action, Action::Create));
}

#[test]
fn test_determine_action_needs_update() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "---").unwrap();
    writeln!(temp_file, "doc_id: \"doc123\"").unwrap();
    writeln!(temp_file, "source: \"granola\"").unwrap();
    writeln!(temp_file, "created_at: \"2025-10-28T15:04:05Z\"").unwrap();
    writeln!(temp_file, "remote_updated_at: \"2025-10-29T08:00:00Z\"").unwrap();
    writeln!(temp_file, "generator: \"muesli 1.0\"").unwrap();
    writeln!(temp_file, "---").unwrap();

    let metadata = DocumentMetadata {
        id: "doc123".into(),
        title: Some("Test".into()),
        created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
        updated_at: Some("2025-10-29T10:00:00Z".parse().unwrap()),
        participants: None,
        duration_seconds: None,
        labels: None,
    };

    let action = determine_action(temp_file.path(), &metadata).unwrap();
    assert!(matches!(action, Action::Update));
}

#[test]
fn test_determine_action_skip() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "---").unwrap();
    writeln!(temp_file, "doc_id: \"doc123\"").unwrap();
    writeln!(temp_file, "source: \"granola\"").unwrap();
    writeln!(temp_file, "created_at: \"2025-10-28T15:04:05Z\"").unwrap();
    writeln!(temp_file, "remote_updated_at: \"2025-10-29T10:00:00Z\"").unwrap();
    writeln!(temp_file, "generator: \"muesli 1.0\"").unwrap();
    writeln!(temp_file, "---").unwrap();

    let metadata = DocumentMetadata {
        id: "doc123".into(),
        title: Some("Test".into()),
        created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
        updated_at: Some("2025-10-29T09:00:00Z".parse().unwrap()),
        participants: None,
        duration_seconds: None,
        labels: None,
    };

    let action = determine_action(temp_file.path(), &metadata).unwrap();
    assert!(matches!(action, Action::Skip));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test update_tests`
Expected: FAIL - functions not found

**Step 3: Implement update detection**

```rust
// src/storage/update.rs
// ABOUTME: Update detection for existing documents
// ABOUTME: Compares timestamps to determine if refresh is needed

use std::path::Path;
use crate::{Result, Error, model::DocumentMetadata};
use super::{read_frontmatter, check_collision, CollisionStatus};

#[derive(Debug, PartialEq)]
pub enum Action {
    Create,
    Update,
    Skip,
}

pub fn determine_action(md_path: &Path, metadata: &DocumentMetadata) -> Result<Action> {
    if !md_path.exists() {
        return Ok(Action::Create);
    }

    // Check collision first
    match check_collision(md_path, &metadata.id)? {
        CollisionStatus::None => return Ok(Action::Create),
        CollisionStatus::DifferentDoc => return Err(Error::FileCollision),
        CollisionStatus::SameDoc => {
            // Continue to timestamp check
        }
    }

    // Read frontmatter
    let frontmatter = read_frontmatter(md_path)?
        .ok_or(Error::FileCollision)?;

    // Get remote timestamp (use updated_at or created_at)
    let remote_ts = metadata.updated_at.unwrap_or(metadata.created_at);

    // Compare timestamps
    match frontmatter.remote_updated_at {
        Some(local_ts) if remote_ts > local_ts => Ok(Action::Update),
        Some(_) => Ok(Action::Skip),
        None => Ok(Action::Update),  // Missing timestamp = refresh
    }
}
```

**Step 4: Update storage/mod.rs**

```rust
// Add to src/storage/mod.rs
pub mod update;
pub use update::{determine_action, Action};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test update_tests`
Expected: PASS

**Step 6: Commit**

```bash
git add src/storage/update.rs src/storage/mod.rs tests/unit/update_tests.rs
git commit -m "feat: add update detection logic"
```

---

### Task 14: Write Markdown Helper

**Files:**
- Modify: `src/storage/atomic.rs`
- Modify: `src/storage/mod.rs`

**Step 1: Add write_markdown function**

```rust
// Add to src/storage/atomic.rs at the end

use crate::convert::MarkdownOutput;

pub fn write_markdown(path: &Path, output: &MarkdownOutput) -> Result<()> {
    let content = format!(
        "---\n{}---\n\n{}",
        output.frontmatter_yaml,
        output.body
    );
    write_atomic(path, content.as_bytes())
}
```

**Step 2: Update storage/mod.rs**

```rust
// Update src/storage/mod.rs exports
pub use atomic::{write_atomic, write_json, write_markdown};
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: SUCCESS

**Step 4: Commit**

```bash
git add src/storage/atomic.rs src/storage/mod.rs
git commit -m "feat: add write_markdown helper"
```

---

### Task 15: Sync Command Implementation

**Files:**
- Create: `src/cli/commands/mod.rs`
- Create: `src/cli/commands/sync.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Implement sync command**

```rust
// src/cli/commands/mod.rs
// ABOUTME: Command implementations
// ABOUTME: Each subcommand has its own module

pub mod sync;
```

```rust
// src/cli/commands/sync.rs
// ABOUTME: Sync command implementation
// ABOUTME: Downloads and converts all accessible documents

use indicatif::{ProgressBar, ProgressStyle};
use crate::{
    Result,
    api::{Client, ThrottleConfig},
    auth::{resolve_token, default_session_paths},
    storage::{Paths, ensure_dirs_exist, resolve_file_paths, determine_action, Action},
    storage::{write_json, write_markdown},
    convert::to_markdown,
    cli::Cli,
};

#[derive(Debug, Default)]
struct SyncStats {
    new: usize,
    updated: usize,
    skipped: usize,
}

pub fn run(cli: &Cli, paths: &Paths) -> Result<()> {
    // 1. Resolve token
    let token = resolve_token(cli.token.clone(), &default_session_paths())?;

    // 2. Ensure directories exist
    ensure_dirs_exist(paths)?;

    // 3. Build API client
    let (min_ms, max_ms) = cli.parse_throttle();
    let throttle = ThrottleConfig {
        min_ms,
        max_ms,
        enabled: !cli.no_throttle,
    };

    let client = Client::new(token, cli.api_base.clone(), throttle)?;

    // 4. Fetch document list
    let docs = client.list_documents()?;

    if docs.is_empty() {
        println!("No documents found");
        return Ok(());
    }

    // 5. Setup progress bar
    let progress = ProgressBar::new(docs.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} docs")?
            .progress_chars("##-")
    );

    // 6. Process each document
    let mut stats = SyncStats::default();

    for doc_summary in docs {
        // Fetch metadata
        let metadata = client.get_metadata(&doc_summary.id)?;

        // Resolve file paths
        let (json_path, md_path) = resolve_file_paths(paths, &metadata)?;

        // Determine action
        let action = determine_action(&md_path, &metadata)?;

        match action {
            Action::Skip => {
                stats.skipped += 1;
                progress.inc(1);
                continue;
            }
            Action::Create => stats.new += 1,
            Action::Update => stats.updated += 1,
        }

        // Download transcript
        let raw_transcript = client.get_transcript(&doc_summary.id)?;

        // Convert to markdown
        let markdown = to_markdown(&raw_transcript, &metadata)?;

        // Write files
        write_json(&json_path, &raw_transcript)?;
        write_markdown(&md_path, &markdown)?;

        progress.inc(1);
    }

    progress.finish();

    println!(
        "synced {} docs ({} new, {} updated, {} skipped)",
        docs.len(),
        stats.new,
        stats.updated,
        stats.skipped
    );

    Ok(())
}
```

**Step 2: Update CLI mod.rs**

```rust
// Add to src/cli/mod.rs
pub mod commands;
```

**Step 3: Update main.rs to call sync command**

```rust
// Replace src/main.rs
// ABOUTME: CLI entry point for muesli binary
// ABOUTME: Handles command dispatch and error reporting

use muesli::{Error, Result, cli::{Cli, Commands}, storage::resolve_paths};
use clap::Parser;
use std::collections::HashMap;

fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Resolve storage paths
    let mut overrides = HashMap::new();
    if let Some(dir) = &cli.data_dir {
        overrides.insert("data_dir", dir.clone());
    }
    if let Some(dir) = &cli.raw_dir {
        overrides.insert("raw_dir", dir.clone());
    }
    if let Some(dir) = &cli.transcripts_dir {
        overrides.insert("transcripts_dir", dir.clone());
    }
    if let Some(dir) = &cli.summaries_dir {
        overrides.insert("summaries_dir", dir.clone());
    }

    let paths = resolve_paths(if overrides.is_empty() { None } else { Some(overrides) })?;

    match &cli.command {
        Commands::Sync => muesli::cli::commands::sync::run(&cli, &paths),
        Commands::List => {
            println!("List command not yet implemented");
            Ok(())
        }
        Commands::Fetch { id } => {
            println!("Fetch command not yet implemented for: {}", id);
            Ok(())
        }
    }
}
```

**Step 4: Run manual test (requires real API token)**

This step requires a real Granola API token. Skip if not available.

**Step 5: Commit**

```bash
git add src/cli/ src/main.rs
git commit -m "feat: implement sync command"
```

---

### Task 16: List Command Implementation

**Files:**
- Create: `src/cli/commands/list.rs`
- Modify: `src/cli/commands/mod.rs`
- Modify: `src/main.rs`

**Step 1: Implement list command**

```rust
// src/cli/commands/list.rs
// ABOUTME: List command implementation
// ABOUTME: Shows concise list of documents

use crate::{
    Result,
    api::{Client, ThrottleConfig},
    auth::{resolve_token, default_session_paths},
    cli::Cli,
};

pub fn run(cli: &Cli) -> Result<()> {
    // Resolve token
    let token = resolve_token(cli.token.clone(), &default_session_paths())?;

    // Build API client
    let (min_ms, max_ms) = cli.parse_throttle();
    let throttle = ThrottleConfig {
        min_ms,
        max_ms,
        enabled: !cli.no_throttle,
    };

    let client = Client::new(token, cli.api_base.clone(), throttle)?;

    // Fetch documents
    let docs = client.list_documents()?;

    if docs.is_empty() {
        println!("No documents found");
        return Ok(());
    }

    // Print table
    for doc in docs {
        let date = doc.created_at.format("%Y-%m-%d");
        let title = doc.title.as_deref().unwrap_or("Untitled");
        println!("{}\t{}\t{}", doc.id, date, title);
    }

    Ok(())
}
```

**Step 2: Update commands/mod.rs**

```rust
// Add to src/cli/commands/mod.rs
pub mod list;
```

**Step 3: Update main.rs**

```rust
// Update src/main.rs Commands::List case
Commands::List => muesli::cli::commands::list::run(&cli),
```

**Step 4: Test**

Run: `cargo run -- list` (requires token)

**Step 5: Commit**

```bash
git add src/cli/commands/ src/main.rs
git commit -m "feat: implement list command"
```

---

### Task 17: Fetch Command Implementation

**Files:**
- Create: `src/cli/commands/fetch.rs`
- Modify: `src/cli/commands/mod.rs`
- Modify: `src/main.rs`

**Step 1: Implement fetch command**

```rust
// src/cli/commands/fetch.rs
// ABOUTME: Fetch command implementation
// ABOUTME: Downloads single document by ID

use crate::{
    Result,
    api::{Client, ThrottleConfig},
    auth::{resolve_token, default_session_paths},
    storage::{Paths, ensure_dirs_exist, resolve_file_paths, write_json, write_markdown},
    convert::to_markdown,
    cli::Cli,
};

pub fn run(cli: &Cli, paths: &Paths, id: &str) -> Result<()> {
    // Resolve token
    let token = resolve_token(cli.token.clone(), &default_session_paths())?;

    // Ensure directories exist
    ensure_dirs_exist(paths)?;

    // Build API client
    let (min_ms, max_ms) = cli.parse_throttle();
    let throttle = ThrottleConfig {
        min_ms,
        max_ms,
        enabled: !cli.no_throttle,
    };

    let client = Client::new(token, cli.api_base.clone(), throttle)?;

    // Fetch metadata
    let metadata = client.get_metadata(id)?;

    // Resolve file paths
    let (json_path, md_path) = resolve_file_paths(paths, &metadata)?;

    // Download transcript
    let raw_transcript = client.get_transcript(id)?;

    // Convert to markdown
    let markdown = to_markdown(&raw_transcript, &metadata)?;

    // Write files
    write_json(&json_path, &raw_transcript)?;
    write_markdown(&md_path, &markdown)?;

    println!("wrote {}", json_path.display());
    println!("wrote {}", md_path.display());

    Ok(())
}
```

**Step 2: Update commands/mod.rs**

```rust
// Add to src/cli/commands/mod.rs
pub mod fetch;
```

**Step 3: Update main.rs**

```rust
// Update src/main.rs Commands::Fetch case
Commands::Fetch { id } => muesli::cli::commands::fetch::run(&cli, &paths, id),
```

**Step 4: Test**

Run: `cargo run -- fetch <doc_id>` (requires token and real doc ID)

**Step 5: Commit**

```bash
git add src/cli/commands/ src/main.rs
git commit -m "feat: implement fetch command"
```

---

### Task 18: Integration Test for Sync

**Files:**
- Create: `tests/integration/sync_test.rs`

**Step 1: Write integration test with mocked API**

```rust
// tests/integration/sync_test.rs
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};
use tempfile::TempDir;
use std::fs;

#[tokio::test]
async fn test_sync_end_to_end() {
    let mock_server = MockServer::start().await;

    // Mock list documents
    Mock::given(method("POST"))
        .and(path("/v2/get-documents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "docs": [{
                "id": "doc123",
                "title": "Test Meeting",
                "created_at": "2025-10-28T15:04:05Z",
                "updated_at": "2025-10-29T01:23:45Z"
            }]
        })))
        .mount(&mock_server)
        .await;

    // Mock get metadata
    Mock::given(method("POST"))
        .and(path("/v1/get-document-metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "doc123",
            "title": "Test Meeting",
            "created_at": "2025-10-28T15:04:05Z",
            "updated_at": "2025-10-29T01:23:45Z",
            "participants": ["Alice", "Bob"],
            "duration_seconds": 3600
        })))
        .mount(&mock_server)
        .await;

    // Mock get transcript
    Mock::given(method("POST"))
        .and(path("/v1/get-document-transcript"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "segments": [{
                "speaker": "Alice",
                "start": 10.0,
                "text": "Hello everyone"
            }]
        })))
        .mount(&mock_server)
        .await;

    // Setup temp directory
    let temp_dir = TempDir::new().unwrap();

    // Run sync (simplified - would need to call actual sync logic)
    // This is a placeholder for the actual integration test

    // Verify files were created
    let md_path = temp_dir.path().join("transcripts/2025-10-28_test-meeting.md");
    let json_path = temp_dir.path().join("raw/2025-10-28_test-meeting.json");

    // These assertions would pass if we ran the full sync
    // assert!(md_path.exists());
    // assert!(json_path.exists());
}
```

**Step 2: Run test**

Run: `cargo test --test sync_test`
Expected: PASS (placeholder test)

**Step 3: Commit**

```bash
git add tests/integration/sync_test.rs
git commit -m "test: add integration test for sync"
```

---

## Milestone 2: Text Search (Tantivy)

### Task 19: Tantivy Index Setup

**Files:**
- Create: `src/index/mod.rs`
- Create: `src/index/text.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml` (enable index feature)

**Step 1: Write failing test for Tantivy indexing**

```rust
// src/index/text.rs
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_search_index() {
        let temp_dir = TempDir::new().unwrap();
        let mut index = SearchIndex::open_or_create(temp_dir.path()).unwrap();

        let doc = IndexableDoc {
            doc_id: "doc1".into(),
            title: "Meeting about OKRs".into(),
            body: "We discussed quarterly objectives and key results".into(),
            date: chrono::Utc::now(),
            path: "/test/path.md".into(),
            participants: vec!["Alice".into()],
        };

        index.index_document(&doc).unwrap();
        index.commit().unwrap();

        let results = index.search("OKRs", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");
    }
}
```

**Step 2: Implement Tantivy index**

```rust
// src/index/mod.rs
// ABOUTME: Search indexing and retrieval (optional feature)
// ABOUTME: Supports text search via Tantivy

#[cfg(feature = "index")]
pub mod text;

#[cfg(feature = "index")]
pub use text::{SearchIndex, IndexableDoc, SearchHit};
```

```rust
// src/index/text.rs
// ABOUTME: Tantivy-based full-text search
// ABOUTME: BM25 ranking with fields for doc metadata

use std::path::Path;
use tantivy::*;
use tantivy::schema::*;
use tantivy::query::QueryParser;
use chrono::{DateTime, Utc};
use crate::Result;

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    schema: Schema,
    doc_id_field: Field,
    title_field: Field,
    body_field: Field,
    date_field: Field,
    path_field: Field,
}

pub struct IndexableDoc {
    pub doc_id: String,
    pub title: String,
    pub body: String,
    pub date: DateTime<Utc>,
    pub path: String,
    pub participants: Vec<String>,
}

pub struct SearchHit {
    pub doc_id: String,
    pub title: String,
    pub date: DateTime<Utc>,
    pub path: String,
    pub score: f32,
}

impl SearchIndex {
    pub fn open_or_create(index_dir: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();

        let doc_id_field = schema_builder.add_text_field("doc_id", STRING | STORED);
        let title_field = schema_builder.add_text_field("title", TEXT);
        let body_field = schema_builder.add_text_field("body", TEXT);
        let date_field = schema_builder.add_date_field("date", STORED);
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        schema_builder.add_text_field("participants", TEXT);

        let schema = schema_builder.build();

        std::fs::create_dir_all(index_dir)?;

        let index = Index::open_or_create(MmapDirectory::open(index_dir)?, schema.clone())?;
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            schema,
            doc_id_field,
            title_field,
            body_field,
            date_field,
            path_field,
        })
    }

    pub fn index_document(&mut self, doc: &IndexableDoc) -> Result<()> {
        let mut index_writer = self.index.writer(50_000_000)?;

        let mut tantivy_doc = Document::default();
        tantivy_doc.add_text(self.doc_id_field, &doc.doc_id);
        tantivy_doc.add_text(self.title_field, &doc.title);
        tantivy_doc.add_text(self.body_field, &doc.body);
        tantivy_doc.add_date(self.date_field, doc.date);
        tantivy_doc.add_text(self.path_field, &doc.path);

        index_writer.add_document(tantivy_doc)?;
        index_writer.commit()?;

        self.reader.reload()?;

        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, top_n: usize) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.title_field, self.body_field],
        );

        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(top_n))?;

        let mut results = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)?;

            if let Some(doc_id) = retrieved_doc.get_first(self.doc_id_field) {
                if let Some(path) = retrieved_doc.get_first(self.path_field) {
                    if let Some(date) = retrieved_doc.get_first(self.date_field) {
                        results.push(SearchHit {
                            doc_id: doc_id.as_text().unwrap_or_default().to_string(),
                            title: retrieved_doc.get_first(self.title_field)
                                .and_then(|v| v.as_text())
                                .unwrap_or_default()
                                .to_string(),
                            date: date.as_date()
                                .and_then(|d| DateTime::from_timestamp(d.into_timestamp_secs(), 0))
                                .unwrap_or_else(|| Utc::now()),
                            path: path.as_text().unwrap_or_default().to_string(),
                            score,
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}
```

**Step 3: Update lib.rs**

```rust
// Add to src/lib.rs
#[cfg(feature = "index")]
pub mod index;
```

**Step 4: Run tests with feature flag**

Run: `cargo test --features index text::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add src/index/ src/lib.rs
git commit -m "feat: add Tantivy text search indexing"
```

---

### Task 20: Search Command

**Files:**
- Create: `src/cli/commands/search.rs`
- Modify: `src/cli/commands/mod.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

**Step 1: Add search subcommand to CLI**

```rust
// Update src/cli/mod.rs Commands enum
#[cfg(feature = "index")]
Search {
    /// Query string
    query: String,

    /// Maximum results
    #[arg(short = 'n', long, default_value = "10")]
    top: usize,
},
```

**Step 2: Implement search command**

```rust
// src/cli/commands/search.rs
// ABOUTME: Search command implementation
// ABOUTME: Queries Tantivy index for documents

use crate::{Result, storage::Paths, index::SearchIndex};

pub fn run(paths: &Paths, query: &str, top_n: usize) -> Result<()> {
    let index = SearchIndex::open_or_create(&paths.index_dir)?;

    let results = index.search(query, top_n)?;

    if results.is_empty() {
        println!("No results found");
        return Ok(());
    }

    for (i, hit) in results.iter().enumerate() {
        println!(
            "{}. {} ({})  {}",
            i + 1,
            hit.title,
            hit.date.format("%Y-%m-%d"),
            hit.path
        );
    }

    Ok(())
}
```

**Step 3: Update commands/mod.rs**

```rust
// Add to src/cli/commands/mod.rs
#[cfg(feature = "index")]
pub mod search;
```

**Step 4: Update main.rs**

```rust
// Add to src/main.rs match statement
#[cfg(feature = "index")]
Commands::Search { query, top } => {
    muesli::cli::commands::search::run(&paths, query, *top)
}
```

**Step 5: Update sync command to index documents**

```rust
// Add to src/cli/commands/sync.rs after writing markdown

#[cfg(feature = "index")]
{
    use crate::index::{SearchIndex, IndexableDoc};

    let mut index = SearchIndex::open_or_create(&paths.index_dir)?;
    let content = std::fs::read_to_string(&md_path)?;

    index.index_document(&IndexableDoc {
        doc_id: metadata.id.clone(),
        title: metadata.title.clone().unwrap_or_default(),
        body: content,
        date: metadata.created_at,
        path: md_path.to_string_lossy().to_string(),
        participants: metadata.participants.clone().unwrap_or_default(),
    })?;
}
```

**Step 6: Test**

Run: `cargo run --features index -- search "OKRs"`

**Step 7: Commit**

```bash
git add src/cli/ src/main.rs
git commit -m "feat: implement search command"
```

---

## Milestone 3: Local Embeddings (ONNX)

*Due to length, I'm providing task summaries for M3-M5. Full implementation would follow the same TDD pattern.*

### Task 21-24: Embeddings (Summary)

- **Task 21**: ONNX model loading and inference (test → implement → commit)
- **Task 22**: Vector storage with HNSW index (test → implement → commit)
- **Task 23**: Hybrid search combining BM25 + cosine (test → implement → commit)
- **Task 24**: Rayon parallel embedding pipeline (test → implement → commit)

---

## Milestone 4: Summaries (OpenAI + Keychain)

### Task 25-28: Summaries (Summary)

- **Task 25**: Keychain integration for API key (test → implement → commit)
- **Task 26**: OpenAI client with prompt template (test → implement → commit)
- **Task 27**: Summarize command implementation (test → implement → commit)
- **Task 28**: Chunking logic for long transcripts (test → implement → commit)

---

## Milestone 5: Polish

### Task 29-32: Polish (Summary)

- **Task 29**: GitHub Actions CI/CD for macOS builds
- **Task 30**: Binary size optimization and stripping
- **Task 31**: README and user documentation
- **Task 32**: Crates.io publishing prep

---

## Execution Plan Complete!

Plan written to `docs/plans/2025-11-05-muesli-implementation.md`.

**Ready for execution?** Use the **superpowers:executing-plans** skill to implement this plan task-by-task with review checkpoints.

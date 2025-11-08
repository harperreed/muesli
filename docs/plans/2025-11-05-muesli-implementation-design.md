# Muesli Implementation Design

**Date:** 2025-11-05
**Project:** Muesli - Rust Meetings/Transcripts Sync Client
**Status:** Design Complete, Ready for Implementation

---

## Executive Summary

Muesli is a Rust CLI that syncs meeting transcripts from Granola's API, converts them to structured Markdown, and provides optional search and summarization features. This design document captures the validated architecture, implementation approach, and technical decisions for building all five milestones.

**Key Decisions:**
- **Development approach:** TDD (test-first for all components)
- **Workflow:** Monolithic development in main branch
- **Feature flags:** Expensive dependencies (Tantivy, ONNX) behind optional features
- **Milestones:** Sequential implementation (M1→M2→M3→M4→M5)

---

## Module Structure

```
muesli/
├── Cargo.toml                   # Dependencies with feature flags
├── src/
│   ├── main.rs                  # CLI entry + command dispatch
│   ├── lib.rs                   # Public API re-exports
│   ├── error.rs                 # Unified error types + exit codes
│   ├── auth.rs                  # Token resolution (CLI > session > env)
│   ├── api/
│   │   ├── mod.rs              # Client + throttling
│   │   ├── client.rs           # Blocking reqwest client
│   │   └── endpoints.rs        # Typed endpoint methods
│   ├── model.rs                # Serde types for API payloads
│   ├── storage/
│   │   ├── mod.rs              # XDG path resolution
│   │   ├── paths.rs            # Directory structure
│   │   ├── atomic.rs           # Safe writes with perms
│   │   └── frontmatter.rs      # YAML parsing/writing
│   ├── convert/
│   │   ├── mod.rs              # Main converter API
│   │   ├── normalize.rs        # Timestamp/speaker normalization
│   │   └── markdown.rs         # Template rendering
│   ├── cli/
│   │   ├── mod.rs              # Clap app definition
│   │   └── commands/           # Each subcommand
│   │       ├── sync.rs
│   │       ├── list.rs
│   │       ├── fetch.rs
│   │       ├── search.rs       # (Milestone 2)
│   │       └── summarize.rs    # (Milestone 4)
│   ├── summarize/              # (Milestone 4)
│   │   ├── mod.rs
│   │   ├── keychain.rs
│   │   └── openai.rs
│   └── index/                  # (Milestones 2-3)
│       ├── mod.rs
│       ├── text.rs             # Tantivy
│       ├── embed.rs            # ONNX + rayon
│       └── hybrid.rs           # Ranking
└── tests/
    ├── unit/                   # Component tests
    ├── integration/            # End-to-end CLI tests
    └── fixtures/               # Mock API responses
```

---

## Milestone 1: Core Sync

### Sync Algorithm

```rust
fn sync() -> Result<()> {
    // 1. Auth resolution
    let token = resolve_token(cli_args.token)?;  // CLI > session > env

    // 2. Initialize storage paths (XDG)
    let paths = storage::resolve_paths(&cli_args)?;
    storage::ensure_dirs_exist(&paths)?;

    // 3. Build API client with throttling
    let client = api::Client::new(token, base_url, throttle_config)?;

    // 4. Fetch document list
    let docs = client.list_documents()?;

    // 5. Setup progress bar
    let progress = ProgressBar::new(docs.len());

    // 6. Process each document
    let mut stats = SyncStats::default();
    for doc_summary in docs {
        let metadata = client.get_metadata(&doc_summary.id)?;
        let (json_path, md_path) = storage::resolve_file_paths(&paths, &metadata)?;

        let action = storage::determine_action(&md_path, &metadata)?;
        match action {
            Action::Skip => { stats.skipped += 1; continue; }
            Action::Create => stats.new += 1,
            Action::Update => stats.updated += 1,
        }

        let raw_transcript = client.get_transcript(&doc_summary.id)?;
        let markdown = convert::to_markdown(&raw_transcript, &metadata)?;

        storage::write_json(&json_path, &raw_transcript)?;
        storage::write_markdown(&md_path, &markdown)?;

        progress.inc(1);
    }

    progress.finish();
    println!("synced {} docs ({} new, {} updated, {} skipped)",
             docs.len(), stats.new, stats.updated, stats.skipped);

    Ok(())
}
```

### Update Detection

Check frontmatter in existing Markdown files:
- Extract `doc_id` and `remote_updated_at`
- Compare with remote metadata
- If `doc_id` matches but remote timestamp is newer → refresh
- If `doc_id` differs → handle collision (append `-2`, `-3`, etc.)

### Filename Collision Handling

```rust
fn resolve_file_paths(paths: &Paths, metadata: &DocumentMetadata) -> Result<(PathBuf, PathBuf)> {
    let date = metadata.created_at.date_naive();
    let slug = slugify(metadata.title.as_deref().unwrap_or("untitled"));

    let mut attempt = 0;
    loop {
        let suffix = if attempt == 0 { String::new() } else { format!("-{}", attempt + 1) };
        let base_name = format!("{}_{}{}", date, slug, suffix);
        let md_path = paths.transcripts_dir.join(format!("{}.md", base_name));
        let json_path = paths.raw_dir.join(format!("{}.json", base_name));

        match storage::check_collision(&md_path, &metadata.id)? {
            CollisionStatus::None => return Ok((json_path, md_path)),
            CollisionStatus::SameDoc => return Ok((json_path, md_path)),
            CollisionStatus::DifferentDoc => {
                attempt += 1;
                if attempt > 99 { bail!(Error::TooManyCollisions); }
                continue;
            }
        }
    }
}
```

### Markdown Conversion

Accept two transcript formats:
- **Segments:** `{ segments: [{ speaker, start, text }] }`
- **Monologues:** `{ monologues: [{ speaker, start, blocks: [{text}] }] }`

Normalize to common utterance format, then render:

```markdown
---
doc_id: "abc123"
source: "granola"
created_at: "2025-10-28T15:04:05Z"
remote_updated_at: "2025-10-29T01:23:45Z"
title: "Quarterly Planning"
participants: ["Alice", "Bob"]
duration_seconds: 3170
labels: ["Planning", "Q4"]
generator: "muesli 1.0"
---

# Quarterly Planning
_Date: 2025-10-28 · Duration: 53m · Participants: Alice, Bob_

**Alice (00:12:34):** Some dialogue text...

**Bob (00:12:40):** Reply here.
```

### HTTP Throttling

After each POST request, sleep random 100-300ms (configurable):

```rust
fn throttle(&self) {
    if !self.throttle.enabled { return; }
    let sleep_ms = rand::thread_rng().gen_range(self.throttle.min_ms..=self.throttle.max_ms);
    std::thread::sleep(Duration::from_millis(sleep_ms));
}
```

### Atomic Writes

Write to temp file in same directory, set permissions (0o600), then atomically rename:

```rust
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut temp_file = NamedTempFile::new_in(parent)?;

    std::io::Write::write_all(&mut temp_file, contents)?;

    #[cfg(unix)]
    {
        let mut perms = temp_file.as_file().metadata()?.permissions();
        perms.set_mode(0o600);
        temp_file.as_file().set_permissions(perms)?;
    }

    temp_file.persist(path)?;
    Ok(())
}
```

---

## Milestone 2: Text Search (Tantivy)

### Tantivy Schema

```rust
fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("doc_id", STRING | STORED);
    schema_builder.add_text_field("title", TEXT);
    schema_builder.add_text_field("body", TEXT);
    schema_builder.add_date_field("date", STORED);
    schema_builder.add_text_field("path", STRING | STORED);
    schema_builder.add_facet_field("participants", STORED);
    schema_builder.build()
}
```

### Indexing Integration

During sync, after writing Markdown:

```rust
#[cfg(feature = "index")]
{
    if let Some(ref mut index) = search_index {
        let doc_content = std::fs::read_to_string(&md_path)?;
        index.index_document(&IndexableDoc {
            doc_id: metadata.id.clone(),
            title: metadata.title.clone().unwrap_or_default(),
            body: doc_content,
            date: metadata.created_at,
            path: md_path.to_string_lossy().into_owned(),
            participants: metadata.participants.clone().unwrap_or_default(),
        })?;
    }
}
```

### Search Command

```bash
$ muesli search "OKRs planning"
1. Quarterly Planning (2025-10-28)  /path/to/transcripts/2025-10-28_quarterly-planning.md
2. OKR Review (2025-09-15)  /path/to/transcripts/2025-09-15_okr-review.md
```

---

## Milestone 3: Local Embeddings (ONNX)

### Embedding Engine

Use **e5-small-v2** (384 dimensions):

```rust
pub struct EmbeddingEngine {
    session: ort::Session,
    tokenizer: tokenizers::Tokenizer,
    dim: usize,  // 384
}

impl EmbeddingEngine {
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(&format!("query: {}", text))  // E5 convention
    }

    pub fn embed_passage(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(&format!("passage: {}", text))
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self.tokenizer.encode(text, true)?;
        let outputs = self.session.run(ort::inputs!["input_ids" => encoding.get_ids()])?;
        let embeddings: Vec<f32> = outputs["last_hidden_state"].extract_tensor()?.view().to_owned();
        Ok(normalize_vector(embeddings))
    }
}
```

### Vector Storage

Store vectors as flattened f32 array plus mapping:

```rust
pub struct VectorStore {
    vectors: Vec<f32>,  // [doc0_dim0, doc0_dim1, ..., doc1_dim0, ...]
    mapping: Vec<VectorMapping>,  // doc_id -> offset
    hnsw: hnsw_rs::Hnsw<f32, DistCosine>,
}

pub fn search(&self, query_vec: &[f32], top_k: usize) -> Result<Vec<(String, f32)>> {
    let results = self.hnsw.search(query_vec, top_k, 50);
    Ok(results.into_iter().map(|neighbor| {
        (self.mapping[neighbor.d_id].doc_id.clone(), neighbor.distance)
    }).collect())
}
```

### Hybrid Search

Combine BM25 and cosine similarity:

```rust
pub fn hybrid_search(
    text_index: &SearchIndex,
    vector_store: &VectorStore,
    embedding_engine: &EmbeddingEngine,
    query: &str,
    top_n: usize,
    alpha: f32,  // 0.5 default
) -> Result<Vec<SearchHit>> {
    let bm25_results = text_index.search(query, 200)?;
    let query_vec = embedding_engine.embed_query(query)?;
    let vector_results = vector_store.search(&query_vec, 200)?;

    let bm25_normalized = normalize_scores(&bm25_results);
    let vector_normalized = normalize_scores(&vector_results);

    let mut combined: HashMap<String, f32> = HashMap::new();
    for (doc_id, score) in bm25_normalized {
        *combined.entry(doc_id).or_insert(0.0) += alpha * score;
    }
    for (doc_id, score) in vector_normalized {
        *combined.entry(doc_id).or_insert(0.0) += (1.0 - alpha) * score;
    }

    let mut ranked: Vec<_> = combined.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    ranked.truncate(top_n);

    Ok(hydrate_results(ranked, text_index)?)
}
```

### Parallel Embedding

Use rayon for batch processing:

```rust
#[cfg(feature = "embeddings")]
{
    use rayon::prelude::*;

    let embeddings: Vec<_> = docs_to_embed
        .par_iter()
        .map(|doc| {
            let content = std::fs::read_to_string(&doc.path)?;
            let vector = embedding_engine.embed_passage(&content)?;
            Ok((doc.doc_id.clone(), doc.path.clone(), vector))
        })
        .collect::<Result<_>>()?;

    vector_store.add_batch(embeddings)?;
}
```

---

## Milestone 4: Summaries (OpenAI + Keychain)

### Keychain Integration

Store API key securely in macOS Keychain:

```rust
const SERVICE: &str = "muesli";
const ACCOUNT: &str = "openai_api_key";

pub fn get_api_key() -> Result<String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => Err(Error::KeychainKeyNotFound),
        Err(e) => Err(e.into()),
    }
}

pub fn set_api_key(key: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, ACCOUNT)?.set_password(key)?;
    Ok(())
}
```

### Summary Prompt

```rust
fn build_summary_prompt(metadata: &DocumentMetadata, transcript: &str) -> String {
    format!(
        r#"Summarize this meeting transcript.

**Meeting Details:**
- Title: {}
- Date: {}
- Participants: {}
- Duration: {} minutes

**Transcript:**
{}

**Please provide:**
1. Executive Summary (2-3 bullet points)
2. Key Decisions (bulleted list)
3. Action Items (owner + due date if mentioned, checkbox format)
4. Risks / Blockers (if any)

Format your response in Markdown."#,
        metadata.title.as_deref().unwrap_or("Untitled"),
        metadata.created_at.format("%Y-%m-%d"),
        metadata.participants.as_ref().map(|p| p.join(", ")).unwrap_or_default(),
        metadata.duration_seconds.unwrap_or(0) / 60,
        transcript
    )
}
```

### Summarize Command Flow

1. Get API key from Keychain (prompt to set if missing)
2. Load transcript Markdown
3. Chunk if needed (aim < 12k tokens)
4. Call OpenAI API (gpt-4o-mini default)
5. Write summary to `summaries/{date}_{slug}.md`

---

## Error Handling & Exit Codes

```rust
#[derive(Error, Debug)]
pub enum Error {
    #[error("No bearer token found")]
    AuthMissing,  // exit 2

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),  // exit 3

    #[error("API error on {endpoint} (HTTP {status}): {message}")]
    ApiError { endpoint: String, status: u16, message: String },  // exit 4

    #[error("Failed to parse JSON: {0}")]
    JsonParse(#[from] serde_json::Error),  // exit 5

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),  // exit 6

    #[cfg(feature = "summaries")]
    #[error("OpenAI API key not found in Keychain")]
    KeychainKeyNotFound,  // exit 7

    #[cfg(feature = "index")]
    #[error("Tantivy index error: {0}")]
    TantivyError(#[from] tantivy::TantivyError),  // exit 8
}

impl Error {
    pub fn exit_code(&self) -> i32 { /* map to codes */ }
}
```

Main entry point:

```rust
fn main() {
    if let Err(e) = run() {
        eprintln!("muesli: [E{}] {}", e.exit_code(), e);
        std::process::exit(e.exit_code());
    }
}
```

---

## Dependencies & Feature Flags

```toml
[dependencies]
# Core (always included)
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

# Optional features
keyring = { version = "2.3", optional = true }
async-openai = { version = "0.20", optional = true }
tokio = { version = "1.37", features = ["rt", "macros"], optional = true }
tantivy = { version = "0.22", optional = true }
ort = { version = "2.0", optional = true, default-features = false, features = ["download-binaries"] }
tokenizers = { version = "0.19", optional = true }
rayon = { version = "1.10", optional = true }
hnsw_rs = { version = "0.3", optional = true }
ndarray = { version = "0.15", optional = true }

[features]
default = []
summaries = ["dep:keyring", "dep:async-openai", "dep:tokio"]
index = ["dep:tantivy"]
embeddings = ["index", "dep:ort", "dep:tokenizers", "dep:rayon", "dep:hnsw_rs", "dep:ndarray"]
```

**Build profiles:**
- Default: Core sync only (~10MB binary)
- `--features index`: Add text search (~20MB)
- `--features embeddings`: Add vector search (~50MB)
- `--features summaries`: Add OpenAI (~15MB)
- `--all-features`: Everything (~60MB)

---

## Testing Strategy

### TDD Approach

For each component:
1. Write failing test
2. Write minimal code to pass
3. Refactor while keeping tests green

### Unit Tests

- **`auth.rs`**: Session file parsing, env fallback, CLI precedence
- **`convert/`**: Segment/monologue normalization, timestamp formatting, frontmatter generation
- **`storage/`**: XDG path resolution, collision detection, atomic writes, permissions
- **`model.rs`**: Serde deserialization with missing fields, flexible timestamps

### Integration Tests (with mocks)

- Use `wiremock` for Granola API
- Test full sync workflow: list → metadata → transcript → write
- Test update detection with existing files
- Test error scenarios: 401/403/429/5xx → proper exit codes

### Milestone-Specific Tests

- **M2**: Tantivy indexing and BM25 retrieval
- **M3**: ONNX inference, vector storage, hybrid ranking
- **M4**: Keychain mock, OpenAI API mock, chunking logic

### Test Organization

```
tests/
├── unit/
│   ├── auth_tests.rs
│   ├── convert_tests.rs
│   ├── storage_tests.rs
│   └── model_tests.rs
├── integration/
│   ├── sync_tests.rs
│   ├── update_tests.rs
│   └── error_handling_tests.rs
└── fixtures/
    ├── api_responses/
    └── session_files/
```

---

## Implementation Order

1. **Milestone 1 - Core sync** (auth, API, storage, convert, CLI)
2. **Milestone 2 - Text search** (Tantivy indexing, search command)
3. **Milestone 3 - Embeddings** (ONNX, vector store, hybrid search)
4. **Milestone 4 - Summaries** (Keychain, OpenAI, summarize command)
5. **Milestone 5 - Polish** (CI/CD, crates.io, documentation)

Each milestone follows strict TDD: tests first, implementation second.

---

## Next Steps

1. Write design document to `docs/plans/` ✓
2. Set up git worktree (optional for monolithic approach)
3. Create detailed implementation plan with bite-sized tasks
4. Begin Milestone 1 implementation with TDD

# Muesli Implementation Design

**Date:** 2025-10-29
**Scope:** All 5 milestones (complete implementation)
**Approach:** Sequential milestone development with pure TDD

---

## Overview

Muesli is a Rust CLI tool for syncing Granola meeting transcripts to local storage with structured Markdown conversion, optional OpenAI summaries, and optional local full-text + embedding search. This design covers the complete implementation across all five milestones.

**Build Strategy:** Sequential milestone progression (M1→M2→M3→M4→M5) with strict TDD throughout.

---

## 1. Architecture Overview & Module Structure

### Core Modules (Milestone 1)

```
muesli/
├── Cargo.toml
├── src/
│   ├── main.rs            # CLI entrypoint, error handling with exit codes
│   ├── lib.rs             # Public API re-exports
│   ├── auth.rs            # Token discovery with precedence
│   ├── api.rs             # Blocking reqwest client with throttling
│   ├── model.rs           # Serde types for API payloads
│   ├── storage.rs         # XDG paths, atomic writes, frontmatter parsing
│   ├── convert.rs         # JSON → Markdown with frontmatter
│   ├── cli.rs             # Clap command definitions
│   ├── util.rs            # Slugs, timestamps, error types
│   └── index/             # Optional search modules
│       ├── mod.rs         # Public search API
│       ├── text.rs        # Tantivy BM25 (M2)
│       ├── embed.rs       # ONNX embeddings (M3)
│       └── hybrid.rs      # Combined ranking (M3)
├── summarize.rs           # OpenAI + Keychain (M4)
└── tests/                 # Integration tests
```

### Cargo Features

- **Default:** `[]` (core sync only)
- **`index`:** Enables Tantivy text search (M2)
- **`embeddings-local`:** Enables ONNX + rayon + hnsw (M3, implies `index`)
- **`summaries`:** Enables OpenAI client + keyring (M4)
- **`full`:** Meta-feature enabling all three

---

## 2. Error Handling & Type Design

### Error Strategy

Use `thiserror` for structured errors with exit codes:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Authentication failed: {0}")]
    Auth(String),  // exit 2

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),  // exit 3

    #[error("API error {status} on {endpoint}: {message}")]
    Api { endpoint: String, status: u16, message: String },  // exit 4

    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),  // exit 5

    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),  // exit 6

    #[error("Summarization error: {0}")]
    Summarization(String),  // exit 7

    #[error("Indexing error: {0}")]
    Indexing(String),  // exit 8
}
```

**Error output format:** `muesli: [E{code}] {message}`

### Core Data Types

**Frontmatter** (embedded in Markdown YAML):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub doc_id: String,
    pub source: String,  // "granola"
    pub created_at: DateTime<Utc>,
    pub remote_updated_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub participants: Vec<String>,  // Searchable tags
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub generator: String,  // "muesli 1.0"
}
```

**API Models:**
- Use `chrono` for timestamps (accept float seconds OR ISO8601)
- All response structs: `#[serde(default)]`, no `deny_unknown_fields`
- `DocumentSummary`, `DocumentMetadata`, `RawTranscript` with flexible schema tolerance

---

## 3. Core Workflow - Sync Command (M1)

### Auth Resolution (`auth.rs`)

**Precedence:** CLI flag → macOS session → XDG session → env var

1. Check `--token` CLI argument
2. Try `~/Library/Application Support/Granola/supabase.json` (macOS legacy)
3. Try `$XDG_CONFIG_HOME/granola/supabase.json` (fallback: `~/.config/granola/supabase.json`)
4. Check `BEARER_TOKEN` env var
5. Fail with exit code 2 if none found

**Session file parsing:** Parse `workos_tokens` (stringified JSON) → extract `access_token`.

### Sync Flow

1. **Auth resolution** → Get bearer token
2. **API client setup** → Blocking reqwest with auth headers, 100-300ms random throttle after each POST
3. **List documents** → POST `/v2/get-documents`
4. **Per-document processing** (with progress bar):
   - Fetch metadata via `/v1/get-document-metadata`
   - Compute filename: `{YYYY-MM-DD}_{slug}` from `created_at` + slugified title
   - Check existing MD file, parse frontmatter if present
   - **Update decision:**
     - If `doc_id` matches and `remote_updated_at <= frontmatter.remote_updated_at` → skip
     - If `doc_id` differs → append `-2`, `-3`, etc. to filename
   - Fetch transcript via `/v1/get-document-transcript`
   - Convert to Markdown (frontmatter + body)
   - Atomic write: JSON to `raw/`, MD to `transcripts/` with `0o600` perms
5. **Progress output** → Minimal console with `indicatif` (multi-progress: outer for total, inner spinner per doc)

### Storage Layout (XDG)

Base: `$XDG_DATA_HOME/muesli/` (fallback: `~/.local/share/muesli/`)

```
$DATA/
  raw/                 # {YYYY-MM-DD}_{slug}.json
  transcripts/         # {YYYY-MM-DD}_{slug}.md
  summaries/           # {YYYY-MM-DD}_{slug}.md
  index/
    tantivy/           # Text index
    embeddings/
      vectors.f32      # Contiguous float32 embeddings
      mapping.jsonl    # {doc_id, path, offset} per line
      hnsw.index       # ANN structure
  models/              # ONNX + tokenizer for e5-small-v2
  tmp/                 # Atomic write staging
```

**Permissions:** Files `0o600`, directories `0o700`.

---

## 4. Markdown Conversion (`convert.rs`)

### Supported Transcript Formats

**Format A (segments):**
```json
{
  "segments": [
    {"speaker": "Alice", "start": 12.34, "end": 18.90, "text": "..."}
  ]
}
```

**Format B (monologues):**
```json
{
  "monologues": [
    {"speaker": "Bob", "start": "00:05:10", "blocks": [{"text": "..."}]}
  ]
}
```

### Conversion Process

1. Detect format (check for `segments` vs `monologues` keys)
2. Normalize to flat list: `(speaker, timestamp, text)`
3. Parse timestamps: float seconds OR `HH:MM:SS.sss` → output `HH:MM:SS` (truncate subseconds)
4. Generate Markdown:
   - YAML frontmatter block (using `Frontmatter` struct)
   - `# {title or "Untitled Meeting"}`
   - Metadata line: `_Date: YYYY-MM-DD · Duration: XXm · Participants: Alice, Bob_`
   - Body: `**{Speaker} (HH:MM:SS):** {text}` per line
   - Optional sections: `## Agenda`, `## Action Items`, `## Decisions`, `## Links` (if present)

### Robustness

- Missing speaker → `"Speaker"`
- Missing timestamp → omit `(HH:MM:SS)`
- Missing title → `"Untitled Meeting"`
- Empty transcript → `_No transcript content available._`

### Testing

Use `insta` for snapshot testing:
- Both formats (segments, monologues)
- Edge cases (missing fields, unparseable timestamps)
- Frontmatter round-trip

---

## 5. Text Search (Milestone 2)

### Tantivy Schema (`index/text.rs`)

**Fields:**
- `doc_id` (STRING, STORED) - Granola document ID (primary key)
- `title` (TEXT) - Analyzed for search
- `date` (STRING, STORED) - ISO date for sorting
- `body` (TEXT) - Full markdown content (excluding frontmatter)
- `path` (STRING, STORED) - Absolute path to `.md`

**Index location:** `$DATA/index/tantivy/`

### Indexing Lifecycle

- During `sync`: After writing each MD, call `index::index_markdown()`
- Use `doc_id` as primary key: delete old + insert new
- Commit in batches for performance

### Search Command

```bash
muesli search "OKRs onboarding"
```

**Flow:**
1. Parse query
2. Search `title` + `body` fields with BM25
3. Return top N (default 10)
4. Output: `{rank}. {title} ({date})  {path}`

**Feature gate:** `#[cfg(feature = "index")]` - CLI only shows `search` when enabled.

### Testing

- Small test corpus
- Verify BM25 ranking
- Partial matches, multi-term queries

---

## 6. Local Embeddings Pipeline (Milestone 3)

### Model & Infrastructure (`index/embed.rs`)

**Model:** E5-small-v2 (384 dimensions)
**Location:** `$DATA/models/e5-small-v2/`

**Embedding workflow:**
1. Preprocess: prefix docs with `"passage: "`, queries with `"query: "` (E5 convention)
2. Tokenize with HF `tokenizers` (BPE)
3. ONNX inference via `ort` (CPU session, reused)
4. L2 normalize output vectors
5. Parallel processing with `rayon` (configurable via `--threads`)

### Storage

- `vectors.f32` - Contiguous float32 array
- `mapping.jsonl` - Per-doc metadata: `{"doc_id", "path", "offset"}`
- `hnsw.index` - ANN index (`hnsw_rs` with cosine metric)

### Hybrid Ranking (`index/hybrid.rs`)

1. Get top-K from BM25 (e.g., 200)
2. Get top-K from ANN cosine (e.g., 200)
3. Normalize scores to [0, 1] per list
4. Blend: `score = α * bm25_norm + (1-α) * cosine_norm` (α=0.5 default)
5. Return top-N (default 10)

**Feature gate:** `embeddings-local` (implies `index`)
**Default:** Disabled (ONNX adds binary size)

### Testing

- Cosine similarity sanity checks (semantically similar docs score high)
- Verify embedding dimensions (384)
- Test hybrid ranking blends results correctly

---

## 7. OpenAI Summarization (Milestone 4)

### Keychain Integration (`summarize.rs`)

**Storage:** macOS Keychain via `keyring` crate
- Service: `"muesli"`
- Account: `"openai_api_key"`

**First-run flow:**
1. Check keychain for existing key
2. If missing, prompt: "No OpenAI API key found in Keychain. Paste one to store? (y/N)"
3. If yes → store securely; if no → exit code 7
4. Also check `OPENAI_API_KEY` env for one-off runs (doesn't auto-store unless `--store-key`)

### Summarization Workflow

```bash
muesli summarize <doc_id>
```

1. Load transcript MD (fetch first if missing)
2. Extract content (strip frontmatter)
3. Chunk if needed (< 12k tokens per request)
4. Build prompt: title, date, participants, transcript text
5. Call OpenAI (default: `gpt-4o-mini`, overridable via `--openai-model`)
6. Parse response
7. Write to `summaries/{YYYY-MM-DD}_{slug}.md`

### Prompt Template

Request from OpenAI:
- Executive summary (3-5 bullets)
- Key decisions made
- Action items (with owner + due date if mentioned)
- Risks/blockers identified

### Output Format

```markdown
# Summary: {title} ({date})

## Executive Summary
- ...

## Decisions
- ...

## Action Items
- [ ] Owner — Task (Due: YYYY-MM-DD)

## Risks & Blockers
- ...
```

**Feature gate:** `summaries`

### Testing

- Mock OpenAI responses
- Verify prompt construction
- Test keychain error handling (document manual testing; can't fully test in CI)

---

## 8. Testing Strategy & CI/CD (Milestone 5)

### Unit Tests (TDD Throughout)

**By module:**
- `auth.rs`: Session file parsing (various shapes), precedence order with mocked env
- `convert.rs`: Snapshot tests (`insta`) for both formats, edge cases
- `storage.rs`: XDG resolution, collision detection (`-2` appending), frontmatter round-trip, atomic writes
- `util.rs`: Slug generation, timestamp normalization
- `model.rs`: Deserialize tolerance (extra fields)

### Integration Tests

**Tools:** `wiremock` or `httpmock` for API mocking

**Scenarios:**
- Full `sync` flow: list → metadata → transcript → files written
- Update detection: existing file + older `remote_updated_at` → refresh
- Error scenarios: 401/403/429/5xx → fail fast with proper exit codes

### Feature-Specific Tests

**Search/embeddings:**
- Small test corpus
- BM25 ranking verification
- Embedding cosine similarity (semantically similar queries)
- Hybrid ranking score blending

### CI/CD (GitHub Actions)

**Build matrix:**
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

**Workflow steps:**
1. Run tests with `--all-features` and individual feature combinations
2. Lint: `cargo clippy --all-features -- -D warnings`
3. Format: `cargo fmt --check`
4. Build release binaries
5. Upload with SHA256 checksums
6. Publish to crates.io on tagged releases

**Platform:** macOS-only (per spec)

---

## 9. Dependencies

### Core (M1)
- `clap` (derive) - CLI
- `reqwest` (blocking) - HTTP
- `serde`, `serde_json` - Serialization
- `serde_yaml` - Frontmatter
- `chrono` - Timestamps
- `directories` - XDG paths
- `slug` - Filename slugging
- `indicatif` - Progress bars
- `rand` - Throttle randomization
- `thiserror` - Error handling

### Optional Features
- **`index`**: `tantivy`
- **`embeddings-local`**: `ort`, `tokenizers`, `rayon`, `hnsw_rs`
- **`summaries`**: `keyring`, OpenAI client (likely `async-openai` or direct reqwest)

### Testing
- `wiremock` or `httpmock` - API mocking
- `insta` - Snapshot testing
- `assert_fs` - Filesystem fixtures

---

## 10. Milestone Deliverables

### M1: Core Sync
- [ ] Auth resolution (session file + env + CLI)
- [ ] API client (3 endpoints, throttling, fail-fast)
- [ ] Storage (XDG, atomic writes, permissions)
- [ ] Markdown conversion (both formats, frontmatter)
- [ ] CLI commands: `sync`, `list`, `fetch`
- [ ] Unit + integration tests
- [ ] All tests passing

### M2: Text Search
- [ ] Tantivy schema + indexing
- [ ] `search` command (BM25 only)
- [ ] Feature flag: `index`
- [ ] Tests

### M3: Local Embeddings
- [ ] ONNX e5-small-v2 inference
- [ ] Vector storage + ANN index
- [ ] Hybrid ranking (BM25 + cosine)
- [ ] CLI flag: `--enable-embeddings`, `--threads`
- [ ] Feature flag: `embeddings-local`
- [ ] Tests

### M4: Summaries
- [ ] Keychain integration
- [ ] OpenAI client + prompt engineering
- [ ] `summarize` command
- [ ] Feature flag: `summaries`
- [ ] Tests (with mocked responses)

### M5: Polish & Release
- [ ] GitHub Actions (macOS builds)
- [ ] Release binaries with SHA256
- [ ] crates.io publishing
- [ ] README & documentation
- [ ] End-to-end validation

---

## 11. TDD Workflow

For each module/feature:
1. **RED:** Write failing test defining desired behavior
2. **GREEN:** Write minimal code to pass test
3. **REFACTOR:** Clean up while keeping tests green
4. Repeat for next behavior

**Test-first order:**
1. Core types (model deserialization)
2. Utilities (slug, timestamp normalization)
3. Auth resolution
4. Storage (paths, atomic writes)
5. API client (with mocked endpoints)
6. Conversion (snapshot tests)
7. CLI integration (end-to-end with mocks)
8. Search indexing
9. Embeddings pipeline
10. Summarization

---

## Success Criteria

- ✅ All 5 milestones delivered
- ✅ Pure TDD throughout (no code without tests)
- ✅ All tests passing in CI
- ✅ Successful build on macOS (x86_64 + arm64)
- ✅ Published to crates.io
- ✅ Clean `cargo clippy` with no warnings
- ✅ Formatted with `cargo fmt`
- ✅ Feature flags work correctly (core, index, embeddings, summaries, full)

---

**End of design document**

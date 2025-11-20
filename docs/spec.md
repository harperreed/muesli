# Muesli — Rust Meetings/Transcripts Sync Client

**Scope:** Robust, macOS‑focused Rust rewrite of the Granola meeting/transcripts puller with structured Markdown conversion, optional OpenAI summaries, and optional local full‑text + embedding search.

---

## 1) Goals & Non‑Goals

### Goals

* **Rust lib + CLI** (`muesli` crate and `muesli` binary).
* **Default command: `sync`** – enumerate meetings/notes and download:

  * Raw transcripts **as JSON**.
  * Processed transcripts **as Markdown**, enhanced formatting.
* **Auth from Granola’s session file**, with CLI/env fallback.
* **XDG‑compliant data storage** for transcripts, raw, summaries, models, and indexes.
* **Fail‑fast** error handling.
* **Polite HTTP** (blocking; throttle 100–300 ms between API calls).
* **Update detection**: re‑download when remote `updated_at` is newer.
* **Structured Markdown output** (speaker + timestamp inline; optional YAML frontmatter).
* **Minimal UX with progress bars** (`indicatif`), macOS‑only builds.
* **Optional OpenAI summaries** (key stored in macOS Keychain).
* **Optional local search** (Tantivy + local embeddings using `e5-small-v2` ONNX).
* **Extensible API layer** for future Granola endpoints.

### Non‑Goals

* No daemon/background service.
* No remote server or UI; CLI only.
* No persistent backups (we can rebuild).
* No Windows/Linux official builds (macOS only for releases).
* No network concurrency (single-threaded HTTP).
* No heavy config system (CLI > env > file).

---

## 2) Terminology

* **Document / Meeting**: a Granola entity listed by `/v2/get-documents` (aka “notes”).
* **Raw transcript**: JSON returned by `/v1/get-document-transcript`.
* **Processed transcript**: Markdown produced by the converter.
* **Session file**: Granola’s local `supabase.json` containing `workos_tokens`.

---

## 3) Architecture Overview

### Crate layout

```
muesli/
├── Cargo.toml
├── src/
│   ├── main.rs            # CLI entrypoint
│   ├── lib.rs             # re-exports
│   ├── api.rs             # Granola API client (blocking reqwest)
│   ├── auth.rs            # token discovery (session file / env / CLI)
│   ├── storage.rs         # XDG paths, file I/O, safe writes
│   ├── model.rs           # serde types for API payloads
│   ├── convert.rs         # raw JSON → structured Markdown (+ frontmatter)
│   ├── cli.rs             # clap subcommands & flags
│   ├── summarize.rs       # optional OpenAI summaries + Keychain integration
│   ├── index/
│   │   ├── mod.rs         # search public API
│   │   ├── text.rs        # Tantivy indexing
│   │   ├── embed.rs       # local ONNX e5-small-v2, rayon, ANN search
│   │   └── hybrid.rs      # combined ranking (BM25 + cosine)
│   └── util.rs            # slugging, throttle, time helpers, errors
└── README.md
```

### Data flow: `sync`

1. Load bearer token (CLI flag > session file(s) > env).
2. List docs via `/v2/get-documents`.
3. For each doc:

   * Fetch metadata via `/v1/get-document-metadata`.
   * Decide local filenames (`{DATE}_{slug}.md` and `.json`) and paths.
   * If file exists: read embedded frontmatter (doc_id & remote_updated_at); skip or refresh based on comparison with remote.
   * Download raw transcript JSON.
   * Convert to Markdown with structured formatting and YAML frontmatter.
   * Optionally: generate/update search index & embeddings.
4. Progress bars around steps; fail fast on first fatal error.

---

## 4) CLI Specification

### Binary name

* `muesli`

### Subcommands (default = `sync`)

* `muesli` → `sync`
* `muesli sync`
  Sync all accessible docs; download/update raw + MD.
* `muesli list`
  Print a concise list of doc IDs + titles + dates.
* `muesli fetch <id>`
  Fetch one doc by ID; write raw + MD.
* `muesli search <query>`
  Local search (hybrid embedding + full‑text if enabled; else text only).
* `muesli summarize <id>`
  Generate a summary (OpenAI), write Markdown.

### Global flags

* `--token <STRING>` — Bearer token (overrides all).
* `--api-base <URL>` — override API base (default `https://api.granola.ai`).
* `--throttle-ms <MIN>:<MAX>` — override random sleep window (default `100:300`).
* `--data-dir <PATH>` — override `$XDG_DATA_HOME/muesli`.
* `--raw-dir <PATH>` — override `$DATA/raw/`.
* `--transcripts-dir <PATH>` — override `$DATA/transcripts/`.
* `--summaries-dir <PATH>` — override `$DATA/summaries/`.
* `--no-embed` — disable embedding pipeline (indexing still text).
* `--enable-embeddings` — enable local embedding pipeline (default off).
* `--threads <N>` — embedding threads (if embeddings enabled; default = all cores).
* `--no-throttle` — disable inter‑request sleep (not recommended).
* `--openai-model <NAME>` — summarization model (defaults below).
* `--version`, `--help`

### UX

* Minimal console noise.
* `indicatif` progress bars during sync/fetch.
* Exit non‑zero on first fatal error.

---

## 5) Configuration & Precedence

**Precedence:** CLI flags > session file > env vars.

* **Session file discovery (macOS first):**

  1. `~/Library/Application Support/Granola/supabase.json` (macOS legacy path)
  2. `$XDG_CONFIG_HOME/granola/supabase.json`
     fallback: `~/.config/granola/supabase.json`
* **Env:** `BEARER_TOKEN`
* **OpenAI key (summaries/remote embeddings):** retrieved from **macOS Keychain** (see §9). CLI `--token` does **not** affect OpenAI.

---

## 6) Storage Layout (XDG)

Base: `$XDG_DATA_HOME/muesli/` (fallback `~/.local/share/muesli/`)

```
$DATA/
  raw/                 # raw transcripts (*.json)
  transcripts/         # processed markdown (*.md)
  summaries/           # optional summaries (*.md)
  index/               # text + vector indexes
  models/              # ONNX & tokenizer files for embeddings
  tmp/                 # temp files for atomic writes
```

**Filenames** (local time):

* JSON:  `raw/{YYYY-MM-DD}_{slug}.json`
* MD:    `transcripts/{YYYY-MM-DD}_{slug}.md`
* Summary MD: `summaries/{YYYY-MM-DD}_{slug}.md`

**Collision policy:**
If `{DATE}_{slug}.md` exists *but* belongs to a **different** `doc_id` (checked via frontmatter), append `-2`, `-3`, … until unique. If it belongs to the **same** `doc_id`, treat as update candidate.

**Permissions:**
Create files with `0o600` (owner read/write). Directories `0o700`.

---

## 7) HTTP/API Layer

### Client

* **Blocking** `reqwest` client.
* Default headers:

  * `Authorization: Bearer <token>`
  * `Accept: application/json`
  * `Content-Type: application/json`
  * Minimal UA: `User-Agent: muesli/1.0 (Rust)`
* **Throttle**: sleep a random 100–300 ms after **each** POST.
* **Fail fast:** any request failure aborts the run with a clear error.

### Endpoints (POST)

* `/v2/get-documents`
  **Req**: `{}`
  **Resp**: `{ "docs": [DocumentSummary, ...] }`
* `/v1/get-document-metadata`
  **Req**: `{ "document_id": "<id>" }`
  **Resp**: `DocumentMetadata`
* `/v1/get-document-transcript`
  **Req**: `{ "document_id": "<id>" }`
  **Resp**: `RawTranscript`
* (Optional) `/v1/get-panel-templates` (meeting types)
* (Optional) `/v1/update-document` (not required for sync)
* (Optional) `/v1/update-document-panel` (not required for sync)
* (Optional) `/v2/get-document-lists`

> **Schema tolerance:** We don’t know Granola’s full schemas. Use `serde` with:
>
> * `#[serde(default)]` for optional fields
> * **No** `deny_unknown_fields`
> * Timestamp fields parsed via `chrono` with flexible formats.

---

## 8) Data Models (Rust + JSON)

### `DocumentSummary`

```jsonc
{
  "id": "string",                     // required
  "title": "string|null",             // may be missing/empty
  "created_at": "2025-10-28T15:04:05Z", // ISO8601
  "updated_at": "2025-10-29T01:23:45Z", // ISO8601 (if provided)
  "...": "additional fields ignored"
}
```

### `DocumentMetadata`

```jsonc
{
  "id": "string",
  "title": "string|null",
  "created_at": "ISO8601",
  "updated_at": "ISO8601|null",
  "participants": ["Alice", "Bob"],   // optional
  "duration_seconds": 3600,           // optional
  "labels": ["Sales", "Weekly"],      // optional
  "...": "additional fields ignored"
}
```

### `RawTranscript` (flexible)

Support **either** of these common shapes:

**A) Segment list**

```jsonc
{
  "segments": [
    {
      "speaker": "Alice",
      "start": 12.34,                 // seconds (float) or "00:00:12.340"
      "end": 18.90,                   // seconds (float) or timestamp string
      "text": "Hello everyone…"
    }
  ]
}
```

**B) Monologues**

```jsonc
{
  "monologues": [
    {
      "speaker": "Bob",
      "start": "00:05:10",
      "blocks": [
        {"text": "First thought."},
        {"text": "Second thought."}
      ]
    }
  ]
}
```

**Converter** MUST gracefully accept either and normalize.

---

## 9) Auth & Secrets

### Bearer token

* **Resolution order:** `--token` → session file(s) → `BEARER_TOKEN`.
* Session file parsing: JSON with `workos_tokens` stringified JSON; extract `access_token`.

### OpenAI key (only if summaries/remote embeddings are used)

* Stored/retrieved via **macOS Keychain** (crate: `keyring`).

  * Service: `muesli`
  * Account: `openai_api_key`
* If no key is set:

  * `muesli summarize <id>` asks once to **store** key securely.
  * Alternatively accept `OPENAI_API_KEY` env for one‑off runs; **do not store** unless user passes `--store-key`.

---

## 10) Conversion to Structured Markdown

### Style (you chose **inline conversational**)

```
# {title or "Untitled Meeting"}
_Date: 2025-10-28 · Duration: 53m · Participants: Alice, Bob_

**Alice (00:12:34):** Some dialogue text that may span multiple lines…
**Bob (00:12:40):** Reply here.
```

### YAML frontmatter (at top of `.md`)

```yaml
---
doc_id: "<granola_doc_id>"
source: "granola"
created_at: "2025-10-28T15:04:05Z"
remote_updated_at: "2025-10-29T01:23:45Z"
title: "Quarterly Planning"
participants: ["Alice", "Bob"]
duration_seconds: 3170
labels: ["Planning", "Q4"]
generator: "muesli 1.0"
---
```

* **Purpose:** enables update checks (compare `remote_updated_at`) and stable doc mapping despite filename not containing ID.
* If `updated_at` is missing in metadata, fall back to `created_at` for comparison.

### Content sections

* `# Title` (or “Untitled Meeting”)
* Metadata line under title (date/duration/participants if available).
* **Transcript body**: one line per utterance:

  * `**{Speaker} (HH:MM:SS):** {text}`
* Optional sections (include if present in raw/metadata):

  * `## Agenda`
  * `## Action Items`
  * `## Decisions`
  * `## Links`

### Timestamp normalization

* Accept floats (seconds) or time strings; output `HH:MM:SS` (truncate subseconds).

### Empty/missing data handling

* If `title` missing → `Untitled`.
* If `speaker` missing → `Speaker`.
* If `start` missing → omit timestamp.

---

## 11) Update & Dedupe Rules

* Determine expected paths from `{DATE}_{slug}`.
* If the MD path exists:

  * Read frontmatter:

    * If `doc_id` matches:

      * If remote `updated_at` > frontmatter `remote_updated_at` → **refresh** raw + MD.
      * Else skip.
    * If `doc_id` differs → append `-2`, `-3`… to filename for this new doc.
* If MD missing but JSON exists (or vice versa), rebuild the missing artifact.

---

## 12) Optional Summaries (OpenAI)

* Command: `muesli summarize <id>` or `muesli sync --summarize` (if you decide to add a flag later).
* Default model (suggestion): `gpt-4o-mini` or `gpt-4.1-mini`. (Configurable via `--openai-model`.)
* **Prompt**: include meeting title, date, participants, and transcript text (chunked).
* **Output**: Markdown at `summaries/{DATE}_{slug}.md` with:

  * Executive summary (bulleted)
  * Key decisions
  * Action items (owner + due date if obvious)
  * Risks / blockers

> Summarization is optional and **out of the hot path** (core `sync` does not require OpenAI).

---

## 13) Local Search (Optional)

### Components

* **Text index**: Tantivy (BM25), fields:

  * `doc_id` (string, stored)
  * `title` (text, analyzed)
  * `date` (i64 epoch day or RFC3339 string, stored)
  * `body` (text, analyzed) – entire Markdown content
  * `path` (string, stored)
* **Embeddings** (local, default model **`e5-small-v2`**, 384 dims):

  * ONNX inference via `ort` + HF `tokenizers`.
  * Normalize vectors (L2) and store on disk.
  * ANN index (recommend `hnsw_rs` or similar) for cosine similarity.
* **Hybrid ranking**: weighted sum of normalized BM25 and cosine (default α=0.5; may become a flag later).

### Persistence

```
$DATA/index/
  tantivy/               # text index
  embeddings/
    vectors.f32          # contiguous 384-d vectors
    mapping.jsonl        # line-delimited {doc_id, path, offset}
    hnsw.index           # ANN structure
```

### Lifecycle

* **Disabled by default** (no embeddings).
* When `--enable-embeddings` is passed:

  * On **new or updated** MD, (re)embed those docs in parallel (rayon).
  * Update Tantivy doc and ANN index incrementally.
* `muesli search "<query>"`:

  * If embeddings enabled: compute query embedding, get top‑K by ANN, merge with BM25 top‑K → hybrid top‑N.
  * Else: BM25 only.
* Output: list `rank. title  (YYYY-MM-DD)  path` (doc_id if `--ids` added later).

---

## 14) Rate Limiting & Throttle

* After **every POST**: sleep `rand::thread_rng().gen_range(min..=max)` where default `min=100ms`, `max=300ms`.
* `--no-throttle` to disable (dev only).
* **No automatic retry/backoff** (your choice: fail fast).
* If API returns `429` or 5xx → treat as fatal; print helpful context (endpoint, HTTP status, short body snippet).

---

## 15) Error Handling & Exit Codes

**Fail fast** on first fatal error, with clear message.
Recommended exit codes:

* `0` — success
* `1` — unknown/general error
* `2` — auth/token error (missing/invalid)
* `3` — network error (DNS/TLS/timeout)
* `4` — API error (HTTP non‑2xx)
* `5` — parse error (JSON/serde)
* `6` — filesystem error (permissions/path)
* `7` — summarization error (OpenAI/keychain)
* `8` — indexing/embedding error

Error output format:

```
muesli: [E4] API error on /v1/get-document-transcript (HTTP 403): Forbidden
```

---

## 16) Dependencies (stable Rust; moderate footprint)

* **CLI**: `clap` (derive)
* **HTTP**: `reqwest` (blocking), `serde`, `serde_json`
* **Time**: `chrono`
* **Paths**: `directories` (XDG); `path_abs` optional
* **Slug**: `slug` (or `slugify`)
* **Progress**: `indicatif`
* **Random**: `rand`
* **YAML**: `serde_yaml` (frontmatter parsing/emit)
* **Keychain**: `keyring` (macOS)
* **Search (optional features)**:

  * `tantivy`
  * `rayon`
  * `tokenizers` (HF)
  * `ort` (ONNX Runtime, CPU)
  * `hnsw_rs` (or similar ANN)
* **Testing**: `httpmock` or `wiremock-rs`, `assert_fs`, `insta` (snapshots)

**Feature flags** (suggested):

* `summaries` (enables Keychain & OpenAI client)
* `index` (enables Tantivy)
* `embeddings-local` (enables ONNX + tokenizers + rayon)
* `sqlite` (future opt-in manifest if you choose to add it later)

Default features: none (core sync only).

---

## 17) Build & Distribution

* **macOS only** releases (x86_64, arm64).
* GitHub Actions:

  * Build matrices for `aarch64-apple-darwin` and `x86_64-apple-darwin`.
  * Upload `muesli` binaries to Releases with SHA256.
* **crates.io** publishing for `cargo install muesli`.

---

## 18) Security & Privacy

* Never log bearer tokens or transcript content.
* Create files with restrictive permissions (`0o600`).
* Use Keychain for OpenAI secret; never print the key.
* Consider redacting PII in summaries (optional later).

---

## 19) Testing Strategy

**Hybrid** (unit + integration w/ mocks):

* **Unit tests**

  * `convert.rs`: segment normalization, timestamp formatting, frontmatter I/O.
  * `storage.rs`: path resolution, collision handling.
  * `auth.rs`: session file parsing variants.
* **Integration tests** (mock network)

  * `sync` happy path: list → metadata → transcript.
  * Update path: updated remote → refresh local.
  * Error path: 401/403/429/5xx.
* **Index tests** (if features on)

  * BM25 only search returns expected docs.
  * Embedding cosine similarity sanity tests.
* **Golden files** for Markdown outputs using `insta`.

**No live API** required for CI (can add `GRANOLA_TEST_TOKEN` later for opt‑in e2e).

---

## 20) Implementation Details

### 20.1 Auth resolution

```rust
// Pseudocode
fn resolve_token(cli_tok: Option<String>) -> Result<String> {
    if let Some(t) = cli_tok { return Ok(t); }
    if let Some(t) = from_legacy_macos_session()? { return Ok(t); }
    if let Some(t) = from_xdg_session()? { return Ok(t); }
    if let Ok(t) = env::var("BEARER_TOKEN") { return Ok(t); }
    bail!(Error::AuthMissing);
}
```

**Session file parsing:**
`supabase.json` → read `workos_tokens` (JSON string) → parse → `access_token`.

### 20.2 Filename + slug

* `slug = slugify(title.or("untitled"))`
* Date basis: **local time** from `created_at` (fallback to today).
* Pattern: `{YYYY-MM-DD}_{slug}`.
* Collision: if frontmatter doc_id mismatch → append `-2`, `-3`, …

### 20.3 Update decision

* Read frontmatter (`doc_id`, `remote_updated_at`).
* Compute `remote_ts = metadata.updated_at.unwrap_or(metadata.created_at)`.
* If `frontmatter.remote_updated_at` is None → refresh.
* Else if `remote_ts > frontmatter.remote_updated_at` → refresh.
* Else skip.

### 20.4 Converter robustness

* Accept segments or monologues; flatten to `(speaker, start, text)` lines.
* Normalize timestamps:

  * If numeric → format HH:MM:SS (floor).
  * If `HH:MM:SS.sss` → truncate subseconds.
* Line wrapping: preserve original; don’t auto‑wrap.

### 20.5 Atomic writes

* Write to `$DATA/tmp/<random>.part` then `rename()` to final path.
* Set `0o600` on files.
* Ensure directory exists (`create_dir_all`).

### 20.6 Progress bars

* Outer bar: total docs.
* Inner (per doc) spinner for network + convert + write.
* Minimal text; on success: `synced N docs`.

---

## 21) Search Details (if enabled)

### 21.1 Tantivy schema

* Analyzer: default English (or simple) tokenizer.
* Fields:

  * `doc_id` (STORED, STRING)
  * `title` (TEXT)
  * `date` (STRING or i64 days)
  * `body` (TEXT)
  * `path` (STORED, STRING)
* Index one doc per markdown file. Body includes entire visible text (frontmatter excluded).

### 21.2 Embedding pipeline

* Model: **`e5-small-v2`** (384‑dim).
  Download/cached under `$DATA/models/e5-small-v2/`.
* Tokenization: HF `tokenizers` (BPE), normalize input:

  * Prefix with instruction: `"query: <text>"` for queries; `"passage: <text>"` for documents (E5 convention).
* Inference: `ort` CPU session (create once).
* Concurrency: **rayon**; batch over docs; cap threads by `--threads`.
* Storage: contiguous `f32` vectors; mapping JSONL line: `{ "doc_id": "...", "path": "...", "offset": <index> }`.
* ANN: `hnsw_rs` with cosine metric; rebuild incrementally for changed docs.

### 21.3 Hybrid search

* BM25 top‑K (e.g., 200).
* ANN cosine top‑K (e.g., 200).
* Normalize scores to [0,1] per list; blend: `score = α * bm25 + (1-α) * cosine`, α=0.5 default.
* Return top‑N (default 10). Print:

  ```
  1. Quarterly Planning (2025-10-28)  /Users/.../transcripts/2025-10-28_quarterly-planning.md
  ```

---

## 22) OpenAI Summarization (optional feature)

* Retrieve key from Keychain (`keyring`).
* If no key exists:

  * Prompt: “No OpenAI API key found in Keychain. Paste one to store? (y/N)”
  * If yes: store securely; else abort with code 7.
* Chunk transcript if long (aim < 12k tokens per request).
* Prompt template includes:

  * Title, date, participants
  * Instructions: concise executive summary, decisions, action items (owner/due), risks
* Output example:

  ```
  # Summary: Quarterly Planning (2025-10-28)

  ## Executive Summary
  - ...

  ## Decisions
  - ...

  ## Action Items
  - [ ] Owner — Task (Due: YYYY-MM-DD)
  ```

---

## 23) Public Library API (for reuse)

```rust
pub mod api {
  pub struct Client { /* base_url, token, reqwest::blocking::Client */ }
  impl Client {
    pub fn new(token: String, base_url: Url) -> Result<Self>;
    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>>;
    pub fn get_metadata(&self, id: &str) -> Result<DocumentMetadata>;
    pub fn get_transcript(&self, id: &str) -> Result<RawTranscript>;
  }
}

pub mod convert {
  pub fn to_markdown(raw: &RawTranscript, meta: &DocumentMetadata) -> MarkdownOutput;
  pub struct MarkdownOutput {
    pub frontmatter_yaml: String,
    pub body: String,
  }
}

pub mod storage {
  pub struct Paths { /* resolved dirs */ }
  pub fn resolve_paths(overrides: Opts) -> Result<Paths>;
  pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()>;
  pub fn read_frontmatter(md_path: &Path) -> Result<Option<Frontmatter>>;
}

pub mod index {
  pub fn index_markdown(path: &Path, doc_id: &str, title: &str, date: NaiveDate, body: &str) -> Result<()>;
  pub fn search(query: &str, top_n: usize) -> Result<Vec<SearchHit>>;
}
```

---

## 24) Command Workflows

### `muesli sync`

* Resolve token.
* Build API client.
* List docs.
* For each doc:

  * Fetch metadata.
  * Resolve filenames/paths.
  * Determine update action via frontmatter.
  * Fetch transcript JSON.
  * Convert → Markdown.
  * Write JSON, write MD (atomic).
  * If embeddings enabled: (re)index this doc (rayon queue).
* Finish: print `synced X docs (Y new, Z updated, W skipped)` **(optional minimal one‑liner; or omit per your “minimal output” choice).**

### `muesli fetch <id>`

* Same as per-doc in `sync`.

### `muesli list`

* Print: `<id>\t<YYYY-MM-DD>\t<title>`

### `muesli search "<query>"`

* If embeddings enabled: hybrid; else BM25 only.
* Print ranked results (path per line).

### `muesli summarize <id>`

* Load MD; if missing, fetch first.
* Read/OpenAI key from Keychain (prompt to set if missing).
* Generate and write `summaries/{DATE}_{slug}.md`.

---

## 25) Migration Notes (from Python)

* Python stored in `./transcripts/` and `./raw_transcripts/`.
  Rust stores under XDG data dir. Consider copying existing files into `$XDG_DATA_HOME/muesli/` if you want continuity.
* Filenames differ (short, local date, no ID).
  Use frontmatter to maintain doc mapping and update checks.
* JSON raw is still pretty‑printed for diffability.

---

## 26) Edge Cases & Handling

* **Missing `title`**: slug = `untitled`.
* **Empty transcript**: write JSON; MD body includes `_No transcript content available._`
* **No `updated_at`**: treat as `created_at` for freshness.
* **Weird timestamps**: if unparsable, omit `(HH:MM:SS)`.
* **Slug collisions** across different docs on same `DATE`: resolve with `-2`, `-3`, … (based on frontmatter doc_id mismatch).
* **429/5xx**: fatal per run; recommend re‑run later.
* **OpenAI quota errors**: exit with code 7; do not retry automatically.
* **Embedding model download** fails: disable embeddings for this run, print clear message, exit with code 8.

---

## 27) Example Console Sessions

**Sync all:**

```
$ muesli
Syncing…
[####------] 40%  12/30 docs
synced 30 docs
```

**Fetch one:**

```
$ muesli fetch 1a2b3c4d
wrote raw/2025-10-28_quarterly-planning.json
wrote transcripts/2025-10-28_quarterly-planning.md
```

**Search (text only):**

```
$ muesli search "OKRs onboarding"
1. Onboarding OKRs (2025-09-10)  /…/transcripts/2025-09-10_onboarding-okrs.md
2. Q4 OKR Review (2025-09-15)    /…/transcripts/2025-09-15_q4-okr-review.md
```

---

## 28) Open Items (assumptions made)

* `RawTranscript` exact schema may vary; we designed a tolerant converter.
* Optional sections (Agenda/Decisions/Actions) depend on raw payload presence—implemented opportunistically.
* If you later want `sync --summarize` or **remote embeddings** (OpenAI), both are straightforward to add behind flags using the same Keychain key.

---

## 29) Quick Task Breakdown

**Milestone 1 – Core sync (no summaries, no index)**

* [ ] `auth.rs` (session + env + CLI)
* [ ] `api.rs` (3 endpoints, blocking, throttle)
* [ ] `model.rs` (serde types)
* [ ] `storage.rs` (XDG dirs, atomic write, perms)
* [ ] `convert.rs` (frontmatter + inline format)
* [ ] `cli.rs` + `main.rs` (sync/list/fetch)
* [ ] Tests (unit + integration mocks)

**Milestone 2 – Search (text only)**

* [ ] `index/text.rs` (Tantivy)
* [ ] `search` subcommand
* [ ] Tests

**Milestone 3 – Local embeddings**

* [ ] `index/embed.rs` (ONNX e5-small-v2 + rayon)
* [ ] `index/hybrid.rs`
* [ ] `--enable-embeddings` + `--threads`
* [ ] Tests

**Milestone 4 – Summaries**

* [ ] `summarize.rs` (Keychain + OpenAI client)
* [ ] `summarize` subcommand
* [ ] Tests

**Milestone 5 – Polishing**

* [ ] GitHub Actions (macOS).
* [ ] crates.io publishing.

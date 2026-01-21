# Muesli

**A fast, offline-first Rust CLI for syncing and searching Granola meeting transcripts**

[![CI](https://github.com/harperreed/muesli/workflows/CI/badge.svg)](https://github.com/harperreed/muesli/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Muesli syncs your [Granola](https://granola.ai) meeting transcripts to local markdown files and provides powerful search capabilities including full-text search (BM25) and semantic search (embeddings).

## Features

- 🔄 **Sync transcripts** - Download and convert to clean markdown with frontmatter
- 🔍 **Full-text search** - Fast BM25 search with Tantivy
- 🧠 **Semantic search** - Meaning-based search using e5-small-v2 embeddings
- 📝 **AI summaries** - Generate structured summaries with OpenAI
- 🚀 **Fast & offline** - All search happens locally, no API calls
- 💾 **XDG compliant** - Follows XDG Base Directory specification
- 🔒 **Secure** - API tokens in keychain (macOS) or environment variables

## Installation

### From Release Binaries

Download the latest release for your platform:

```bash
# macOS (Apple Silicon)
curl -L https://github.com/harperreed/muesli/releases/latest/download/muesli-macos-aarch64 -o muesli
chmod +x muesli
sudo mv muesli /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/harperreed/muesli/releases/latest/download/muesli-macos-x86_64 -o muesli
chmod +x muesli
sudo mv muesli /usr/local/bin/

# Linux
curl -L https://github.com/harperreed/muesli/releases/latest/download/muesli-linux-x86_64 -o muesli
chmod +x muesli
sudo mv muesli /usr/local/bin/

# Windows
# Download muesli-windows-x86_64.exe from releases page
```

### From Source

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/harperreed/muesli.git
cd muesli

# Build with all features (recommended)
cargo build --release --all-features

# Install to PATH
cargo install --path . --all-features
```

## Quick Start

```bash
# 1. Set your Granola API token
export GRANOLA_TOKEN="your-token-here"
# Or let muesli read from ~/.granola/supabase.json

# 2. Sync your transcripts
muesli sync

# 3. Search (full-text)
muesli search "quarterly planning"

# 4. Search (semantic - meaning-based)
muesli search --semantic "improving team collaboration"
```

## Usage

### Sync Transcripts

```bash
# Sync all transcripts (updates only changed documents)
muesli sync

# Force rebuild text search index without re-downloading
muesli sync --reindex
```

Synced files are stored in:
- **Transcripts:** `~/.local/share/muesli/transcripts/` (markdown)
- **Raw data:** `~/.local/share/muesli/raw/` (JSON)
- **Indexes:** `~/.local/share/muesli/index/` (search indexes)

### Search

**Full-text search** (keyword matching with BM25 ranking):
```bash
# Basic search
muesli search "machine learning"

# Limit results
muesli search "product roadmap" -n 5

# Multi-word queries
muesli search "Q1 planning meeting"
```

**Semantic search** (meaning-based with embeddings):
```bash
# Find conceptually similar documents
muesli search --semantic "team productivity improvements"

# Works with questions
muesli search --semantic "how do we handle customer feedback"

# Finds related concepts (not just exact keywords)
muesli search --semantic "innovation strategy" -n 10
```

### List Documents

```bash
# List all synced documents
muesli list
```

Output format: `<doc-id>  <date>  <title>`

### Fetch Single Document

```bash
# Download a specific document by ID
muesli fetch <doc-id>
```

### AI Summaries (Optional)

```bash
# Set OpenAI API key (macOS - stores in Keychain)
muesli set-api-key sk-...

# Or use environment variable
export OPENAI_API_KEY="sk-..."

# Generate summary for a document
muesli summarize <doc-id>
```

Summaries include:
- Key topics discussed
- Action items
- Decisions made
- Follow-up items

### Configure Summarization

```bash
# Show current configuration
muesli set-config --show

# Change the OpenAI model
muesli set-config --model gpt-4o

# Set context window size (in characters)
muesli set-config --context-window 8000

# Use a custom prompt file
muesli set-config --prompt-file /path/to/prompt.txt
```

### MCP Server

Muesli can run as a [Model Context Protocol](https://modelcontextprotocol.io/) server, allowing AI assistants like Claude to search and access your meeting transcripts.

```bash
# Start the MCP server
muesli mcp
```

Configure in your AI assistant's MCP settings to enable transcript search and retrieval.

## Feature Flags

All features are enabled by default. If you need a smaller binary, you can disable features:

| Feature | Description |
|---------|-------------|
| `index` | Full-text search (Tantivy) |
| `embeddings` | Semantic search (ONNX, e5-small-v2) |
| `summaries` | AI summaries (OpenAI) |
| `mcp` | MCP server for AI assistant integration |

### Building with Specific Features

```bash
# Default build (all features, ~21MB)
cargo build --release

# Core only (sync, list, fetch - ~5MB)
cargo build --release --no-default-features

# With only text search (~9MB)
cargo build --release --no-default-features --features index

# With semantic search (includes text search, ~17MB)
cargo build --release --no-default-features --features embeddings

# With summaries (~11MB)
cargo build --release --no-default-features --features summaries
```

## Configuration

### Authentication

Muesli looks for your Granola API token in this order:

1. `--token` flag
2. `GRANOLA_TOKEN` environment variable
3. `~/.granola/supabase.json` (Granola desktop app location)

### Data Directory

Override the default data directory:

```bash
muesli sync --data-dir /custom/path
```

### API Throttling

By default, muesli throttles API requests (500-1000ms between calls) to be respectful to the Granola API.

```bash
# Disable throttling (not recommended)
muesli sync --no-throttle

# Custom throttle range (min:max in milliseconds)
muesli sync --throttle-ms 200:400
```

## How It Works

### Sync

1. Fetches document list from Granola API
2. Checks local cache to determine which documents need updating
3. Downloads updated documents (metadata + transcript)
4. Converts to clean markdown with YAML frontmatter
5. Writes atomically to disk (crash-safe)
6. Updates search indexes (if features enabled)

### Full-Text Search (BM25)

1. Documents are indexed with Tantivy during sync
2. Search uses BM25 ranking algorithm (like Elasticsearch)
3. Searches both title and body fields
4. Results ranked by relevance

### Semantic Search (Embeddings)

1. Downloads e5-small-v2 model from HuggingFace (~133MB, cached locally)
2. Generates 384-dimensional embeddings for each document during sync
3. Stores vectors in binary format (~1.5KB per document)
4. Search uses cosine similarity for meaning-based matching
5. Finds related concepts even without keyword matches

## Development

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs))
- Granola API access

### Setup

```bash
# Clone repository
git clone https://github.com/harperreed/muesli.git
cd muesli

# Run tests
cargo test

# Run tests with all features
cargo test --all-features

# Run integration tests
cargo test --test workflow_integration --features index,embeddings

# Build debug binary
cargo build

# Run with logging
RUST_LOG=debug cargo run -- sync
```

### Project Structure

```
muesli/
├── src/
│   ├── api.rs           # Granola API client
│   ├── auth.rs          # Token resolution
│   ├── cli.rs           # Command-line interface
│   ├── convert.rs       # Transcript → Markdown
│   ├── error.rs         # Error types
│   ├── lib.rs           # Library exports
│   ├── main.rs          # Binary entry point
│   ├── model.rs         # Data structures
│   ├── storage.rs       # File I/O and paths
│   ├── sync.rs          # Sync orchestration
│   ├── util.rs          # Helpers
│   ├── index/
│   │   └── text.rs      # Tantivy full-text search
│   ├── embeddings/
│   │   ├── downloader.rs # Model download
│   │   ├── engine.rs    # ONNX embedding generation
│   │   └── vector.rs    # Vector store and search
│   └── summary.rs       # OpenAI integration
├── tests/
│   ├── api_integration.rs      # API mocking tests
│   └── workflow_integration.rs # End-to-end tests
└── docs/
    ├── IMPLEMENTATION_STATUS.md
    └── plans/
```

### Running CI Locally

```bash
# Format check
cargo fmt --all -- --check

# Clippy
cargo clippy --all-features -- -D warnings

# All tests
cargo test --all-features
```

## Performance

| Operation | Speed | Notes |
|-----------|-------|-------|
| Sync (initial) | ~536 docs in 60s | API rate limited |
| Sync (incremental) | ~1s | Only changed docs |
| Reindex | ~538 docs in 1s | From local files |
| Text search | <50ms | BM25 index scan |
| Semantic search | ~200ms | First query (model load) |
| Semantic search | <50ms | Subsequent queries |
| Embedding generation | ~100ms/doc | During sync |

## Binary Size

| Build Type | Size | Configuration |
|------------|------|---------------|
| Debug | 120MB | Default dev build |
| Release (default) | 34MB | `cargo build --release` |
| Release (optimized) | 21MB | LTO + strip (current) |

Optimizations applied:
- Link Time Optimization (LTO)
- Size-optimized (`opt-level = "z"`)
- Debug symbols stripped
- Panic abort (no unwinding)

## Troubleshooting

### "No results found" for text search

The text search index needs to be built:

```bash
muesli sync --reindex
```

### "No vector store found" for semantic search

Run sync with embeddings feature to generate vectors:

```bash
muesli sync
```

### Slow embeddings generation

This is normal on first sync. The e5-small-v2 model (~133MB) is downloaded once and cached. Subsequent syncs only generate embeddings for new documents.

### macOS keychain permission denied

Grant Terminal/iTerm2 keychain access in System Preferences → Privacy & Security.

## Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Write tests for your changes
4. Ensure all tests pass (`cargo test --all-features`)
5. Run clippy (`cargo clippy --all-features -- -D warnings`)
6. Format code (`cargo fmt --all`)
7. Commit with conventional commits
8. Open a Pull Request

## License

MIT License - see [LICENSE](LICENSE) file for details

## Acknowledgments

- Built with [Tantivy](https://github.com/quickwit-oss/tantivy) for full-text search
- Embeddings powered by [e5-small-v2](https://huggingface.co/intfloat/e5-small-v2)
- ONNX Runtime via [ort](https://github.com/pykeio/ort)
- CLI powered by [clap](https://github.com/clap-rs/clap)

## Related Projects

- [Granola](https://granola.ai) - AI notepad for meetings
- [Obsidian](https://obsidian.md) - Perfect for organizing synced markdown notes

---

**Built with ❤️ in Rust**

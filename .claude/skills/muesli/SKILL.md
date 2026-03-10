---
name: muesli
description: Search and retrieve Granola meeting transcripts using the muesli CLI. Use when the user asks about past meetings, wants to find what was discussed, look up meeting details, or reference meeting transcripts.
user_invocable: false
---

# Muesli - Granola Meeting Transcript CLI

Muesli syncs and searches Granola meeting transcripts locally. It downloads transcripts from the Granola API, stores them as markdown files with a local DuckDB database, and provides search, filtering, and display commands.

## Data Location

- Transcripts: `~/.local/share/muesli/transcripts/`
- Summaries: `~/.local/share/muesli/summaries/`
- Index: `~/.local/share/muesli/index/`
- Database: `~/.local/share/muesli/muesli.duckdb`

## Core Agent Workflow

These four commands are the primary workflow for an agent working with meeting transcripts:

### 1. Sync — ensure data is up to date

```bash
muesli sync                # Download new/updated transcripts
muesli sync --force        # Force re-sync, ignoring cache timestamps
muesli sync --reindex      # Reindex all without re-downloading
```

Auth tokens are auto-detected from the Granola app on macOS. Run sync before searching if you need the latest meetings.

### 2. Local — see what meetings exist

```bash
muesli local               # List all meetings from local DB (no auth needed)
```

Output: tab-separated `doc_id`, `date`, `title` (sorted newest first, matches `list`). Pipe to `head` for recent meetings or `grep` for a date range:

```bash
muesli local | head -10                   # Last 10 meetings
muesli local | grep "2026-02"             # All February 2026 meetings
```

### 3. Find — search for relevant meetings

```bash
muesli find "quarterly planning"          # Semantic search (default)
muesli find "quarterly planning" --text   # Full-text search
muesli find "budget" -n 5                 # Limit results (default: 10)
```

Output: numbered list with title, date, score, and doc ID. Use the doc ID with `show`.

### 4. Show — view a meeting's content

```bash
muesli show <DOC_ID>                      # Summary, metadata, attendees
muesli show <DOC_ID> --full               # Full transcript
```

Displays title, date, duration, attendees, labels, and summary (or full transcript with `--full`). Works offline.

## Additional Commands

### Search (full-text, returns file paths)

```bash
muesli search "quarterly planning"              # Full-text BM25 search
muesli search --semantic "team collaboration"    # Semantic search
muesli search "budget" -n 5                      # Limit results (default: 10)
```

Output: numbered list with title, date, and file path. Use `find` instead when you need doc IDs for `show`.

### List (API-based)

```bash
muesli list                               # List from API (requires auth)
```

Output: tab-separated `doc_id`, `date`, `title`. Prefer `local` for offline/faster access.

### Fetch a document

```bash
muesli fetch <DOC_ID>                     # Fetch a specific document by ID from API
```

### Query by metadata

```bash
muesli query --attendee "Alice"           # Filter by attendee
muesli query --label "Planning"           # Filter by label
muesli query --title "standup"            # Search by title
muesli query --title "standup" -n 10      # Limit results (default: 20)
```

### Stats

```bash
muesli stats                              # Meeting statistics from local DB
```

### Summarize a transcript

```bash
muesli summarize <DOC_ID>                 # Print summary to stdout
muesli summarize <DOC_ID> --save          # Save to summaries directory
```

Requires an OpenAI API key (`muesli set-api-key`).

### Configuration

```bash
muesli set-api-key                        # Store OpenAI API key in keychain
muesli set-config --show                  # Show current summarization config
muesli set-config --model gpt-4o          # Set summarization model
muesli set-config --prompt-file path.txt  # Set custom summarization prompt
```

### Utility

```bash
muesli open                               # Open data directory in Finder
muesli fix-dates                          # Fix file mod dates to match meeting dates
muesli tui                                # Interactive terminal dashboard
muesli mcp                                # Start MCP server for AI integration
```

## Transcript Format

Transcripts are markdown with YAML frontmatter:

```yaml
---
doc_id: uuid
source: granola
created_at: ISO-8601 timestamp
title: Meeting Title
participants: []
duration_seconds: null
labels: []
generator: muesli 1.0
---
```

Body contains timestamped speaker turns: `**Speaker (HH:MM:SS):** text`

Note: Speaker names are often generic ("Speaker") rather than identified by name.

## Usage Patterns

### Find what was discussed in a meeting

```bash
muesli find "budget review" -n 5               # Find related meetings
muesli show <DOC_ID>                            # View the summary
muesli show <DOC_ID> --full                     # View full transcript
```

### Find meetings in a date range

```bash
muesli local | grep "2026-02"                  # All February 2026 meetings
```

### Read a transcript file directly

Transcripts are markdown files at `~/.local/share/muesli/transcripts/`. Read them directly if needed.

### Offline usage

Local commands (`local`, `show`, `find`, `search`, `query`, `stats`) work without API access.

### Syncing to another machine with rsync

You can copy the muesli data directory to another machine so it can use all offline commands without Granola API access or a Granola account. The source machine must have already run `muesli sync`.

**Full sync (all features including search and embeddings):**

```bash
rsync -av --delete \
  ~/.local/share/muesli/ \
  user@remote:~/.local/share/muesli/
```

**Minimal sync (just transcripts, database, and index — skip large model files):**

```bash
rsync -av --delete \
  --exclude='models/' \
  --exclude='raw/' \
  --exclude='tmp/' \
  ~/.local/share/muesli/ \
  user@remote:~/.local/share/muesli/
```

**What each directory contains:**

| Directory | Size | Purpose | Needed for |
|---|---|---|---|
| `transcripts/` | ~25 MB | Markdown files | `show --full`, direct file reads |
| `muesli.duckdb` | ~6 MB | Document metadata, attendees, labels | `local`, `show`, `query`, `stats` |
| `index/` | ~16 MB | Tantivy full-text index + vector store | `find`, `search` |
| `raw/` | ~94 MB | Raw JSON from API | Rebuilding transcripts (optional) |
| `models/` | ~128 MB | ONNX embedding model | Semantic search (downloaded on first sync) |
| `summaries/` | small | Generated summaries | `summarize --save` output |
| `tmp/` | — | Temp files during sync | Not needed |

**Using a custom data directory on the remote:**

```bash
# Sync to a non-default location
rsync -av --delete ~/.local/share/muesli/ user@remote:~/muesli-data/

# On the remote, point muesli to it
muesli --data-dir ~/muesli-data local
muesli --data-dir ~/muesli-data show <DOC_ID>
```

**Keeping it up to date (run after each sync):**

```bash
muesli sync && rsync -av --delete --exclude='models/' --exclude='tmp/' \
  ~/.local/share/muesli/ user@remote:~/.local/share/muesli/
```

The remote machine needs `muesli` installed but does not need a Granola account or API token. If you exclude `models/`, semantic search (`muesli find`) will automatically fall back to text search.

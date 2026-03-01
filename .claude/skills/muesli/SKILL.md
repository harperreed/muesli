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

## Commands

### Sync transcripts from Granola

```bash
muesli sync              # Download new/updated transcripts
muesli sync --reindex    # Reindex all without re-downloading
```

Auth tokens are auto-detected from the Granola app on macOS.

### Search transcripts

```bash
muesli search "quarterly planning"              # Full-text BM25 search
muesli search --semantic "team collaboration"    # Meaning-based semantic search
muesli search "budget" -n 5                      # Limit results
```

Output format: numbered list with title, date, and file path.

### Find documents (returns doc IDs)

```bash
muesli find "quarterly planning"                 # Semantic search (default)
muesli find "quarterly planning" --text          # Full-text search
muesli find "budget" -n 5                        # Limit results
```

Output format: numbered list with title, date, score, and doc ID. Use with `show` to view details.

### List all transcripts

```bash
muesli list                                      # List from API (requires auth)
muesli local                                     # List from local DB (no auth needed)
```

`list` output: tab-separated `doc_id`, `date`, `title`.
`local` output: tab-separated `date`, `doc_id`, `title` (sorted oldest first).

### Show a document

```bash
muesli show <DOC_ID>                             # Show summary/metadata
muesli show <DOC_ID> --full                      # Show full transcript
```

Displays title, date, duration, attendees, labels, and summary (or full transcript with `--full`). No API key needed.

### Read a transcript file directly

Transcripts are markdown files. Read them directly:

```bash
cat ~/.local/share/muesli/transcripts/2025-06-20_pricing-discussion.md
```

### Query by metadata

```bash
muesli query --attendee "Alice"                  # Filter by attendee
muesli query --label "Planning"                  # Filter by label
muesli query --title "standup"                   # Search by title
```

### Summarize a transcript

```bash
muesli summarize <DOC_ID>          # Print summary to stdout
muesli summarize <DOC_ID> --save   # Save to summaries directory
```

Requires an OpenAI API key (`muesli set-api-key`).

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
muesli local | grep "2025-06"                  # All June 2025 meetings (no auth)
muesli list | grep "2025-06"                   # Same, via API
```

### Offline usage

Local commands (`local`, `show`, `find`, `search`, `query`, `stats`) work without API access. Copy the data directory (`~/.local/share/muesli/`) to another machine and use `--data-dir` to point to it.

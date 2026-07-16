# Granola public API research

Research date: 2026-07-16

## Outcome

Granola now provides an official, read-only REST API. Muesli can use a user-created
API key instead of recovering Granola's WorkOS session or Data Protection Keychain
DEK. The API is sufficient for titles, timestamps, owners, calendar metadata,
attendees, folders, AI summaries, and transcripts. It does not document a field for
the user's handwritten/raw note content, so migrating Muesli to it would lose that
content unless Granola extends the API or Muesli also uses another supported source.

## Authentication and access

- Base URL: `https://public-api.granola.ai/v1`
- Authentication: `Authorization: Bearer grn_...`
- Personal keys are created in Granola at Settings → Connectors → API keys.
- Available note scopes are Personal notes, Public notes, or both, subject to the
  workspace plan and administrator controls.
- Business and Enterprise workspaces are supported. Workspace administrators can
  also create workspace-owned keys with access to public notes and explicitly
  granted spaces.
- The documented API is read-only.

This API key is distinct from the WorkOS access token in `supabase.json.enc`. The
official API therefore removes the need for Keychain entitlement workarounds.

## Endpoints

### `GET /notes`

Lists accessible notes. Query parameters:

| Parameter | Meaning |
| --- | --- |
| `created_before` | ISO date or timestamp upper bound |
| `created_after` | ISO date or timestamp lower bound |
| `updated_after` | ISO date or timestamp incremental-sync bound |
| `folder_id` | Include a folder and its descendants |
| `cursor` | Opaque continuation cursor |
| `page_size` | 1–30, default 10 |

The response is `{ "notes": [...], "hasMore": bool, "cursor": string|null }`.
List items contain the public `not_...` ID, title, owner, and created/updated
timestamps. Pagination is cursor-based, not the private API's offset pagination.

### `GET /notes/{note_id}`

Returns one note. Add `?include=transcript` to include its transcript. The public
note ID matches `^not_[a-zA-Z0-9]{14}$`; it is not the private API's document UUID.

Documented fields include:

- title, owner, creation/update timestamps, and Granola web URL;
- calendar event title, invitees, organiser, event ID, and scheduled start/end;
- attendees;
- folder membership, including ancestor folders;
- `summary_text` and nullable `summary_markdown`;
- optional transcript entries with speaker source, text, start time, and end time.

macOS transcript speaker sources include `microphone` and `speaker`. Mobile entries
may also contain `diarization_label`.

### `GET /folders`

Lists accessible folders alphabetically using `cursor` and `page_size` (1–30,
default 10). Each folder has a `fol_...` ID, name, and nullable parent folder ID.

## Limits and omissions

- Granola documents a sustained limit of 5 requests/second, a burst capacity of 25
  requests in 5 seconds, and HTTP 429 when exceeded.
- The API only exposes notes after the required generated content is available;
  processing or unsummarized notes can be absent from lists or return 404.
- No write endpoint is documented.
- No webhook endpoint is documented, so Muesli should poll using `updated_after`.
- The note response does not document handwritten/raw user notes, ProseMirror panel
  data, labels, duration, company enrichment, or LinkedIn enrichment.

## Impact on current Muesli

Muesli is not currently wired to this API:

- `src/auth.rs` resolves a WorkOS bearer token from Granola's private session.
- `src/api.rs` lists private documents with `POST /v2/get-documents` and offset
  pagination.
- Its existing `get_public_note` method reuses the same token and models only three
  fields; official public API keys and the complete public schema need separate
  types and configuration.
- `RawTranscript` expects flat private-API fields such as `source` and
  `start_timestamp`; the public API nests speaker data and uses `start_time` and
  `end_time`.

A minimal supported sync would therefore:

1. accept a `grn_...` API key without reading Granola's session files;
2. paginate `GET /notes` with cursors and `updated_after`;
3. fetch each changed note through `GET /notes/{id}?include=transcript`;
4. adapt the public note, summary, attendee, calendar, folder, and transcript schema
   into Muesli's internal document model;
5. preserve existing local data for fields the public API omits.

## Installed-client corroboration

Granola 7.427.2 is installed locally. Its `app.asar` SHA-256 is
`2c014dede9f4deecddbb89983d1c0071cbe431edbbbb1580f2f6ece94a2a6299`.
Static inspection confirms client functions for creating, listing, updating, and
revoking public API keys, and UI scope labels for Personal notes and Public notes.
No Granola executable was run and no credential was read during this research.

The same client still contains a much larger private API surface at
`api.granola.ai`, authenticated with a WorkOS bearer token and Granola-specific
client headers. Those private routes are unnecessary for the supported Muesli path.

## Sources

- [Granola API overview](https://docs.granola.ai/introduction)
- [List Notes](https://docs.granola.ai/api-reference/list-notes)
- [Get Note](https://docs.granola.ai/api-reference/get-note)
- [List Folders](https://docs.granola.ai/api-reference/list-folders)
- [API changelog](https://docs.granola.ai/api-reference/changelog)

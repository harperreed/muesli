# Granola 7.427.2 companion CLI static analysis

- Analysis date: 2026-07-16
- Scope: `/Applications/Granola.app/Contents/Resources/bin/granola` and its matching Electron IPC server
- Method: static inspection only; neither the CLI nor the Granola launcher was executed by this analysis

## Executive summary

The companion CLI is fully understood well enough to integrate with Muesli. It does not read Granola's encrypted session, retrieve a Supabase key, or call Granola's cloud APIs directly. It is a small, stripped Go client for a private local IPC service hosted by the running Granola desktop app.

The client reads a mode-0600 JSON capability file from `~/Library/Application Support/Granola/companion-cli/companion-cli.json`. That file contains a protocol version, a Unix socket path, and a random per-app-run token. The CLI connects to the socket, authenticates with the token, sends one newline-delimited JSON request, receives one JSON response, and prints the response's `result` value to stdout. The desktop app owns the WorkOS bearer token and makes the cloud requests.

The bridge exposes enough data for Muesli's core sync: note discovery, note and summary content, and transcript chunks. It is not a complete replacement for Muesli's current private-API responses because it omits labels, duration, and richer attendee/company metadata.

The bridge was not active during the final read-only check. Granola 7.427.2 was running, but the companion metadata directory, capability file, and socket were absent. The bundled feature flag defaults to false. The missing artifacts prove that the local service is currently unavailable, but static analysis alone cannot distinguish a disabled account flag from a server startup failure.

## Artifact identity

| Property | Value |
|---|---|
| Granola app version | `7.427.2` |
| CLI path | `/Applications/Granola.app/Contents/Resources/bin/granola` |
| CLI size | 5,128,368 bytes |
| CLI SHA-256 | `93f9ee71467882c7dfa7665b32677e4015c09527875fd1d60b5a9265609a171e` |
| arm64 slice SHA-256 | `d420e8092ddba874762d0ec8c479e32f6f5d46935b102182bbb513a874bfbe38` |
| x86_64 slice SHA-256 | `2a3636be20035d033b6792a23e2931b0569f3ca2f3912086663b58fce5026846` |
| Architectures | arm64 and x86_64 Mach-O 64-bit |
| Language/toolchain | Go 1.23.12, module `granola-companion-cli` |
| Signing identity | Developer ID Application: Granola Labs Ltd (`QZ7DHHLN25`) |
| Signature verification | strict verification passed |
| Matching ASAR SHA-256 | `2c014dede9f4deecddbb89983d1c0071cbe431edbbbb1580f2f6ece94a2a6299` |

The CLI is stripped, but Go's `gopclntab` retains the original function and source-file names. Recovered functions include `main.dialServer`, `main.invoke`, `main.ensureServerAvailable`, `main.authenticateConnection`, `main.writeJSONLine`, `main.readJSONLine`, `main.readMetadata`, and `main.getMetadataPath`. The two source paths are `granola-companion-cli/main.go` and `granola-companion-cli/dial_unix.go`.

## Client lifecycle

Every invocation begins with an availability probe before command dispatch:

1. `main.readMetadata` calls `os.UserConfigDir` and reads `Granola/companion-cli/companion-cli.json` beneath it.
2. It JSON-decodes the file and requires `protocol_version == 1`, a non-empty `socket_path`, and a non-empty `token`.
3. `main.dialServer` opens the metadata-provided Unix socket with a five-second dial timeout.
4. The client applies a 30-second connection deadline.
5. It sends an authentication line and requires an auth response with `ok: true`.
6. It closes this probe connection.
7. Only then does it parse and dispatch `help`, `notes`, or `folders`.

This means even built-in help is unavailable when the desktop bridge is unavailable. A data command then performs a second metadata read and opens a second authenticated connection. On that connection it applies another 30-second deadline, sends one request, reads one response, closes the connection, and prints the result.

The fixed request ID is `cli-request`; it is not a generated UUID. Client writes are JSON followed by `\n`. Client reads use `bufio.Reader.ReadString('\n')`, trim whitespace, and JSON-decode the resulting line.

## Capability file and local security boundary

The desktop app writes this shape:

```json
{
  "protocol_version": 1,
  "socket_path": "/private/var/folders/.../T/granola-companion-cli.sock",
  "token": "<random UUID>"
}
```

On macOS, the paths are derived independently but converge:

- server metadata: Electron `appData/Granola/companion-cli/companion-cli.json`
- client metadata: Go `os.UserConfigDir()/Granola/companion-cli/companion-cli.json`
- socket: Electron `app.getPath("temp")/granola-companion-cli.sock`

The app requests mode `0600` when it creates the metadata file. After the socket begins listening, it asynchronously requests `chmod(0600)` and logs a warning if that operation fails. It generates a fresh UUID token for each server instance and removes the metadata and socket when the server stops. Because the chmod is not awaited before metadata is written, there can be a short permissions race; the mode-0600 token file still prevents a process that cannot read the token from authenticating.

The token is a local bearer capability stored as plaintext inside the protected file. The server compares only the supplied token; it does not verify the connecting executable's code signature. Consequently, another process running as the same user and able to read the file can use the service. The security boundary is the macOS account plus filesystem permissions, not the signed CLI binary.

## Wire protocol

The protocol is newline-delimited JSON. Authentication must be the first successful message on a connection.

Client authentication:

```json
{"type":"auth","token":"<token from metadata>"}
```

Successful server reply:

```json
{"type":"auth","ok":true}
```

Failed authentication returns `ok: false`, includes `error: "Invalid companion CLI token"`, and closes the connection.

A request has this envelope:

```json
{"type":"request","id":"cli-request","method":"notes.list","params":{"limit":10,"offset":0}}
```

Success and failure responses are:

```json
{"type":"response","id":"cli-request","ok":true,"result":{}}
{"type":"response","id":"cli-request","ok":false,"error":{"code":"...","message":"..."}}
```

The server rejects unauthenticated requests with `UNAUTHENTICATED` and closes the connection. It caps accumulated inbound data at 256 KiB. Schema or JSON failures return `INVALID_REQUEST`. Application failures include `NOT_FOUND` and `INTERNAL_ERROR`. Client-side failures are normalized to codes including `APP_NOT_RUNNING`, `INVALID_METADATA`, `UNSUPPORTED_PROTOCOL`, `REQUEST_TIMEOUT`, and `REQUEST_FAILED`.

## Commands and method mapping

| CLI command | IPC method | Params |
|---|---|---|
| `notes list` | `notes.list` | `limit` (default 50, max 100), `offset` (default 0), optional `created_after`, optional `created_before` |
| `notes get --id ...` | `notes.get` | `ids`, one to 100 UUIDs |
| `notes transcript get --id ...` | `notes.transcript.get` | `id`, a UUID |
| `folders list` | `folders.list` | optional trimmed `search`, one to 200 characters |
| `folders add-document` | `folders.documents.add` | `folder_id` and `document_id`, both UUIDs |
| `folders remove-document` | `folders.documents.remove` | `folder_id` and `document_id`, both UUIDs |

Date filters must be ISO 8601 timestamps with an explicit timezone, such as `Z` or `-07:00`.

Read results have these useful shapes:

- `notes.list`: `notes`, `has_more`, and nullable `next_offset`.
- `notes.get`: `notes` and `not_found`; each returned note includes basic metadata plus `notes_plain`, `notes_markdown`, `summary_text`, and `summary_markdown`.
- `notes.transcript.get`: note `id`, `title`, and transcript chunks containing `id`, `text`, `source`, `is_final`, `start_timestamp`, and `end_timestamp`.
- `folders.list`: `folders` with ID, name, parent, visibility, and document count.
- folder mutations: a `message` string.

The list metadata includes title, creation/update timestamps, status, owner name/email, and a simplified calendar event. The server merges owned and shared notes, de-duplicates by ID, sorts newest first, then paginates.

## Where cloud authentication happens

The CLI imports no Granola API SDK and contains no Granola API URL or static cloud credential. It connects only to the local socket named in metadata.

For every IPC operation, the Electron server obtains the active desktop session internally and adds `X-Granola-Client: companion-cli` to its Granola API requests. The WorkOS access token never crosses the companion socket. This is why reversing the CLI does not recover the Supabase/WorkOS key: the design deliberately keeps that credential inside Granola Desktop.

## Muesli integration assessment

The simplest compatibility-preserving adapter is to invoke the signed bundled CLI and parse stdout JSON. A core sync would:

1. Page through `notes list` to discover note IDs.
2. Fetch note and summary content with batched `notes get` calls of at most 100 IDs.
3. Fetch each desired transcript with `notes transcript get`.
4. Map missing richer metadata to explicit optional values rather than inventing it.

Calling the private socket directly is also technically straightforward, but it couples Muesli to an undocumented protocol and duplicates Granola's official client behavior. Protocol versioning makes breakage detectable, not impossible. Subprocess integration keeps Granola's CLI as the compatibility boundary and should be the default implementation.

Neither approach works until the desktop bridge exists. During this analysis Granola was running, but `~/Library/Application Support/Granola/companion-cli` did not exist. Muesli therefore cannot use the companion route on this machine at this moment. It can still use an explicitly supplied bearer token through its existing `--token` or `BEARER_TOKEN` paths.

## Static-analysis limitations

No Granola component or companion command was launched, and no live socket or cloud request was made. The analysis therefore does not establish:

- why the bridge is absent for the current running session;
- whether this account can enable the `companion_cli` flag through a Labs UI;
- runtime output formatting beyond the recovered client/server code;
- server-side behavior that differs from the bundled 7.427.2 implementation; or
- future compatibility of this private, default-off feature.

## Reproduction notes

Principal tools used were `file`, `lipo`, `codesign`, `shasum`, `go version -m`, `go tool nm`, radare2/rabin2, Go's local `debug/macho` and `debug/gosym` packages, and bounded static extraction of `dist-electron/main/index.js` from the matching ASAR.

Relevant recovered client address ranges in the arm64 slice:

| Function | Address range |
|---|---|
| `main.dialServer` | `0x100109650`–`0x1001096f0` |
| `main.invokeAndPrint` | `0x10010c020`–`0x10010c280` |
| `main.invoke` | `0x10010c280`–`0x10010c7f0` |
| `main.ensureServerAvailable` | `0x10010c860`–`0x10010c9c0` |
| `main.authenticateConnection` | `0x10010ca30`–`0x10010ce20` |
| `main.writeJSONLine` | `0x10010cf50`–`0x10010d020` |
| `main.readJSONLine` | `0x10010d020`–`0x10010d0b0` |
| `main.readMetadata` | `0x10010d0b0`–`0x10010d410` |
| `main.getMetadataPath` | `0x10010d410`–`0x10010d4d0` |

Matching Electron server code begins near byte 6,952,667 in the extracted 7.427.2 `dist-electron/main/index.js`; the server's feature-flag subscription is near byte 7,687,048. These offsets are artifact-specific.

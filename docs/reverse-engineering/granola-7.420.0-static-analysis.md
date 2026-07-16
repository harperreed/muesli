# Granola 7.420.0 static analysis

- Analysis date: 2026-07-15
- Scope: `/Applications/Granola.app`, beginning with `Contents/MacOS/Granola`
- Method: static inspection only; no Granola executable or embedded helper was launched

## Executive summary

The requested binary is a small, stripped, universal Electron launcher. Its `main` function delegates to `ElectronMain`; nearly all Granola-specific behavior lives in the 58.6 MB `Resources/app.asar` archive and native modules beside it.

Static evidence confirms that Granola captures microphone and system audio, maintains separate microphone/system transcription sources, and sends raw 16-bit audio over TLS WebSockets to AssemblyAI or Deepgram using short-lived tokens obtained through Granola APIs. Final transcript chunks are then synchronized to Granola's cloud. Local application data is stored in a SQLCipher database with a 32-byte data-encryption key held in the macOS Keychain.

The most important privacy-sensitive optional feature is `ambient_context`. It is disabled by default and requires both a feature flag and the `ambientContextEnabled` preference. When enabled, it OCRs foreground windows, writes app names, window titles, and recognized text to plaintext daily logs, and batches captures to Granola's `process-ambient-context` endpoint.

The most concrete security finding is that the code named `networkAllowlist` is observe-only. Requests outside the list generate `network-allowlist-blocked` telemetry, but Electron is explicitly told `cancel: false`, and the Node/Undici wrapper still dispatches the request.

## Artifact identity and integrity

| Property | Value |
|---|---|
| App version | `7.420.0` |
| Launcher SHA-256 | `1a9ee95d855591b3af3962132ae75ae6e6258b23115b365729a2afec57106385` |
| Launcher size | 119,328 bytes |
| Architectures | arm64 and x86_64 Mach-O 64-bit |
| arm64 slice SHA-256 | `6cd0048cb73a457636f610d1c71c8560889c2132679842bd8ebe0739959b1be5` |
| x86_64 slice SHA-256 | `50902454e70fb947cc31b89e2bfc49c7cf8639308c6a440ba69b4fd968d6aa6c` |
| ASAR SHA-256 | `82c12a46fad7794e1f893edd160d01513f8bb8270a60027e1989e9ec88e99cac` |
| Signing identity | Developer ID Application: Granola Labs Ltd (`QZ7DHHLN25`) |
| Notarization | Stapled ticket present |
| Signature verification | `codesign --verify --deep --strict` passed |

`Info.plist` records ASAR integrity hash `faa7b91f...3852`. This is the SHA-256 of the 253,576-byte ASAR header JSON, not the whole archive. The computed header hash matches exactly. A sampled embedded file also matched its per-file SHA-256 record.

## Component map

```text
Contents/MacOS/Granola
  -> Electron Framework / ElectronMain
      -> Resources/app.asar
          -> dist-electron/main/index.js       privileged main process
          -> dist-electron/preload/preload.js  renderer bridge
          -> dist-app/assets/*.js              renderer UI
          -> utility processes                 audio, SQLite, EventKit,
                                                permissions, meeting automation
      -> Resources/native/*.node               macOS native capabilities
      -> Resources/bin/granola                 companion notes/folders CLI
      -> Resources/native-host/...             browser consent native host
      -> Library/SystemExtensions/...           virtual-camera extension
```

The ASAR contains 959 entries: 754 packed and 205 unpacked. There are 531 JavaScript files. Source-map comments remain in the bundles, but `.map` files are not shipped.

## Confirmed behavior

### Launcher

The arm64 `main` function is 96 bytes. It checks the Electron `run-as-node` fuse and branches to either `ElectronMain` or `ElectronInitializeICUandStartNode`. It contains no Granola business logic.

### Audio capture and transcription

- `native/granola.node` links AVFoundation, AVFAudio, and CoreAudio.
- Native strings identify microphone capture, system-audio capture, CoreAudio aggregate devices/taps, ScreenCaptureKit fallback, audio-device switching, WebRTC AEC3 echo cancellation, and 48 kHz audio.
- The Electron main process forks `dist-electron/audio_process/index.js` and sends it `start-audio-capture` and `stop-audio-capture` messages.
- Capture callbacks receive distinct `microphoneBuffer` and `systemAudioBuffer` values with capture timestamps.
- The transcription handler keeps a source map keyed by `microphone` and `system`, creates a separate provider connection per source, and sends buffers while its state is `starting` or `running`.
- AssemblyAI uses `wss://streaming.assemblyai.com/v3/ws`; Deepgram uses `wss://api.deepgram.com/v1/listen` or a dedicated Deepgram endpoint. Both paths send raw buffer bytes through WebSocket connections.
- AssemblyAI is the bundled default provider for the separate Granola Talk dictation feature; Deepgram is supported as a flag-selected alternative.
- Granola obtains provider tokens through authenticated Granola API calls. No private transcription-provider credential was found hardcoded in the app.
- Final transcript chunks are submitted through Granola's transcript synchronization API with document IDs, source, timing, word details, provider, model, and optional speaker attribution.

This supports the high-confidence conclusion that captured meeting audio leaves the machine for cloud transcription during an active transcription session. Static analysis does not establish which provider is selected for a particular account at a particular moment.

### Local persistence and secrets

- The storage utility uses `better-sqlite3-multiple-ciphers` with `cipher = 'sqlcipher'`, `legacy = 4`, and a hex key.
- A random 32-byte data-encryption key is created when necessary.
- Packaged macOS builds load or create that key through `native/keychain.node`, which links CryptoKit and Security.
- Older `storage.dek` files are decrypted through Electron `safeStorage`, imported into Keychain, and removed after successful migration.
- The database uses WAL mode and `synchronous = NORMAL`.

The local SQLite protection is materially better than plaintext storage. This does not cover every local artifact: the optional ambient-context log described below is explicitly written as plaintext.

### Ambient context

`ambient_context.node` links AppKit, ApplicationServices, CoreGraphics, and Vision. Native strings identify the frontmost application, focused-window title, Accessibility APIs, screen-recording checks, and Vision OCR.

The feature is double gated:

- feature flag `ambient_context`, default `false`; and
- preference `ambientContextEnabled`, defaulting to false when absent.

When both gates are true, the code:

1. Starts native foreground-window monitoring.
2. Captures timestamp, app name, window title, and recognized text.
3. Appends the complete capture to `userData/ambient-context-logs/ambient-context-YYYY-MM-DD.txt` without encryption.
4. Queues up to 100 captures.
5. Sends batches after 15 captures or every five minutes to `https://cinnamon.api.granola.ai/v1/process-ambient-context`.
6. Limits each transmitted capture to 200 text entries of at most 1,000 characters each.

Privacy impact is high when enabled because this can include content from unrelated foreground applications. Static analysis does not show that it is enabled for this installation or account.

### Global input listener / Granola Talk

The bundled `keyspy` helper creates a CoreGraphics event tap for keyboard, modifier, and mouse events. Its protocol exposes key codes, up/down state, mouse location, and an event ID to the Electron process.

Granola's current callback filters events to a configured activation key, defaulting to `Fn`, for push-to-talk/hybrid dictation behavior. Non-activation events are returned without being blocked; the app does not appear to accumulate typed text. In one combo-abort debug path it may log the name or virtual key code of the non-activation key.

This is not evidence of a keylogger exfiltrating typed content. It is nevertheless a broad Accessibility-sensitive primitive: the helper observes all input events before the Granola callback filters them.

### Calendar, meeting detection, automation, and virtual camera

- `eventkit.node` reads calendars/events and subscribes to EventKit changes after permission is granted.
- `macos_mic_apps_with_devices.node` identifies processes actively using microphone devices, including Zoom, Teams, browsers, Slack, FaceTime, Webex, and other meeting apps.
- `third_party_meeting_automation.node` and its Swift bridge use Accessibility/ApplicationServices to monitor Zoom speakers, inspect participants, focus chat fields, paste consent messages, and change the selected Zoom camera.
- `meet-consent-host` is installed as a Chrome-family native-messaging host for Granola's consent-messaging extension.
- The signed, sandboxed CoreMediaIO system extension exposes a virtual camera and has camera plus app-group entitlements.
- The companion `Resources/bin/granola` binary is a Go CLI that can list/get notes, retrieve transcripts, list folders, and change folder membership through the running desktop app's authenticated account.

### Analytics and diagnostics

- Main-process Amplitude events are sent to `https://amp.granola.ai/2/httpapi`.
- A stable device identifier is derived from hardware/platform identifiers and hashed before inclusion in main-process metadata.
- Sentry is configured for production crash/error reporting to a Granola project at `ingest.us.sentry.io`.
- Renderer-side Amplitude session replay code is included and controlled by a runtime flag. When enabled, the plugin is configured at sample rate 1, masks all ordinary rendered text, masks all inputs, and does not record canvas content. It still records UI structure and interaction/session metadata.
- The app also exposes opt-in troubleshooting tools that can record an Electron performance trace and heap snapshot, package them as a ZIP, and reveal the file in Finder.

## Security findings

### RE-01: The network allowlist does not enforce blocking

- Severity: Medium defense-in-depth gap
- Status: Confirmed

The request classifier marks non-allowlisted HTTP(S) and WebSocket URLs as disallowed. However:

- Electron's `webRequest.onBeforeRequest` callback always returns `{cancel: false}`.
- The Undici dispatcher logs a rejected classification and then calls the inner dispatcher anyway.
- The WebSocket path logs the classification and still constructs the socket.

The event name `network-allowlist-blocked` is therefore misleading: it means observed, not blocked. If this mechanism is intended as an egress security boundary, it is broken. If it is intentionally audit-only, its naming should be changed so operators do not rely on it.

### RE-02: Ambient-context logs are plaintext and unusually sensitive

- Severity: High privacy impact when enabled
- Status: Confirmed, feature disabled by default

Foreground app names, window titles, and OCR text are appended to daily plaintext files and also sent to a Granola endpoint in bounded batches. The SQLite encryption design does not protect these logs.

Recommended controls are explicit in-product disclosure, a visible always-on indicator, short retention, encryption at rest, per-application exclusions, and a one-click purge operation.

### RE-03: Renderer compromise would have a large privilege blast radius

- Severity: Medium hardening concern; no exploit demonstrated
- Status: Confirmed configuration, exploitability unvalidated

Positive controls:

- `nodeIntegration: false`
- `contextIsolation: true`
- external navigation interception and denied popup creation in primary paths
- a local packaged renderer rather than a remote main UI

Risk-increasing controls:

- the HTML Content Security Policy is commented out;
- preload exposes generic `ipcInvoke`, `ipcSend`, and `ipcOn` functions with caller-provided channel names rather than a narrow capability API; and
- privileged main-process handlers include token storage, permission prompts, screenshots, companion-skill install/remove, virtual-camera management, and other native actions.

An XSS or compromised renderer dependency could therefore reach more main-process functionality than necessary. This is a hardening finding, not proof of a currently exploitable vulnerability.

## Entitlements and permissions

The main app requests or declares access to:

- microphone and system audio;
- camera;
- calendar;
- screen capture/recording;
- Accessibility through helper behavior;
- Keychain group `QZ7DHHLN25.granola`;
- associated domain `applinks:notes.granola.ai`;
- system-extension installation;
- JIT and unsigned executable memory, expected for Electron.

The app is not sandboxed as a whole. The virtual-camera system extension is sandboxed.

## Hypotheses

| Hypothesis | Status | Confidence | Evidence |
|---|---|---|---|
| Requested binary contains Granola's business logic | Refuted | High | 96-byte launcher `main`; delegates to Electron |
| Granola sends meeting audio off-device | Confirmed | High | Native capture callbacks -> per-source handler -> provider WebSocket `send` |
| Granola stores its primary local database encrypted | Confirmed | High | SQLCipher pragmas plus 32-byte Keychain DEK |
| Granola exfiltrates general keystroke text | Not supported | Medium-high | Helper sees all input events, but app filters to activation key and does not assemble typed text |
| Ambient context can capture unrelated application text | Confirmed when enabled | High | frontmost-window OCR, plaintext logs, batch API submission |
| Network allowlist blocks unapproved destinations | Refuted | High | `cancel: false`; inner dispatch continues |
| Installed app is tampered or unsigned | Refuted | High | strict deep signature verification and ASAR header integrity match |

## Static-analysis limitations and next tests

No Granola component was executed. Consequently, this report does not establish:

- current account-level feature-flag values;
- the currently selected transcription provider;
- actual runtime payloads, headers, certificate behavior, or request frequency;
- whether ambient context or session replay is enabled for this account;
- whether any server-side behavior differs from the bundled client contract.

The highest-value next phase would be controlled dynamic observation with network access separately approved: process/file tracing, DNS/TLS destination capture, and payload-shape inspection during a synthetic meeting containing no private data. A second useful check would inspect runtime preferences and feature flags, but that would touch personal application data and should also be explicitly authorized.

## Reproduction notes

Principal tools used:

- `file`, `lipo`, `otool`, `nm`, `codesign`, `plutil`, `shasum`
- radare2/rabin2 6.1.8 for launcher metadata, imports, and disassembly
- a read-only ASAR header parser validated against embedded SHA-256 records
- `rg` and bounded byte-context extraction over unpacked production JavaScript

Key code locations in the extracted, minified `dist-electron/main/index.js` for this exact artifact:

- data-encryption key handling: approximately byte 5,634,392
- audio-process manager: approximately byte 5,885,028
- network classifier/observe-only dispatcher: approximately byte 6,050,829
- transcript source buffer routing: approximately byte 6,105,120
- ambient-context capture/log/upload logic: approximately byte 5,714,081
- main BrowserWindow hardening configuration: approximately byte 7,634,502

These byte offsets are artifact-specific and will change in later builds.

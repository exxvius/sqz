# Architecture

sqz is a [Tauri v2](https://v2.tauri.app) app: a Rust engine behind a React +
TypeScript UI. The engine is headless and has no dependency on Tauri, so it can
be unit-tested on its own.

## Layers

```
┌──────────────────────────── src/ (React + TS) ────────────────────────────┐
│  views (Home, Live, History, Settings)  ·  components  ·  lib/store        │
│                     invoke() ▲                    ▲ listen()               │
└─────────────────────────────┼────────────────────┼───────────────────────┘
                              commands.rs         events.rs (TauriReporter)
┌─────────────────────────────┼────────────────────┼───────────────────────┐
│  run.rs  (worker pool, cancel/pause)  ── uses ──> core::pipeline           │
│                                                                            │
│  core/ (headless engine, no Tauri deps):                                   │
│    config → probe → encoders → encode → verify → paths → replace           │
│          → manifest → discover → pipeline   (+ ffbin, report, util)        │
└────────────────────────────────────────────────────────────────────────────┘
                                   │ subprocess
                          bundled ffmpeg / ffprobe (sidecars)
```

## Modules

| Module              | Responsibility |
|---------------------|----------------|
| `core/config`       | Run settings, codecs, friendly quality presets |
| `core/probe`        | ffprobe → `MediaInfo` (+ computed properties) |
| `core/encoders`     | Detect and validate NVENC / AMF / QSV / VideoToolbox / software |
| `core/encode`       | Build the ffmpeg command (per-family rate control) and run it |
| `core/verify`       | The four safety gates (structural, duration, decode, size) |
| `core/paths`        | Cross-platform same-volume scratch/holding paths |
| `core/replace`      | Atomic swap + trash/holding/delete with recovery |
| `core/manifest`     | Resumable SQLite state (rusqlite, WAL) |
| `core/discover`     | Expand inputs into a de-duplicated video list |
| `core/pipeline`     | Per-file state machine: probe → skip? → encode → verify → swap → record |
| `core/ffbin`        | Locate ffmpeg/ffprobe (custom path → downloaded → PATH) |
| `ffsetup`           | On-demand FFmpeg download/extract + bring-your-own config |
| `core/report`       | `Reporter` trait — the engine's only frontend seam |
| `run`               | Worker pool + cancel/pause orchestration |
| `commands`          | Tauri command surface + app state |
| `events`            | `TauriReporter` — emits engine events to the webview |

## The reporting seam

The engine's only coupling to a frontend is the `core::report::Reporter` trait
(`on_file_start` / `on_file_progress` / `on_file_end` / `on_record`).
`events::TauriReporter` implements it by emitting `sqz-*` events to the webview;
tests use `NoopReporter`.

## Safety invariants (must not regress)

1. An original is only disposed after a verified, smaller replacement exists.
2. Disposal is recoverable (Trash by default; holding/delete keep a restore path).
3. The swap is an atomic same-volume rename.
4. State is durable per file, so any interruption resumes cleanly.
5. `no_gain` (kept original) is distinct from `failed` (something broke).

## Encoder selection

`encoders::detect` parses `ffmpeg -encoders` and **validates** each hardware
candidate with a one-frame test encode — presence in the list does not guarantee
a working driver. `select` picks the best validated encoder for the codec,
hardware first, software always available as a fallback.
`encode::encoder_rate_args` emits the right quality/preset flags per family
(NVENC `-cq`, QSV `-global_quality`, AMF `-qp_i/-qp_p`, VideoToolbox `-q:v`,
software `-crf`).

## Running the tests

`cargo test` compiles the whole crate, and `generate_context!` needs the built
frontend and icons at compile time. So before testing:

```bash
npm ci && npm run build && npm run tauri icon src-tauri/icons/sqz.svg
cd src-tauri && cargo test
```

The engine tests never spawn FFmpeg — they cover pure logic (quality resolution,
skip/marginal predicates, verify tolerances, manifest resume, atomic replace).

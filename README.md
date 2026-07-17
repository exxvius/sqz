<div align="center">

# sqz

**Squeeze your video library. Reclaim disk space. Never lose a file.**

A modern, portable desktop app that bulk re-encodes videos to efficient codecs
(AV1 / HEVC / H.264) and safely replaces the originals — only after the new file
is verified playable, complete, and meaningfully smaller.

A tiny (~6 MB) download. FFmpeg installs itself in one click. No command line.
Works on any GPU or none.

Windows · macOS · Linux

</div>

---

## Why sqz

Most re-encoders make you choose between "easy" and "safe." sqz is both — a clean
desktop app built around a paranoid safety pipeline:

- **Safe by design.** For every file: probe → (skip if already efficient) → encode
  to scratch → **verify** (valid, duration-matched, decodes clean, actually smaller)
  → **atomic swap** → send the original to the **Recycle Bin/Trash**. An original is
  never touched until a verified replacement is in place.
- **Fully resumable.** Progress lives in a SQLite manifest. Close mid-run, reopen,
  continue. Nothing is redone.
- **Works with any hardware.** Auto-detects and uses NVIDIA (NVENC), AMD (AMF),
  Intel (QSV), Apple (VideoToolbox), or falls back to CPU (SVT-AV1 / x265 / x264).
- **One-click FFmpeg.** The app ships tiny (~6 MB). On first run it downloads
  FFmpeg into its own folder in one click — or point it at your own binaries.
- **Friendly quality.** Pick *Maximum savings / Balanced / High quality / Visually
  lossless* — no codec numbers required. Advanced controls are one click away.

## How it protects your files

For each video:

1. **Probe** it with FFprobe. Unreadable/empty → recorded as failed, skipped.
2. **Skip** files already in the target codec at or under the height cap.
3. **Encode** to a scratch dir *on the same volume* (never overwriting in place).
4. **Verify** — all must pass or the original is left untouched:
   - parses as valid video with a readable duration,
   - duration matches the source (±1 s / ±0.5 %),
   - decodes without errors,
   - is at least the required amount smaller (default 10 %).
5. **Swap** atomically (same-volume rename) and send the original to the Recycle
   Bin/Trash (default), a holding folder, or delete it.
6. **Record** the result and bytes saved in the manifest.

## Status

Early development. The engine (probe, encode, verify, atomic swap, resumable
manifest) is implemented in Rust; the UI is built with Tauri v2 + React. See
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Building from source

Requires the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
(Rust toolchain, Node.js, and your platform's webview deps).

```bash
# 1. Install web dependencies
npm install

# 2. Generate the app icons from the source SVG (first time only)
npm run tauri icon src-tauri/icons/sqz.svg

# 3. Run the dev app — click "Download FFmpeg" in the app on first launch
npm run tauri dev
```

To build a portable, self-contained package:

```bash
npm run tauri build
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how the code is organized.

## Licensing

sqz is **MIT** (see [LICENSE](LICENSE)). The installer does **not** bundle
FFmpeg — the app downloads it (a GPL build) into its own data folder at the
user's request, or uses a binary the user already has. See [NOTICE](NOTICE).

FFmpeg is a trademark of Fabrice Bellard. sqz is not affiliated with the FFmpeg
project.

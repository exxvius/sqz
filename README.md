<div align="center">

<img src="docs/images/banner.png" alt="sqz — bulk re-encode your library to modern codecs, reclaim disk space, never lose a file" width="100%">

<br><br>

[![CI](https://img.shields.io/github/actions/workflow/status/exxvius/sqz/ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/exxvius/sqz/actions/workflows/ci.yml)
[![Version](https://img.shields.io/github/v/tag/exxvius/sqz?label=version&style=flat-square&color=2ea043)](https://github.com/exxvius/sqz/releases)
[![License: MIT](https://img.shields.io/github/license/exxvius/sqz?style=flat-square&color=blue)](LICENSE)
![Platforms](https://img.shields.io/badge/Windows%20·%20macOS%20·%20Linux-2ea043?style=flat-square)
![Built with Tauri v2 + Rust](https://img.shields.io/badge/built%20with-Tauri%20v2%20%2B%20Rust-1c3a30?style=flat-square)

</div>

sqz re-encodes the videos sitting on your drives to newer, smaller codecs — AV1,
HEVC, or H.264 — and swaps each original out for the re-encoded copy. The catch
that makes it safe: it does that swap only after checking, one file at a time,
that the replacement actually plays, runs the full length, and is genuinely
smaller. If a single check fails, the original stays exactly where it was.

It's a ~6 MB download. No installer sprawl, no command line. FFmpeg isn't bundled
— sqz fetches it on first launch, or uses a copy you already have.

<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/dashboard-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="docs/images/dashboard-light.png">
    <img src="docs/images/dashboard-dark.png" alt="sqz live dashboard: three videos encoding in parallel with size projections, reclaimed-space meter, and an event log" width="100%">
  </picture>
</div>

## Why

Re-encoding a media library is the kind of chore that's trivial to script and
genuinely nerve-wracking to run. One stray ffmpeg flag and you've overwritten a
movie with a truncated stub, or quietly grown a file you meant to shrink — and
you won't notice until you go to watch it months later.

sqz is built so you don't have to trust it. Every original is treated as
untouchable until a verified, smaller replacement is sitting on the same disk,
ready to drop in with an atomic rename. Stop a run halfway, close the app, pull
the power — start it again next week and it resumes where it left off, redoing
nothing.

## How a file gets replaced

Nothing destructive happens until the new file has earned it:

1. **Probe** with ffprobe. If sqz can't read it, the file is logged as failed and
   left alone.
2. **Skip** anything already in your target codec at or below the height cap.
   There's no point spending an encode on a file that's already lean.
3. **Encode** to a scratch folder on the same volume, so the original is never
   the thing being written to.
4. **Verify** the output. It has to parse as valid video, report a duration
   within a second of the source, decode without errors, and land at least 10%
   smaller (you can change the threshold). Fail any one of those and the original
   wins by default.
5. **Swap** via a same-volume rename, then send the original to the Recycle Bin /
   Trash — or a holding folder, or permanent deletion if you insist.
6. **Record** the outcome and bytes saved to a SQLite manifest.

That manifest is what makes runs resumable and gives you the searchable History
tab below.

## A look around

<p align="center">
  <img src="docs/images/home-dark.png" alt="Home screen: drop zone, queued sources, codec and quality presets, and a Start button" width="49%">
  &nbsp;
  <img src="docs/images/history-dark.png" alt="History screen: all-time reclaimed space, status filters, and per-file results" width="49%">
</p>

Add files or whole folders on the left; watch them process live in the middle;
review everything sqz has ever touched — with search, filters, retry, and
force-process — on the right.

## Whatever hardware you have

sqz checks for a GPU encoder at startup and uses it when it can: NVENC on NVIDIA,
AMF on AMD, QSV on Intel, VideoToolbox on Apple silicon. No hardware path for the
codec you picked? It falls back to a solid software encoder (SVT-AV1, x265, or
x264) and keeps going, slower but just as safe.

You choose a plain-English target — *Maximum savings*, *Balanced*, *High quality*,
or *Visually lossless* — and sqz translates it into reasonable encoder settings.
Every knob is still there under **Advanced** if you'd rather drive manually.

## FFmpeg

To stay small and cleanly MIT-licensed, sqz ships without FFmpeg. On first run it
offers a one-click download (~140 MB, a GPL build kept inside the app's own data
folder), or you can point it at `ffmpeg` / `ffprobe` binaries you already trust.
Nothing is downloaded unless you ask for it.

## Building from source

You'll need the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/):
the Rust toolchain, Node.js, and your platform's webview libraries.

```bash
npm install
npm run tauri icon src-tauri/icons/sqz.svg   # generate app icons (first time only)
npm run tauri dev                            # then click "Download FFmpeg" on first launch
```

Package a portable, self-contained build with:

```bash
npm run tauri build
```

The layout is documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). In short:
the engine (probe, encode, verify, atomic swap, resumable manifest) is Rust; the
UI is React running on Tauri v2.

## Status

Early days — but the full pipeline works end to end, and the safety guarantees
above hold today. Expect the occasional rough edge in the UI before the corners
get sanded down. Bug reports and pull requests are welcome.

## License

sqz is **MIT** ([LICENSE](LICENSE)). The FFmpeg build it downloads is GPL and
lives beside the app in its data folder — it's never bundled into the installer or
linked into the binary. See [NOTICE](NOTICE) for the details.

FFmpeg is a trademark of Fabrice Bellard. sqz is not affiliated with the FFmpeg
project.

#!/usr/bin/env bash
# Fetch full (GPL) FFmpeg + FFprobe builds and place them as Tauri sidecars named
# with the Rust target triple. Run from the repo root:
#
#   ./scripts/fetch-ffmpeg.sh                 # auto-detect this host's triple
#   ./scripts/fetch-ffmpeg.sh <target-triple> # e.g. x86_64-apple-darwin
#
# Override sources with FFMPEG_URL / FFPROBE_URL env vars if a default is stale.
set -euo pipefail

BIN_DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/binaries"
mkdir -p "$BIN_DIR"

TRIPLE="${1:-$(rustc -Vv | sed -n 's/^host: //p')}"
if [[ -z "$TRIPLE" ]]; then
  echo "Could not determine target triple (is rustc installed?)." >&2
  exit 1
fi
echo "Target triple: $TRIPLE"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

place() { # place <src-binary> <base-name>
  local src="$1" base="$2" dest="$BIN_DIR/${2}-${TRIPLE}"
  [[ "$TRIPLE" == *windows* ]] && dest="${dest}.exe"
  cp "$src" "$dest"
  chmod +x "$dest" || true
  echo "  → $dest"
}

case "$TRIPLE" in
  *windows*)
    URL="${FFMPEG_URL:-https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip}"
    echo "Downloading $URL"
    curl -L "$URL" -o "$TMP/ff.zip"
    unzip -q "$TMP/ff.zip" -d "$TMP"
    D="$(find "$TMP" -name ffmpeg.exe -path '*/bin/*' | head -n1)"
    place "$D" ffmpeg
    place "$(dirname "$D")/ffprobe.exe" ffprobe
    ;;
  *linux*)
    URL="${FFMPEG_URL:-https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz}"
    echo "Downloading $URL"
    curl -L "$URL" -o "$TMP/ff.tar.xz"
    tar -xf "$TMP/ff.tar.xz" -C "$TMP"
    D="$(find "$TMP" -maxdepth 2 -name ffmpeg -type f | head -n1)"
    place "$D" ffmpeg
    place "$(dirname "$D")/ffprobe" ffprobe
    ;;
  *apple-darwin*)
    # evermeet ships separate zips (Intel). For Apple Silicon supply your own via
    # FFMPEG_URL / FFPROBE_URL if the default arch doesn't match your triple.
    FURL="${FFMPEG_URL:-https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip}"
    PURL="${FFPROBE_URL:-https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip}"
    echo "Downloading $FURL"
    curl -L "$FURL" -o "$TMP/ffmpeg.zip"
    curl -L "$PURL" -o "$TMP/ffprobe.zip"
    unzip -q "$TMP/ffmpeg.zip" -d "$TMP/f"
    unzip -q "$TMP/ffprobe.zip" -d "$TMP/p"
    place "$(find "$TMP/f" -name ffmpeg -type f | head -n1)" ffmpeg
    place "$(find "$TMP/p" -name ffprobe -type f | head -n1)" ffprobe
    ;;
  *)
    echo "Unsupported triple: $TRIPLE" >&2
    exit 1
    ;;
esac

echo "Done. Sidecars are in src-tauri/binaries/ (git-ignored)."

# binaries/ (optional)

sqz **does not bundle FFmpeg**. The app downloads it on first run into the user's
data folder (or uses a binary the user points it at), which keeps the installer
tiny.

This folder is therefore not required for building or shipping. It exists only as
a convenience: during development you may drop `ffmpeg` / `ffprobe` next to the
built executable (or in this folder and let your own tooling copy them) so the
dev app finds them without downloading. The `scripts/fetch-ffmpeg.*` helpers can
fetch a full build if you want to pre-place binaries.

Anything placed here is git-ignored.

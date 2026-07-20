//! Shared decode-to-null corruption probe.
//!
//! Decoding a file to the null muxer under `-xerror` is the one reliable way to
//! catch truncation and mid-stream corruption the container header can't reveal.
//! Two callers rely on it and must never disagree:
//!   - [`verify`](super::verify) checks a freshly *encoded output* before it's
//!     trusted to replace the original;
//!   - the health scan checks a *source file* already in the library.
//!
//! Keeping the decode logic here — instead of duplicating it — is what makes a
//! standalone health scan honest: "corrupt" means exactly what a real run's
//! verify step would call corrupt.

use std::path::Path;
use std::process::Stdio;

use super::config::{VerifyDepth, DECODE_PROBE_SECONDS};
use super::util::command_no_window;

/// Decode one segment of `path` to null, catching corruption. `seek_from_end`
/// (seconds) probes the tail via `-sseof`; `limit` (seconds) bounds the decode.
/// Returns `(ok, detail)` where `ok` reflects ffmpeg's exit code (authoritative
/// under `-xerror`).
fn decode_segment(
    ffmpeg: &Path,
    path: &Path,
    seek_from_end: Option<u32>,
    limit: Option<u32>,
) -> (bool, String) {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(["-v", "error", "-xerror"]);
    // `-sseof` must precede `-i` (it is an input option).
    if let Some(sec) = seek_from_end {
        cmd.args(["-sseof", &format!("-{sec}")]);
    }
    cmd.arg("-i").arg(path);
    if let Some(sec) = limit {
        cmd.args(["-t", &sec.to_string()]);
    }
    // A tail seek (`-sseof`) lands mid-frame in compressed audio (e.g. AC3), so
    // the decoder reports the first partial frame as corrupt and `-xerror`
    // treats it as fatal — a false-positive DecodeError on otherwise-fine
    // media. The tail probe only needs to catch truncated/garbled *video*, so
    // drop audio for seeked decodes. Full audio integrity is the Checksummed
    // depth's job (it decodes every stream from the start, without seeking).
    if seek_from_end.is_some() {
        cmd.arg("-an");
    }
    cmd.args(["-f", "null", "-"]);

    let out = match cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .output()
    {
        Ok(o) => o,
        Err(e) => return (false, format!("decode probe launch failed: {e}")),
    };
    // `-xerror` makes ffmpeg exit non-zero on a real decode error, so the exit
    // code is authoritative. stderr may carry benign warnings on a clean rc=0
    // decode; treating those as fatal wrongly fails good media.
    if out.status.success() {
        return (true, String::new());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stderr = stderr.trim();
    let detail: String = if stderr.is_empty() {
        format!("rc={}", out.status.code().unwrap_or(-1))
    } else {
        stderr.chars().take(400).collect()
    };
    (false, detail)
}

/// Fully decode every stream (video + audio), forcing a hash so each packet is
/// actually read. Stronger than a video-only null decode: catches audio-side
/// corruption too. Used by the `Checksummed` depth.
fn decode_all_streams(ffmpeg: &Path, path: &Path) -> (bool, String) {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(["-v", "error", "-xerror"])
        .arg("-i")
        .arg(path)
        .args(["-map", "0", "-f", "md5", "-"]);
    let out = match cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .output()
    {
        Ok(o) => o,
        Err(e) => return (false, format!("checksum decode launch failed: {e}")),
    };
    if out.status.success() {
        return (true, String::new());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail: String = stderr.trim().chars().take(400).collect();
    (
        false,
        if detail.is_empty() {
            format!("rc={}", out.status.code().unwrap_or(-1))
        } else {
            detail
        },
    )
}

/// Decode-probe a media file at the given depth:
///   - `Fast`: first *and last* N seconds of video (head-only would let mid/tail
///     corruption pass).
///   - `Thorough`: fully decode the video stream.
///   - `Checksummed`: fully decode every stream and hash it.
///
/// Returns `(ok, detail)`; `detail` is prefixed (`head:`/`tail:`) on Fast so a
/// failure says which end broke.
pub fn decode_probe(ffmpeg: &Path, path: &Path, depth: VerifyDepth) -> (bool, String) {
    match depth {
        VerifyDepth::Checksummed => decode_all_streams(ffmpeg, path),
        VerifyDepth::Thorough => decode_segment(ffmpeg, path, None, None),
        VerifyDepth::Fast => {
            let (ok, detail) = decode_segment(ffmpeg, path, None, Some(DECODE_PROBE_SECONDS));
            if !ok {
                return (false, format!("head: {detail}"));
            }
            let (ok, detail) = decode_segment(ffmpeg, path, Some(DECODE_PROBE_SECONDS), None);
            if !ok {
                return (false, format!("tail: {detail}"));
            }
            (true, String::new())
        }
    }
}

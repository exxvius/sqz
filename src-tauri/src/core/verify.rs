//! Verify an encoded output is playable, complete, and actually smaller.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use super::config::{Config, DECODE_PROBE_SECONDS, DURATION_TOLERANCE_FRAC, DURATION_TOLERANCE_S};
use super::probe::{probe, MediaInfo};
use super::util::command_no_window;

/// Distinguish "no real gain" (keep original, expected) from "broken" (failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyReason {
    Ok,
    NoGain,
    Invalid,
    DurationMismatch,
    DecodeError,
}

#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub ok: bool,
    pub reason: VerifyReason,
    pub out_size: u64,
    pub detail: String,
}

impl VerifyResult {
    fn bad(reason: VerifyReason, out_size: u64, detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            reason,
            out_size,
            detail: detail.into(),
        }
    }
}

fn duration_ok(src: Option<f64>, out: Option<f64>) -> bool {
    match (src, out) {
        (Some(s), Some(o)) => {
            let tol = DURATION_TOLERANCE_S.max(s * DURATION_TOLERANCE_FRAC);
            (s - o).abs() <= tol
        }
        // Can't compare; other checks still guard integrity.
        _ => true,
    }
}

/// Decode one segment of the output to null, catching corruption. `seek_from_end`
/// (seconds) probes the tail via `-sseof`; `limit` (seconds) bounds the decode.
/// Returns `(ok, detail)` where `ok` reflects ffmpeg's exit code (authoritative
/// under `-xerror`).
fn decode_segment(
    ffmpeg: &Path,
    out_path: &Path,
    seek_from_end: Option<u32>,
    limit: Option<u32>,
) -> (bool, String) {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(["-v", "error", "-xerror"]);
    // `-sseof` must precede `-i` (it is an input option).
    if let Some(sec) = seek_from_end {
        cmd.args(["-sseof", &format!("-{sec}")]);
    }
    cmd.arg("-i").arg(out_path);
    if let Some(sec) = limit {
        cmd.args(["-t", &sec.to_string()]);
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
    // decode; treating those as fatal wrongly fails good encodes.
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

/// Decode-verify the output. Paranoid decodes the whole file; the fast path
/// decodes both the first *and last* N seconds — checking only the head would let
/// mid/tail corruption pass verification and trigger deletion of a good original.
fn decode_probe(ffmpeg: &Path, cfg: &Config, out_path: &Path) -> (bool, String) {
    if cfg.paranoid {
        return decode_segment(ffmpeg, out_path, None, None);
    }
    let (ok, detail) = decode_segment(ffmpeg, out_path, None, Some(DECODE_PROBE_SECONDS));
    if !ok {
        return (false, format!("head: {detail}"));
    }
    let (ok, detail) = decode_segment(ffmpeg, out_path, Some(DECODE_PROBE_SECONDS), None);
    if !ok {
        return (false, format!("tail: {detail}"));
    }
    (true, String::new())
}

/// The four gates, in order: structural probe, duration match, decode probe,
/// size gate. Returns a [`VerifyResult`] the pipeline maps to a manifest status.
pub fn verify_output(
    ffmpeg: &Path,
    ffprobe: &Path,
    cfg: &Config,
    src: &MediaInfo,
    out_path: &Path,
) -> VerifyResult {
    let out_size = match std::fs::metadata(out_path) {
        Ok(m) => m.len(),
        Err(_) => return VerifyResult::bad(VerifyReason::Invalid, 0, "output missing"),
    };
    if out_size == 0 {
        return VerifyResult::bad(VerifyReason::Invalid, 0, "output is empty");
    }

    // 1) Structurally valid with a video stream and readable duration.
    let out_info = match probe(ffprobe, out_path, Duration::from_secs(120)) {
        Ok(i) => i,
        Err(e) => return VerifyResult::bad(VerifyReason::Invalid, out_size, e.to_string()),
    };

    // 2) Duration matches the source within tolerance.
    if !duration_ok(src.duration, out_info.duration) {
        return VerifyResult::bad(
            VerifyReason::DurationMismatch,
            out_size,
            format!("src={:?} out={:?}", src.duration, out_info.duration),
        );
    }

    // 3) Decodes without errors.
    let (ok, detail) = decode_probe(ffmpeg, cfg, out_path);
    if !ok {
        return VerifyResult::bad(VerifyReason::DecodeError, out_size, detail);
    }

    // 4) Size gate: must be at least min_savings smaller, else no real gain.
    if let Some(src_size) = src.size {
        if src_size > 0 && (out_size as f64) > (src_size as f64) * (1.0 - cfg.min_savings) {
            return VerifyResult::bad(
                VerifyReason::NoGain,
                out_size,
                format!("out={out_size} src={src_size}"),
            );
        }
    }

    VerifyResult {
        ok: true,
        reason: VerifyReason::Ok,
        out_size,
        detail: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_within_absolute_tolerance() {
        assert!(duration_ok(Some(100.0), Some(100.5)));
        assert!(duration_ok(Some(100.0), Some(99.2)));
    }

    #[test]
    fn duration_uses_fractional_tolerance_for_long_files() {
        // 10000s * 0.5% = 50s tolerance.
        assert!(duration_ok(Some(10_000.0), Some(10_040.0)));
        assert!(!duration_ok(Some(10_000.0), Some(10_060.0)));
    }

    #[test]
    fn missing_durations_pass() {
        assert!(duration_ok(None, Some(10.0)));
        assert!(duration_ok(Some(10.0), None));
    }
}

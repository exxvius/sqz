//! Progress/outcome reporting boundary — the *only* coupling between the
//! headless engine and a frontend. The Tauri layer implements this trait to
//! emit events to the webview; tests use a no-op.

use serde::Serialize;

/// The outcome of processing one file.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessResult {
    pub path: String,
    pub outcome: Outcome,
    pub saved_bytes: i64,
    pub message: String,
    pub orig_size: Option<u64>,
    pub out_size: Option<u64>,
    /// Output container extension when the file was re-encoded/normalized to a new
    /// container (so the UI can open the current file), else `None`.
    pub out_ext: Option<String>,
}

impl ProcessResult {
    pub fn new(path: &str, outcome: Outcome) -> Self {
        Self {
            path: path.to_string(),
            outcome,
            saved_bytes: 0,
            message: String::new(),
            orig_size: None,
            out_size: None,
            out_ext: None,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }
}

/// Per-file outcome. Some variants are run-local (not persisted as file status).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Normalized,
    SkippedEfficient,
    SkippedMarginal,
    SkippedNoGain,
    /// The pre-encode health gate rejected the source (unreadable/corrupt) — a
    /// deliberate skip, not an encode failure. Terminal, never re-encoded on resume.
    SkippedUnhealthy,
    /// Keep-both mode: the original was left in place while an encoded copy was
    /// written alongside it. This is the terminal record *for the original* (the
    /// copy gets its own `Done` row); it exists so the original isn't re-encoded.
    OriginalKept,
    Failed,
    Cancelled,
    DryRun,
}

impl Outcome {
    /// The manifest status string, or `None` for run-local outcomes that are not
    /// persisted (cancelled / dry_run).
    pub fn manifest_status(self) -> Option<&'static str> {
        use super::manifest::*;
        match self {
            Outcome::Done => Some(STATUS_DONE),
            Outcome::Normalized => Some(STATUS_NORMALIZED),
            Outcome::SkippedEfficient => Some(STATUS_SKIPPED_EFFICIENT),
            Outcome::SkippedMarginal => Some(STATUS_SKIPPED_MARGINAL),
            Outcome::SkippedNoGain => Some(STATUS_SKIPPED_NO_GAIN),
            Outcome::SkippedUnhealthy => Some(STATUS_SKIPPED_UNHEALTHY),
            Outcome::OriginalKept => Some(STATUS_KEPT),
            Outcome::Failed => Some(STATUS_FAILED),
            Outcome::Cancelled | Outcome::DryRun => None,
        }
    }
}

/// Sink for live engine progress. Implementations must be cheap and thread-safe;
/// several worker threads call these concurrently.
pub trait Reporter: Send + Sync {
    /// The run is starting with `total` files queued to process.
    fn on_run_start(&self, total: usize);
    /// A file's encode is starting. `duration` seconds, `src_size` bytes.
    fn on_file_start(&self, path: &str, name: &str, duration: Option<f64>, src_size: u64);
    /// One ffmpeg progress tick for the file.
    fn on_file_progress(&self, path: &str, sample: super::encode::ProgressSample);
    /// Progress through the VMAF sample-encode search for `path`, before the real
    /// encode: a fraction 0.0–1.0 that advances continuously. Only fired in VMAF mode.
    fn on_search_progress(&self, path: &str, frac: f64);
    /// Progress decoding `path` through the pre-encode health gate (Deep gate
    /// only), before the encode: a fraction 0.0–1.0. Lets the Live card show a
    /// real bar while the source is checked instead of stalling on a blank panel.
    fn on_health_progress(&self, path: &str, frac: f64);
    /// VMAF mode resolved a per-title CRF for `path` before the full encode.
    /// `vmaf` is the measured score at `crf`, or `None` on a cache hit (no fresh
    /// measurement). Only fired in VMAF quality mode.
    fn on_quality_resolved(&self, path: &str, target: f64, crf: i32, vmaf: Option<f64>);
    /// The file's active progress bar can be cleared.
    fn on_file_end(&self, path: &str);
    /// A file reached a terminal outcome (append to the event log / stats).
    fn on_record(&self, result: &ProcessResult);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::STATUS_SKIPPED_UNHEALTHY;

    #[test]
    fn skipped_unhealthy_persists_as_its_own_status() {
        assert_eq!(
            Outcome::SkippedUnhealthy.manifest_status(),
            Some(STATUS_SKIPPED_UNHEALTHY)
        );
        // It must be distinct from a plain failure.
        assert_ne!(
            Outcome::SkippedUnhealthy.manifest_status(),
            Outcome::Failed.manifest_status()
        );
    }

    #[test]
    fn skipped_unhealthy_serializes_snake_case() {
        let json = serde_json::to_string(&Outcome::SkippedUnhealthy).unwrap();
        assert_eq!(json, "\"skipped_unhealthy\"");
    }
}

/// A reporter that discards everything (tests, headless batch runs).
pub struct NoopReporter;

impl Reporter for NoopReporter {
    fn on_run_start(&self, _: usize) {}
    fn on_file_start(&self, _: &str, _: &str, _: Option<f64>, _: u64) {}
    fn on_file_progress(&self, _: &str, _: super::encode::ProgressSample) {}
    fn on_search_progress(&self, _: &str, _: f64) {}
    fn on_health_progress(&self, _: &str, _: f64) {}
    fn on_quality_resolved(&self, _: &str, _: f64, _: i32, _: Option<f64>) {}
    fn on_file_end(&self, _: &str) {}
    fn on_record(&self, _: &ProcessResult) {}
}

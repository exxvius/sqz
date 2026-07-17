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
    SkippedEfficient,
    SkippedMarginal,
    SkippedNoGain,
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
            Outcome::SkippedEfficient => Some(STATUS_SKIPPED_EFFICIENT),
            Outcome::SkippedMarginal => Some(STATUS_SKIPPED_MARGINAL),
            Outcome::SkippedNoGain => Some(STATUS_SKIPPED_NO_GAIN),
            Outcome::Failed => Some(STATUS_FAILED),
            Outcome::Cancelled | Outcome::DryRun => None,
        }
    }
}

/// Sink for live engine progress. Implementations must be cheap and thread-safe;
/// several worker threads call these concurrently.
pub trait Reporter: Send + Sync {
    /// A file's encode is starting. `duration` seconds, `src_size` bytes.
    fn on_file_start(&self, path: &str, name: &str, duration: Option<f64>, src_size: u64);
    /// Encode progress tick: `sec` encoded, `out_bytes` written so far.
    fn on_file_progress(&self, path: &str, sec: f64, out_bytes: Option<u64>);
    /// The file's active progress bar can be cleared.
    fn on_file_end(&self, path: &str);
    /// A file reached a terminal outcome (append to the event log / stats).
    fn on_record(&self, result: &ProcessResult);
}

/// A reporter that discards everything (tests, headless batch runs).
pub struct NoopReporter;

impl Reporter for NoopReporter {
    fn on_file_start(&self, _: &str, _: &str, _: Option<f64>, _: u64) {}
    fn on_file_progress(&self, _: &str, _: f64, _: Option<u64>) {}
    fn on_file_end(&self, _: &str) {}
    fn on_record(&self, _: &ProcessResult) {}
}

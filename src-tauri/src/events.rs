//! Tauri event bridge: a [`Reporter`] implementation that emits engine progress
//! to the webview — the single seam between the headless engine and the UI.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::core::encode::ProgressSample;
use crate::core::report::{ProcessResult, Reporter};

// Event names (kept in sync with src/lib/events.ts on the frontend).
pub const EV_FILE_START: &str = "sqz-file-start";
pub const EV_FILE_PROGRESS: &str = "sqz-file-progress";
pub const EV_FILE_END: &str = "sqz-file-end";
pub const EV_FILE_RECORD: &str = "sqz-file-record";
pub const EV_RUN_START: &str = "sqz-run-start";
pub const EV_RUN_DONE: &str = "sqz-run-done";
/// Tier-2 (probe-refined) reclaimable-space projection, emitted after a
/// `project_reclaim` call finishes its background probe pass.
pub const EV_PROJECTION: &str = "sqz-projection";
/// Progress through the VMAF sample-encode search for a file (before its encode).
pub const EV_QUALITY_PROGRESS: &str = "sqz-quality-progress";
/// Progress decoding a source through the in-run health gate (Deep), before its
/// encode. Distinct from `EV_HEALTH_PROGRESS` (the standalone library scan).
pub const EV_GATE_PROGRESS: &str = "sqz-gate-progress";
/// VMAF quality mode resolved a per-title CRF for a file (before its full encode).
pub const EV_QUALITY_RESOLVED: &str = "sqz-quality-resolved";
/// Per-file progress during a library health scan.
pub const EV_HEALTH_PROGRESS: &str = "sqz-health-progress";
/// Emitted once when a health scan finishes (payload: the run's summary).
pub const EV_HEALTH_DONE: &str = "sqz-health-done";
/// Emitted when a run launches, telling the UI whether it's a manual run or an
/// unattended (scheduled) run of a named library, so it can label it and show an
/// auto-paused state. Payload: [`RunSourceInfo`].
pub const EV_RUN_SOURCE: &str = "sqz-run-source";

#[derive(Debug, Clone, Serialize)]
pub struct RunSourceInfo {
    /// "manual" or "unattended".
    pub source: String,
    /// The watched library's id/name, present only for unattended runs.
    pub library_id: Option<String>,
    pub library_name: Option<String>,
}

/// Emitted when the supervisor auto-pauses or auto-resumes an unattended run in
/// response to the machine becoming active/idle, so the UI reflects the state the
/// user didn't set by hand.
pub const EV_RUN_PAUSED: &str = "sqz-run-paused";

#[derive(Debug, Clone, Serialize)]
pub struct RunPaused {
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStart {
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileStart {
    pub path: String,
    pub name: String,
    pub duration: Option<f64>,
    pub src_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileProgress {
    pub path: String,
    pub sec: f64,
    pub out_bytes: Option<u64>,
    pub fps: Option<f64>,
    pub speed: Option<f64>,
    pub bitrate_kbps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEnd {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualityProgress {
    pub path: String,
    /// Search progress, 0.0–1.0.
    pub frac: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateProgress {
    pub path: String,
    /// Health-check decode progress, 0.0–1.0.
    pub frac: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualityResolved {
    pub path: String,
    pub target: f64,
    pub crf: i32,
    /// Measured VMAF at `crf`, or `None` on a cache hit.
    pub vmaf: Option<f64>,
}

/// Emits engine callbacks as Tauri events. Cheap and thread-safe (`AppHandle`
/// is `Clone + Send + Sync`), so many workers can report concurrently.
pub struct TauriReporter {
    app: AppHandle,
}

impl TauriReporter {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Reporter for TauriReporter {
    fn on_run_start(&self, total: usize) {
        let _ = self.app.emit(EV_RUN_START, RunStart { total });
    }

    fn on_file_start(&self, path: &str, name: &str, duration: Option<f64>, src_size: u64) {
        let _ = self.app.emit(
            EV_FILE_START,
            FileStart {
                path: path.to_string(),
                name: name.to_string(),
                duration,
                src_size,
            },
        );
    }

    fn on_file_progress(&self, path: &str, sample: ProgressSample) {
        let _ = self.app.emit(
            EV_FILE_PROGRESS,
            FileProgress {
                path: path.to_string(),
                sec: sample.sec,
                out_bytes: sample.out_bytes,
                fps: sample.fps,
                speed: sample.speed,
                bitrate_kbps: sample.bitrate_kbps,
            },
        );
    }

    fn on_search_progress(&self, path: &str, frac: f64) {
        let _ = self.app.emit(
            EV_QUALITY_PROGRESS,
            QualityProgress {
                path: path.to_string(),
                frac,
            },
        );
    }

    fn on_health_progress(&self, path: &str, frac: f64) {
        let _ = self.app.emit(
            EV_GATE_PROGRESS,
            GateProgress {
                path: path.to_string(),
                frac,
            },
        );
    }

    fn on_quality_resolved(&self, path: &str, target: f64, crf: i32, vmaf: Option<f64>) {
        let _ = self.app.emit(
            EV_QUALITY_RESOLVED,
            QualityResolved {
                path: path.to_string(),
                target,
                crf,
                vmaf,
            },
        );
    }

    fn on_file_end(&self, path: &str) {
        let _ = self.app.emit(
            EV_FILE_END,
            FileEnd {
                path: path.to_string(),
            },
        );
    }

    fn on_record(&self, result: &ProcessResult) {
        let _ = self.app.emit(EV_FILE_RECORD, result.clone());
    }
}

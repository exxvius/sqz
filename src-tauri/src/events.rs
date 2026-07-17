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

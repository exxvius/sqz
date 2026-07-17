//! Tauri command surface + shared app state. Commands are thin: they validate,
//! offload engine work to a blocking thread, and steer the active run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::config::{Config, OnSuccess, HOLDING_DIRNAME};
use crate::core::discover::discover;
use crate::core::encoders::{self, Detection};
use crate::core::ffbin::FfBin;
use crate::core::manifest::Manifest;
use crate::events::{TauriReporter, EV_RUN_DONE, EV_RUN_START};
use crate::run::{run, RunSummary};

/// Per-run steering flags shared between a command handler and the run thread.
pub struct RunControl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// Global app state managed by Tauri.
pub struct AppState {
    pub ff: FfBin,
    pub data_dir: PathBuf,
    pub run: Mutex<Option<RunControl>>,
}

impl AppState {
    pub fn new(ff: FfBin, data_dir: PathBuf) -> Self {
        Self {
            ff,
            data_dir,
            run: Mutex::new(None),
        }
    }

    fn db_path(&self) -> PathBuf {
        self.data_dir.join("sqz.db")
    }

    fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }
}

const VALIDATE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
pub struct FfStatus {
    pub present: bool,
    pub ffmpeg: String,
    pub ffprobe: String,
}

#[tauri::command]
pub fn ffmpeg_status(state: State<'_, AppState>) -> FfStatus {
    FfStatus {
        present: state.ff.is_present(),
        ffmpeg: state.ff.ffmpeg.to_string_lossy().into_owned(),
        ffprobe: state.ff.ffprobe.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub async fn detect_encoders(state: State<'_, AppState>) -> Result<Detection, String> {
    let ff = state.ff.clone();
    tauri::async_runtime::spawn_blocking(move || encoders::detect(&ff.ffmpeg, VALIDATE_TIMEOUT))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub count: usize,
    pub total_bytes: u64,
}

#[tauri::command]
pub async fn scan_inputs(inputs: Vec<String>) -> Result<ScanResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = Config {
            inputs: inputs.into_iter().map(PathBuf::from).collect(),
            ..Config::default()
        };
        let files = discover(&cfg);
        let total_bytes = files
            .iter()
            .filter_map(|f| std::fs::metadata(f).ok())
            .map(|m| m.len())
            .sum();
        ScanResult {
            count: files.len(),
            total_bytes,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Inject the managed db path and a default holding dir, then validate.
fn finalize_config(state: &AppState, mut cfg: Config) -> Result<Config, String> {
    cfg.db_path = Some(state.db_path());
    if matches!(cfg.on_success, OnSuccess::Holding) && cfg.holding_dir.is_none() {
        cfg.holding_dir = Some(state.data_dir.join(HOLDING_DIRNAME));
    }
    cfg.validate()?;
    Ok(cfg)
}

#[tauri::command]
pub fn start_run(app: AppHandle, config: Config, state: State<'_, AppState>) -> Result<(), String> {
    {
        let guard = state.run.lock().unwrap();
        if guard.is_some() {
            return Err("A run is already in progress.".into());
        }
    }

    let cfg = finalize_config(&state, config)?;

    // Resolve the encoder up front so failures surface before the run starts.
    let encoder = encoders::select(
        &state.ff.ffmpeg,
        cfg.codec,
        cfg.encoder_override.as_deref(),
        VALIDATE_TIMEOUT,
    )
    .ok_or_else(|| {
        format!(
            "No usable {:?} encoder found on this machine (checked hardware and software).",
            cfg.codec
        )
    })?;

    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    *state.run.lock().unwrap() = Some(RunControl {
        cancel: Arc::clone(&cancel),
        paused: Arc::clone(&paused),
    });

    let ff = state.ff.clone();
    let db_path = state.db_path();
    let app_for_thread = app.clone();

    std::thread::spawn(move || {
        let _ = app_for_thread.emit(EV_RUN_START, ());

        let manifest = match Manifest::open(&db_path) {
            Ok(m) => m,
            Err(e) => {
                let _ = app_for_thread.emit(EV_RUN_DONE, RunError::new(e.to_string()));
                clear_run(&app_for_thread);
                return;
            }
        };
        let reporter = TauriReporter::new(app_for_thread.clone());

        let summary = run(&ff, &cfg, &encoder, &manifest, &reporter, &cancel, &paused);

        notify_done(&app_for_thread, &summary);
        let _ = app_for_thread.emit(EV_RUN_DONE, &summary);
        clear_run(&app_for_thread);
    });

    Ok(())
}

/// Clear the active-run slot so a new run can start.
fn clear_run(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.run.lock().unwrap() = None;
    }
}

#[derive(Debug, Clone, Serialize)]
struct RunError {
    error: String,
}
impl RunError {
    fn new(e: String) -> Self {
        Self { error: e }
    }
}

fn notify_done(app: &AppHandle, summary: &RunSummary) {
    use tauri_plugin_notification::NotificationExt;
    if summary.interrupted {
        return;
    }
    let body = format!(
        "{} re-encoded · {} reclaimed",
        summary.done,
        crate::core::util::human_bytes(summary.saved_bytes.max(0) as f64)
    );
    let _ = app
        .notification()
        .builder()
        .title("sqz — run complete")
        .body(body)
        .show();
}

#[tauri::command]
pub fn pause_run(state: State<'_, AppState>) -> Result<(), String> {
    match &*state.run.lock().unwrap() {
        Some(rc) => {
            rc.paused.store(true, Ordering::Relaxed);
            Ok(())
        }
        None => Err("No run in progress.".into()),
    }
}

#[tauri::command]
pub fn resume_run(state: State<'_, AppState>) -> Result<(), String> {
    match &*state.run.lock().unwrap() {
        Some(rc) => {
            rc.paused.store(false, Ordering::Relaxed);
            Ok(())
        }
        None => Err("No run in progress.".into()),
    }
}

#[tauri::command]
pub fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
    match &*state.run.lock().unwrap() {
        Some(rc) => {
            rc.cancel.store(true, Ordering::Relaxed);
            Ok(())
        }
        None => Err("No run in progress.".into()),
    }
}

#[tauri::command]
pub fn is_running(state: State<'_, AppState>) -> bool {
    state.run.lock().unwrap().is_some()
}

#[derive(Debug, Clone, Serialize)]
pub struct History {
    pub total_saved: i64,
    pub counts: HashMap<String, i64>,
    pub recent: Vec<crate::core::manifest::HistoryRow>,
}

#[tauri::command]
pub async fn get_history(state: State<'_, AppState>) -> Result<History, String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        Ok(History {
            total_saved: m.total_saved_bytes().map_err(|e| e.to_string())?,
            counts: m.status_counts().map_err(|e| e.to_string())?,
            recent: m.recent_done(200).map_err(|e| e.to_string())?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> serde_json::Value {
    std::fs::read_to_string(state.settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({}))
}

#[tauri::command]
pub fn save_settings(settings: serde_json::Value, state: State<'_, AppState>) -> Result<(), String> {
    let path = state.settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

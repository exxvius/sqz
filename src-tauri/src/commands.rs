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
use crate::core::manifest::{HistoryQuery, HistoryRow, Manifest};
use crate::events::{TauriReporter, EV_RUN_DONE};
use crate::run::{run, ActiveMap, RunSummary};

/// Per-run steering flags shared between a command handler and the run thread.
pub struct RunControl {
    pub cancel: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
}

/// Global app state managed by Tauri.
pub struct AppState {
    pub data_dir: PathBuf,
    /// Resolved FFmpeg binaries; refreshed after a download / path change.
    pub ff: Mutex<FfBin>,
    pub run: Mutex<Option<RunControl>>,
    /// Cancel tokens for files currently being processed (for per-file abort).
    pub active: ActiveMap,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let ff = crate::ffsetup::current(&data_dir);
        Self {
            data_dir,
            ff: Mutex::new(ff),
            run: Mutex::new(None),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn ff(&self) -> FfBin {
        self.ff.lock().unwrap().clone()
    }

    fn refresh_ff(&self) {
        *self.ff.lock().unwrap() = crate::ffsetup::current(&self.data_dir);
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
    /// Where the binaries came from: none | custom | managed | system.
    pub source: String,
}

#[tauri::command]
pub fn ffmpeg_status(state: State<'_, AppState>) -> FfStatus {
    let ff = state.ff();
    let present = ff.is_present();
    let cfg = crate::ffsetup::load_config(&state.data_dir);
    let managed = crate::core::ffbin::managed_dir(&state.data_dir);
    let source = if !present {
        "none"
    } else if cfg.ffmpeg.is_some() {
        "custom"
    } else if ff.ffmpeg.starts_with(&managed) {
        "managed"
    } else {
        "system"
    };
    FfStatus {
        present,
        ffmpeg: ff.ffmpeg.to_string_lossy().into_owned(),
        ffprobe: ff.ffprobe.to_string_lossy().into_owned(),
        source: source.into(),
    }
}

/// Download FFmpeg + FFprobe into the app data dir (emits progress events).
#[tauri::command]
pub async fn download_ffmpeg(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let data_dir = state.data_dir.clone();
    let a = app.clone();
    let res = tauri::async_runtime::spawn_blocking(move || crate::ffsetup::download(&a, &data_dir))
        .await
        .map_err(|e| e.to_string())?;
    res?;
    state.refresh_ff();
    Ok(())
}

/// Point sqz at the user's own ffmpeg/ffprobe binaries.
#[tauri::command]
pub async fn set_ffmpeg_paths(
    ffmpeg: String,
    ffprobe: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let data_dir = state.data_dir.clone();
    let res = tauri::async_runtime::spawn_blocking(move || {
        crate::ffsetup::set_custom(&data_dir, &ffmpeg, &ffprobe)
    })
    .await
    .map_err(|e| e.to_string())?;
    res?;
    state.refresh_ff();
    Ok(())
}

/// Forget custom binary paths (revert to managed / PATH resolution).
#[tauri::command]
pub fn clear_ffmpeg_override(state: State<'_, AppState>) -> Result<(), String> {
    crate::ffsetup::clear_custom(&state.data_dir)?;
    state.refresh_ff();
    Ok(())
}

/// Open a file with the OS default application.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    opener::open(&path).map_err(|e| e.to_string())
}

/// Reveal a file in the OS file manager (Explorer / Finder / Files).
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    opener::reveal(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn detect_encoders(state: State<'_, AppState>) -> Result<Detection, String> {
    let ff = state.ff();
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
    let ff = state.ff();
    if !ff.is_present() {
        return Err("FFmpeg isn't set up yet. Add it from Settings first.".into());
    }

    // Resolve the encoder up front so failures surface before the run starts.
    let encoder = encoders::select(
        &ff.ffmpeg,
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

    let db_path = state.db_path();
    let active = Arc::clone(&state.active);
    let app_for_thread = app.clone();

    std::thread::spawn(move || {
        let manifest = match Manifest::open(&db_path) {
            Ok(m) => m,
            Err(e) => {
                let _ = app_for_thread.emit(EV_RUN_DONE, RunError::new(e.to_string()));
                clear_run(&app_for_thread);
                return;
            }
        };
        let reporter = TauriReporter::new(app_for_thread.clone());

        let summary = run(
            &ff, &cfg, &encoder, &manifest, &reporter, &cancel, &paused, &active,
        );

        active.lock().unwrap().clear();
        notify_done(&app_for_thread, &summary);
        let _ = app_for_thread.emit(EV_RUN_DONE, &summary);
        clear_run(&app_for_thread);
    });

    Ok(())
}

/// Abort a single file that's currently being processed (leaves the run going).
#[tauri::command]
pub fn abort_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    match state.active.lock().unwrap().get(&path) {
        Some(token) => {
            token.store(true, Ordering::Relaxed);
            Ok(())
        }
        None => Err("That file isn't currently being processed.".into()),
    }
}

/// Re-queue a file for processing (retry). Works while a run is active.
#[tauri::command]
pub async fn retry_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    requeue(state.db_path(), path, false).await
}

/// Re-queue a file, forcing it past the skip/abort checks.
#[tauri::command]
pub async fn force_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    requeue(state.db_path(), path, true).await
}

async fn requeue(db: PathBuf, path: String, force: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        m.requeue(&path, force).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct HistoryFilter {
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

impl From<HistoryFilter> for HistoryQuery {
    fn from(f: HistoryFilter) -> Self {
        HistoryQuery {
            statuses: f.statuses,
            search: f.search,
            limit: f.limit,
            offset: f.offset,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct History {
    pub total_reclaimed: i64,
    pub counts: HashMap<String, i64>,
    pub rows: Vec<HistoryRow>,
}

#[tauri::command]
pub async fn get_history(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<History, String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        Ok(History {
            total_reclaimed: m.total_reclaimed().map_err(|e| e.to_string())?,
            counts: m.status_counts().map_err(|e| e.to_string())?,
            rows: m.history(&filter.into()).map_err(|e| e.to_string())?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_history_item(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        Manifest::open(&db)
            .map_err(|e| e.to_string())?
            .delete_one(&path)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_history_matching(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        Manifest::open(&db)
            .map_err(|e| e.to_string())?
            .delete_matching(&filter.into())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        Manifest::open(&db)
            .map_err(|e| e.to_string())?
            .clear()
            .map_err(|e| e.to_string())
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

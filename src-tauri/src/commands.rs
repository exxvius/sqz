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
use crate::core::lock::Lock;
use crate::core::manifest::{HistoryQuery, HistoryRow, Manifest};
use crate::core::paths::holding_path_for;
use crate::core::util::command_no_window;
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
    /// Password-gated lock: masks personal info and makes the app read-only.
    pub lock: Lock,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let ff = crate::ffsetup::current(&data_dir);
        let lock = Lock::load(&data_dir);
        Self {
            data_dir,
            ff: Mutex::new(ff),
            run: Mutex::new(None),
            active: Arc::new(Mutex::new(HashMap::new())),
            lock,
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

/// Reject an action that must not run while the app is locked. This is the real
/// gate — the UI disables these controls too, but that guard is cosmetic and
/// trivially bypassed over the IPC boundary.
fn guard_locked(state: &AppState) -> Result<(), String> {
    if state.lock.is_locked() {
        return Err("This action is disabled while the app is locked.".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct LockStatus {
    pub configured: bool,
    pub locked: bool,
}

#[tauri::command]
pub fn lock_status(state: State<'_, AppState>) -> LockStatus {
    let (configured, locked) = state.lock.status();
    LockStatus { configured, locked }
}

#[tauri::command]
pub fn lock_setup(password: String, state: State<'_, AppState>) -> Result<(), String> {
    state.lock.setup(&password)
}

#[tauri::command]
pub fn lock_app(state: State<'_, AppState>) -> Result<(), String> {
    state.lock.engage()
}

#[tauri::command]
pub fn unlock_app(password: String, state: State<'_, AppState>) -> Result<(), String> {
    state.lock.release(&password)
}

#[tauri::command]
pub fn lock_change_password(
    old_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.lock.change_password(&old_password, &new_password)
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
    crate::ffsetup::clear_custom(&state.data_dir)?;
    state.refresh_ff();
    Ok(())
}

/// Open a file with the OS default application.
#[tauri::command]
pub fn open_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    opener::open(&path).map_err(|e| e.to_string())
}

/// Reveal a file in the OS file manager (Explorer / Finder / Files).
#[tauri::command]
pub fn reveal_path(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
    requeue(state.db_path(), path, false).await
}

/// Re-queue a file, forcing it past the skip/abort checks.
#[tauri::command]
pub async fn force_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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

/// Quit the whole app (used by the "quit anyway" close-warning action).
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
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
    /// Wall-clock seconds spent on real re-encodes.
    pub encode_seconds: f64,
    pub files_encoded: i64,
    pub files_touched: i64,
    pub bytes_in: i64,
    pub bytes_out: i64,
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
        let s = m.stats().map_err(|e| e.to_string())?;
        Ok(History {
            total_reclaimed: s.total_reclaimed,
            encode_seconds: s.encode_seconds,
            files_encoded: s.files_encoded,
            files_touched: s.files_touched,
            bytes_in: s.bytes_in,
            bytes_out: s.bytes_out,
            counts: m.status_counts().map_err(|e| e.to_string())?,
            rows: m.history(&filter.into()).map_err(|e| e.to_string())?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_history_item(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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
    guard_locked(&state)?;
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

/// Holding dir the user configured (from persisted settings), or the default.
fn configured_holding_dir(state: &AppState) -> PathBuf {
    let settings = read_settings(state);
    settings
        .get("holding_dir")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| state.data_dir.join(HOLDING_DIRNAME))
}

fn read_settings(state: &AppState) -> serde_json::Value {
    std::fs::read_to_string(state.settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Move a file, tolerating a cross-volume destination (rename → copy+remove).
fn move_across(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)
        }
    }
}

/// Undo a completed re-encode by restoring the original from the holding folder.
/// The encoded replacement is sent to the Recycle Bin (recoverable), the original
/// is moved back into place, and the manifest row is cleared. Only works when the
/// original was preserved via Holding mode.
#[tauri::command]
pub async fn restore_original(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    let holding = configured_holding_dir(&state);
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let orig = PathBuf::from(&path);
        // The original was mirrored under holding preserving its full name.
        let cfg = Config {
            holding_dir: Some(holding),
            ..Config::default()
        };
        let stashed = holding_path_for(&orig, &cfg);
        if !stashed.exists() {
            return Err("Original not found in the holding folder (only Holding-mode runs can be undone here).".to_string());
        }
        if orig.exists() {
            return Err("A file already exists at the original path; not overwriting it.".to_string());
        }
        // Recycle the encoded replacement(s) first (recoverable), then restore.
        for ext in ["mkv", "mp4"] {
            let enc = orig.with_extension(ext);
            if enc != orig && enc.exists() {
                trash::delete(&enc).map_err(|e| format!("could not recycle encoded file: {e}"))?;
            }
        }
        move_across(&stashed, &orig).map_err(|e| format!("could not restore original: {e}"))?;
        if let Ok(m) = Manifest::open(&db) {
            let _ = m.delete_one(&path);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write the persisted settings JSON to a user-chosen file (export).
#[tauri::command]
pub fn export_settings(dest: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    let settings = read_settings(&state);
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&dest, text).map_err(|e| e.to_string())
}

/// Load settings JSON from a file, replacing the current settings (import).
/// Returns the imported object so the UI can apply it immediately.
#[tauri::command]
pub fn import_settings(
    src: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    guard_locked(&state)?;
    let text = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| "That file isn't valid settings JSON.".to_string())?;
    if !value.is_object() {
        return Err("Settings file must contain a JSON object.".into());
    }
    let path = state.settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    Ok(value)
}

/// Export history rows (respecting the filter) to CSV or JSON at `dest`.
#[tauri::command]
pub async fn export_history(
    dest: String,
    format: String,
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    guard_locked(&state)?;
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        let mut q: HistoryQuery = filter.into();
        q.limit = 0; // no cap on export
        let rows = m.history(&q).map_err(|e| e.to_string())?;
        let text = if format.eq_ignore_ascii_case("json") {
            serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
        } else {
            history_to_csv(&rows)
        };
        std::fs::write(&dest, text).map_err(|e| e.to_string())?;
        Ok(rows.len())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn history_to_csv(rows: &[HistoryRow]) -> String {
    let mut out = String::from(
        "path,status,size,src_codec,height,out_size,saved_bytes,updated_at,error\n",
    );
    let num = |v: Option<i64>| v.map(|n| n.to_string()).unwrap_or_default();
    let unum = |v: Option<u64>| v.map(|n| n.to_string()).unwrap_or_default();
    for r in rows {
        let line = [
            csv_escape(&r.path),
            r.status.clone(),
            unum(r.size),
            csv_escape(r.src_codec.as_deref().unwrap_or("")),
            r.height.map(|h| h.to_string()).unwrap_or_default(),
            unum(r.out_size),
            num(r.saved_bytes),
            r.updated_at.map(|t| t.to_string()).unwrap_or_default(),
            csv_escape(r.error.as_deref().unwrap_or("")),
        ]
        .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Pre-flight environment report for the diagnostics panel.
#[derive(Debug, Clone, Serialize)]
pub struct EnvInfo {
    pub os: String,
    pub arch: String,
    pub cpus: usize,
    pub locale: String,
    pub ffmpeg_present: bool,
    pub ffmpeg_path: String,
    pub ffmpeg_version: Option<String>,
    pub detection: Option<Detection>,
}

/// Gather host/tooling capabilities so the user can see what sqz detected
/// (rather than trusting it blindly): OS, cores, locale, FFmpeg build + encoders.
#[tauri::command]
pub async fn environment(state: State<'_, AppState>) -> Result<EnvInfo, String> {
    let ff = state.ff();
    let present = ff.is_present();
    tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg_version = if present { ffmpeg_version(&ff.ffmpeg) } else { None };
        let detection = present.then(|| encoders::detect(&ff.ffmpeg, VALIDATE_TIMEOUT));
        EnvInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpus: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            locale: detect_locale(),
            ffmpeg_present: present,
            ffmpeg_path: ff.ffmpeg.to_string_lossy().into_owned(),
            ffmpeg_version,
            detection,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// First line of `ffmpeg -version` (e.g. "ffmpeg version n7.0 …"), best-effort.
fn ffmpeg_version(ffmpeg: &std::path::Path) -> Option<String> {
    let out = command_no_window(ffmpeg)
        .args(["-hide_banner", "-version"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string())
}

fn detect_locale() -> String {
    for var in ["LC_ALL", "LC_NUMERIC", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    "system default".into()
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
    guard_locked(&state)?;
    let path = state.settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

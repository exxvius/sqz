//! Tauri command surface + shared app state. Commands are thin: they validate,
//! offload engine work to a blocking thread, and steer the active run.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::core::config::{Config, OnSuccess, HOLDING_DIRNAME};
use crate::core::decode::decode_probe;
use crate::core::discover::discover;
use crate::core::encoders::{self, Detection};
use crate::core::estimate::{self, ProbedFile, ReclaimProjection};
use crate::core::ffbin::FfBin;
use crate::core::fswatch::{self, FsWatch};
use crate::core::health::{classify, HealthState};
use crate::core::idle;
use crate::core::library::{self, SavedLibrary, Trigger};
use crate::core::lock::Lock;
use crate::core::manifest::{mtime_secs, HistoryQuery, HistoryRow, LibraryRow, Manifest};
use crate::core::probe::{probe, probe_many};
use crate::core::schedule::{self, AutomationSettings};
use crate::core::util::command_no_window;
use crate::events::{
    RunPaused, RunSourceInfo, TauriReporter, EV_HEALTH_DONE, EV_HEALTH_PROGRESS, EV_PROJECTION,
    EV_RUN_DONE, EV_RUN_PAUSED, EV_RUN_SOURCE,
};
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
    /// The last finalized run config. Retained so retry/force can resume a run
    /// with the same settings when none is active (the workers only exist during a
    /// run, so re-queuing after one finishes needs a run to pick the file up).
    pub last_config: Mutex<Option<Config>>,
    /// Cancel tokens for files currently being processed (for per-file abort).
    pub active: ActiveMap,
    /// Cancel token for the in-flight reclaimable-space projection (Tier-2 probe
    /// pass). A new projection request cancels the previous one.
    pub projection: Mutex<Option<Arc<AtomicBool>>>,
    /// Cancel token for the in-flight library health scan. A new scan cancels the
    /// previous one, so a changing input set never leaves two scans racing.
    pub health: Mutex<Option<Arc<AtomicBool>>>,
    /// Library id of an in-flight *unattended* run (`None` for manual/idle). Lets
    /// the supervisor know which run it owns and whose `idle_only` to honor.
    pub automation_active: Mutex<Option<String>>,
    /// Filesystem-event dirtiness for `OnChange` libraries (library id → unix secs
    /// of the last event), stamped by the watcher and consumed by the supervisor.
    pub fs_dirty: fswatch::DirtyMap,
    /// The live filesystem watcher, rebuilt when the watched root set changes.
    pub fs_watch: Mutex<Option<FsWatch>>,
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
            last_config: Mutex::new(None),
            active: Arc::new(Mutex::new(HashMap::new())),
            projection: Mutex::new(None),
            health: Mutex::new(None),
            automation_active: Mutex::new(None),
            fs_dirty: Arc::new(Mutex::new(HashMap::new())),
            fs_watch: Mutex::new(None),
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

    fn libraries_path(&self) -> PathBuf {
        self.data_dir.join("libraries.json")
    }

    fn watch_state_path(&self) -> PathBuf {
        self.data_dir.join("watch_state.json")
    }

    fn automation_path(&self) -> PathBuf {
        self.data_dir.join("automation.json")
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

/// Estimate how much space a run over `config.inputs` would reclaim.
///
/// Returns immediately with a Tier-1 (instant) projection from the manifest's
/// global savings ratio, then spawns a bounded, cancellable probe pass that
/// emits a refined Tier-2 projection via the `sqz-projection` event. A fresh
/// call cancels any in-flight probe pass, so a changing input set never races.
#[tauri::command]
pub async fn project_reclaim(
    app: AppHandle,
    config: Config,
    state: State<'_, AppState>,
) -> Result<ReclaimProjection, String> {
    let db = state.db_path();
    let ff = state.ff();

    // Install a fresh cancel token, cancelling any previous projection.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.projection.lock().unwrap();
        if let Some(prev) = guard.take() {
            prev.store(true, Ordering::Relaxed);
        }
        *guard = Some(Arc::clone(&cancel));
    }

    // Tier 1 (instant): discover + size the inputs, apply the global ratio.
    let (tier1_proj, sized) = {
        let cfg = config.clone();
        let db = db.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let sized: Vec<(PathBuf, u64)> = discover(&cfg)
                .into_iter()
                .filter_map(|p| std::fs::metadata(&p).ok().map(|m| (p, m.len())))
                .collect();
            let total_bytes: u64 = sized.iter().map(|(_, s)| s).sum();
            let global = Manifest::open(&db)
                .map_err(|e| e.to_string())?
                .global_savings_ratio()
                .map_err(|e| e.to_string())?;
            let proj = estimate::tier1(sized.len() as u32, total_bytes, global, cfg.codec);
            Ok((proj, sized))
        })
        .await
        .map_err(|e| e.to_string())??
    };

    // Tier 2 (background): probe → per-bucket refine → emit. Skipped when there's
    // nothing to probe or FFmpeg isn't available (Tier 1 already stands alone).
    if !sized.is_empty() && ff.is_present() {
        let cfg = config;
        std::thread::spawn(move || {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let manifest = match Manifest::open(&db) {
                Ok(m) => m,
                Err(_) => return,
            };
            let global = manifest.global_savings_ratio().ok().flatten();
            let raw = manifest.bucket_savings_ratios().unwrap_or_default();
            let bucket_ratios = estimate::aggregate_bucket_ratios(&raw);

            let paths: Vec<PathBuf> = sized.iter().map(|(p, _)| p.clone()).collect();
            let infos = probe_many(&ff.ffprobe, &paths, cfg.resolved_workers(), &cancel);
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let rows: Vec<ProbedFile> = sized
                .iter()
                .zip(infos.iter())
                .map(|((_, bytes), info)| match info {
                    Some(mi) => ProbedFile {
                        src_codec: mi.codec.clone().unwrap_or_else(|| "unknown".into()),
                        height_band: mi
                            .height
                            .map(estimate::height_band)
                            .unwrap_or("unknown")
                            .to_string(),
                        bytes: *bytes,
                        skip: estimate::predict_skip(&cfg, mi, cfg.force).is_some(),
                    },
                    None => ProbedFile {
                        src_codec: "unknown".into(),
                        height_band: "unknown".into(),
                        bytes: *bytes,
                        skip: false,
                    },
                })
                .collect();

            let proj = estimate::tier2(&rows, global, &bucket_ratios, cfg.codec);
            if !cancel.load(Ordering::Relaxed) {
                let _ = app.emit(EV_PROJECTION, &proj);
            }
        });
    }

    Ok(tier1_proj)
}

/// Tally of a health scan's outcomes.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HealthSummary {
    pub scanned: u32,
    pub healthy: u32,
    pub corrupt: u32,
    pub unreadable: u32,
    /// Whether a decode pass ran (deep scan) vs. structural-only.
    pub deep: bool,
    /// True if the scan was superseded/cancelled before finishing.
    pub cancelled: bool,
}

/// Per-file health-scan progress event payload.
#[derive(Debug, Clone, Serialize)]
struct HealthProgress {
    scanned: u32,
    total: u32,
    path: String,
    health: String,
}

/// Scan a library for health without re-encoding anything.
///
/// Discovers `config.inputs`, records each file into the manifest as `indexed`
/// (never queued for encoding), then probes every file for structural validity.
/// When `deep`, it additionally decode-probes each source to catch silent
/// corruption — the expensive path, so it's opt-in.
///
/// Emits `sqz-health-progress` per file and `sqz-health-done` with the summary.
/// A fresh call cancels any in-flight scan, so a changing input set never races.
#[tauri::command]
pub async fn scan_health(
    app: AppHandle,
    config: Config,
    deep: bool,
    state: State<'_, AppState>,
) -> Result<HealthSummary, String> {
    let db = state.db_path();
    let ff = state.ff();
    if !ff.is_present() {
        return Err("FFmpeg isn't set up yet. Add it from Settings first.".into());
    }

    // Install a fresh cancel token, cancelling any previous scan.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.health.lock().unwrap();
        if let Some(prev) = guard.take() {
            prev.store(true, Ordering::Relaxed);
        }
        *guard = Some(Arc::clone(&cancel));
    }

    let cfg = config;
    let workers = cfg.resolved_workers();
    tauri::async_runtime::spawn_blocking(move || -> Result<HealthSummary, String> {
        let manifest = Manifest::open(&db).map_err(|e| e.to_string())?;

        // Discover + register every file as indexed (leaves existing rows alone).
        let sized: Vec<(PathBuf, u64)> = discover(&cfg)
            .into_iter()
            .filter_map(|p| std::fs::metadata(&p).ok().map(|m| (p, m)))
            .map(|(p, m)| {
                let _ = manifest.upsert_indexed(&p.to_string_lossy(), m.len(), mtime_secs(&m));
                (p, m.len())
            })
            .collect();

        let n = sized.len();
        let total = n as u32;

        if cancel.load(Ordering::Relaxed) || n == 0 {
            let summary = HealthSummary {
                deep,
                cancelled: cancel.load(Ordering::Relaxed),
                ..Default::default()
            };
            let _ = app.emit(EV_HEALTH_DONE, &summary);
            return Ok(summary);
        }

        // Process files in a bounded worker pool: each worker probes (and, on a
        // deep scan, decodes) one file at a time, records its health, and emits
        // progress the moment it finishes — so the bar advances smoothly for both
        // the fast structural pass and the slow deep pass.
        let next = AtomicUsize::new(0);
        let done = AtomicU32::new(0);
        let summary = Mutex::new(HealthSummary {
            deep,
            ..Default::default()
        });
        let pool = workers.max(1).min(n);

        std::thread::scope(|scope| {
            for _ in 0..pool {
                scope.spawn(|| loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let path = &sized[i].0;
                    let path_str = path.to_string_lossy().to_string();

                    let info = probe(&ff.ffprobe, path, Duration::from_secs(60)).ok();
                    // Deep scan decodes each readable source to catch silent corruption.
                    let decoded = match (deep, &info) {
                        (true, Some(_)) => {
                            Some(decode_probe(&ff.ffmpeg, path, cfg.resolved_verify_depth()).0)
                        }
                        _ => None,
                    };
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }

                    let health = classify(info.is_some(), decoded);

                    // The stored detail explains a bad verdict.
                    let detail: Option<String> = match health {
                        HealthState::Corrupt => {
                            Some("decode errors — likely truncated or corrupted".into())
                        }
                        HealthState::Unreadable => Some("ffprobe could not read this file".into()),
                        HealthState::Healthy => None,
                    };
                    let (codec, height) = info
                        .as_ref()
                        .map(|mi| (mi.codec.clone(), mi.height))
                        .unwrap_or((None, None));

                    let _ = manifest.record_health(
                        &path_str,
                        health.as_str(),
                        detail.as_deref(),
                        codec.as_deref(),
                        height,
                    );

                    let scanned = done.fetch_add(1, Ordering::Relaxed) + 1;
                    {
                        let mut s = summary.lock().unwrap();
                        s.scanned += 1;
                        match health {
                            HealthState::Healthy => s.healthy += 1,
                            HealthState::Corrupt => s.corrupt += 1,
                            HealthState::Unreadable => s.unreadable += 1,
                        }
                    }
                    let _ = app.emit(
                        EV_HEALTH_PROGRESS,
                        HealthProgress {
                            scanned,
                            total,
                            path: path_str,
                            health: health.as_str().to_string(),
                        },
                    );
                });
            }
        });

        let mut summary = summary.into_inner().unwrap();
        summary.cancelled = cancel.load(Ordering::Relaxed);
        let _ = app.emit(EV_HEALTH_DONE, &summary);
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The library view: all known files with health + a per-state count summary.
#[derive(Debug, Clone, Serialize)]
pub struct Library {
    /// Counts by health state, with never-scanned files under `"unscanned"`.
    pub counts: HashMap<String, i64>,
    pub rows: Vec<LibraryRow>,
}

#[tauri::command]
pub async fn get_library(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<Library, String> {
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        Ok(Library {
            counts: m.health_counts().map_err(|e| e.to_string())?,
            rows: m.library(&filter.into()).map_err(|e| e.to_string())?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Where a run came from: a user pressing Start, or the supervisor firing a
/// watched library's schedule. Drives the `automation_active` slot and the
/// run-source label the UI shows.
#[derive(Clone)]
enum RunSource {
    Manual,
    Unattended { id: String, name: String },
}

#[tauri::command]
pub fn start_run(app: AppHandle, config: Config, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    let cfg = finalize_config(&state, config)?;
    // Remember the settings so retry/force can resume with them when idle.
    *state.last_config.lock().unwrap() = Some(cfg.clone());
    launch_run(app, cfg, &state, RunSource::Manual)
}

/// Spawn the worker run for a finalized config. Rejects if a run is already
/// active (re-checked here so it's safe to call from retry/force too).
fn launch_run(
    app: AppHandle,
    cfg: Config,
    state: &AppState,
    source: RunSource,
) -> Result<(), String> {
    {
        let guard = state.run.lock().unwrap();
        if guard.is_some() {
            return Err("A run is already in progress.".into());
        }
    }

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

    // Record whether this run is autonomous (so the supervisor knows which run it
    // owns) and label it for the UI.
    *state.automation_active.lock().unwrap() = match &source {
        RunSource::Unattended { id, .. } => Some(id.clone()),
        RunSource::Manual => None,
    };
    emit_run_source(&app, &source);

    let db_path = state.db_path();
    let active = Arc::clone(&state.active);
    let app_for_thread = app.clone();
    // Name the library on the completion notification for unattended runs.
    let done_label = match &source {
        RunSource::Unattended { name, .. } => Some(name.clone()),
        RunSource::Manual => None,
    };

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
        notify_done(&app_for_thread, &summary, done_label.as_deref());
        let _ = app_for_thread.emit(EV_RUN_DONE, &summary);
        clear_run(&app_for_thread);
    });

    Ok(())
}

/// Tell the UI whether the run that just launched is manual or an unattended run
/// of a named library, so it can label progress and show an auto-paused state.
fn emit_run_source(app: &AppHandle, source: &RunSource) {
    let info = match source {
        RunSource::Manual => RunSourceInfo {
            source: "manual".into(),
            library_id: None,
            library_name: None,
        },
        RunSource::Unattended { id, name } => RunSourceInfo {
            source: "unattended".into(),
            library_id: Some(id.clone()),
            library_name: Some(name.clone()),
        },
    };
    let _ = app.emit(EV_RUN_SOURCE, info);
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

/// Re-queue a file for processing (retry). If no run is active, resumes one with
/// the last-used settings so the file actually gets processed.
#[tauri::command]
pub async fn retry_file(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    guard_locked(&state)?;
    requeue(state.db_path(), path, false).await?;
    resume_if_idle(app, &state);
    Ok(())
}

/// Re-queue a file, forcing it past the skip/abort checks. Resumes an idle run too.
#[tauri::command]
pub async fn force_file(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    guard_locked(&state)?;
    requeue(state.db_path(), path, true).await?;
    resume_if_idle(app, &state);
    Ok(())
}

/// After a re-queue, start a run so the newly-pending file is actually picked up.
/// A no-op while a run is live (its workers claim re-queued files). When idle it
/// resumes with the last-used config, or — for a failed file left over from a
/// previous session — the persisted settings (or defaults).
fn resume_if_idle(app: AppHandle, state: &AppState) {
    let cfg = {
        if state.run.lock().unwrap().is_some() {
            return;
        }
        state.last_config.lock().unwrap().clone()
    };
    let cfg = cfg.or_else(|| persisted_config(state));
    if let Some(cfg) = cfg {
        // Best-effort: launch_run re-checks the run slot, so a race just no-ops.
        let _ = launch_run(app, cfg, state, RunSource::Manual);
    }
}

/// Reconstruct a finalized run config from the persisted settings (or defaults if
/// none), so retry works even for a failed file carried over from a past session
/// before any run started. Inputs are empty — the run processes all pending rows
/// regardless, so the re-queued file is still picked up.
fn persisted_config(state: &AppState) -> Option<Config> {
    let cfg = std::fs::read_to_string(state.settings_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .unwrap_or_default();
    finalize_config(state, cfg).ok()
}

async fn requeue(db: PathBuf, path: String, force: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        m.requeue(&path, force).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Clear the active-run slot (and the unattended marker) so a new run can start.
fn clear_run(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        *state.run.lock().unwrap() = None;
        *state.automation_active.lock().unwrap() = None;
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

fn notify_done(app: &AppHandle, summary: &RunSummary, library: Option<&str>) {
    use tauri_plugin_notification::NotificationExt;
    if summary.interrupted {
        return;
    }
    let body = format!(
        "{} re-encoded · {} reclaimed",
        summary.done,
        crate::core::util::human_bytes(summary.saved_bytes.max(0) as f64)
    );
    // Name the library for unattended runs — the user was away, so the toast is
    // how they learn what happened.
    let title = match library {
        Some(name) => format!("sqz — {name} processed"),
        None => "sqz — run complete".to_string(),
    };
    let _ = app.notification().builder().title(title).body(body).show();
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

/// Cancel an in-progress health scan. Flips the scan's cancel token; the scan
/// loop stops promptly and emits its (cancelled) summary. A no-op if no scan is
/// active (it may have just finished).
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    if let Some(token) = state.health.lock().unwrap().as_ref() {
        token.store(true, Ordering::Relaxed);
    }
    Ok(())
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

/// Remove files from the Library health list by path — the library view's
/// "remove" actions. Clears the health annotation (dropping the file from the
/// Library) while keeping any pipeline-history row for the History view; a
/// scan-only row is deleted outright. Never affects History/predictions data.
#[tauri::command]
pub async fn delete_library_paths(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    guard_locked(&state)?;
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        Manifest::open(&db)
            .map_err(|e| e.to_string())?
            .remove_from_library(&paths)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The saved libraries, in stored order. Read-only; paths are masked by the UI
/// under lock (same as the ad-hoc Library roots), so this stays available locked.
#[tauri::command]
pub fn list_libraries(state: State<'_, AppState>) -> Vec<SavedLibrary> {
    library::load_all(&state.libraries_path())
}

/// Create or update a saved library. Assigns an id/timestamps on first save,
/// strips profile inputs, and validates the encode target before persisting.
/// Echoes the stored row back so the UI learns the assigned id.
#[tauri::command]
pub fn save_library(lib: SavedLibrary, state: State<'_, AppState>) -> Result<SavedLibrary, String> {
    guard_locked(&state)?;
    let normalized = lib.normalized()?;
    let path = state.libraries_path();
    let libs = library::upsert(library::load_all(&path), normalized.clone());
    library::save_all(&path, &libs)?;
    Ok(normalized)
}

/// Delete the saved library with `id`. Never touches the manifest, so a library's
/// scanned/encoded files keep their History and health rows.
#[tauri::command]
pub fn delete_library(id: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    let path = state.libraries_path();
    let libs = library::remove(library::load_all(&path), &id);
    library::save_all(&path, &libs)
}

// ── Unattended automation ──────────────────────────────────────────────────

/// One watched library's automation status, for the Dashboard panel.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationEntry {
    pub library_id: String,
    pub name: String,
    /// "interval" | "daily" | "onchange" — how this library is triggered.
    pub trigger_kind: String,
    /// Whether the library only runs while the machine is idle (so the UI can say
    /// "waiting until you're away" when it's due but the machine is active).
    pub idle_only: bool,
    /// Unix seconds of the next scheduled fire (a due schedule reads as "now").
    /// `None` for `OnChange` — it's event-driven, not scheduled.
    pub next_run_at: Option<f64>,
    /// Unix seconds of the last auto-run, if it has ever fired.
    pub last_auto_run_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationStatus {
    /// The global master switch.
    pub enabled: bool,
    /// Whether the machine is currently input-idle (so the UI can explain an
    /// idle-gated library that's due but held back).
    pub system_idle: bool,
    /// Watched libraries, each with its next/last unattended-run times.
    pub entries: Vec<AutomationEntry>,
}

fn trigger_kind(t: Trigger) -> &'static str {
    match t {
        Trigger::Interval { .. } => "interval",
        Trigger::Daily { .. } => "daily",
        Trigger::OnChange { .. } => "onchange",
    }
}

/// Automation status for the Dashboard: the master switch plus every watched
/// library's next/last run. Read-only, so it stays available under lock (names are
/// labels, not personal paths).
#[tauri::command]
pub fn get_automation(state: State<'_, AppState>) -> AutomationStatus {
    let enabled = schedule::load_automation(&state.automation_path()).enabled;
    let system_idle = idle::is_idle(idle::DEFAULT_IDLE_SECS);
    let libs = library::load_all(&state.libraries_path());
    let watch = schedule::load_state(&state.watch_state_path());
    let now = chrono::Local::now();
    let entries = libs
        .iter()
        .filter(|l| l.watch.enabled)
        .map(|l| {
            let last = watch.last(&l.id);
            // OnChange has no scheduled instant; leave next_run_at empty.
            let next_run_at = match l.watch.trigger {
                Trigger::OnChange { .. } => None,
                t => Some(schedule::next_run_at(t, last, now)),
            };
            AutomationEntry {
                library_id: l.id.clone(),
                name: l.name.clone(),
                trigger_kind: trigger_kind(l.watch.trigger).to_string(),
                idle_only: l.watch.idle_only,
                next_run_at,
                last_auto_run_at: last,
            }
        })
        .collect();
    AutomationStatus {
        enabled,
        system_idle,
        entries,
    }
}

/// Flip the global automation master switch. Off pauses all watching without
/// touching any per-library schedule.
#[tauri::command]
pub fn set_automation_enabled(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    schedule::save_automation(&state.automation_path(), &AutomationSettings { enabled })
}

/// Manually kick a saved library's run now (the automation panel's "Run now").
/// A normal manual run — it does not reset the library's schedule.
#[tauri::command]
pub fn run_library_now(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    guard_locked(&state)?;
    let lib = library::load_all(&state.libraries_path())
        .into_iter()
        .find(|l| l.id == id)
        .ok_or_else(|| "No such library.".to_string())?;
    let mut cfg = lib.profile.clone();
    cfg.inputs = lib.roots.clone();
    let cfg = finalize_config(&state, cfg)?;
    // Running now resets the schedule so the panel shows the next fire (not a
    // stuck "due now") and an idle-gated library won't immediately re-fire.
    let mut watch = schedule::load_state(&state.watch_state_path());
    watch.mark(&id, chrono::Local::now().timestamp() as f64);
    let _ = schedule::save_state(&state.watch_state_path(), &watch);
    state.fs_dirty.lock().unwrap().remove(&id);
    *state.last_config.lock().unwrap() = Some(cfg.clone());
    launch_run(app, cfg, &state, RunSource::Manual)
}

/// The tick interval for the unattended supervisor.
const SUPERVISOR_TICK_SECS: u64 = 60;

/// Spawn the unattended supervisor: a background tick that steers an in-flight
/// unattended run's pause state and launches at most one due library per tick.
/// Cheap and self-contained; started once from `lib.rs` after state is managed.
pub fn spawn_supervisor(app: AppHandle) {
    std::thread::spawn(move || {
        // Let startup (ffmpeg probe, window) settle before the first tick.
        std::thread::sleep(Duration::from_secs(10));
        loop {
            supervisor_tick(&app);
            std::thread::sleep(Duration::from_secs(SUPERVISOR_TICK_SECS));
        }
    });
}

/// One supervisor pass. Pure decisions live in `core::schedule`; this only does the
/// I/O around them (clock, idle probe, launching through the ordinary run path).
fn supervisor_tick(app: &AppHandle) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };
    // Master switch off → nothing to do.
    if !schedule::load_automation(&state.automation_path()).enabled {
        return;
    }
    let system_idle = idle::is_idle(idle::DEFAULT_IDLE_SECS);

    // 1. Steer an in-flight unattended run via its existing paused token, and never
    //    launch over it.
    let active_id = state.automation_active.lock().unwrap().clone();
    if let Some(id) = active_id {
        if let Some(lib) = library::load_all(&state.libraries_path())
            .into_iter()
            .find(|l| l.id == id)
        {
            let pause = schedule::should_pause(&lib, system_idle);
            if let Some(rc) = &*state.run.lock().unwrap() {
                // Only write/emit on a change so the UI toast fires once, not every tick.
                if rc.paused.load(Ordering::Relaxed) != pause {
                    rc.paused.store(pause, Ordering::Relaxed);
                    let _ = app.emit(EV_RUN_PAUSED, RunPaused { paused: pause });
                }
            }
        }
        return;
    }

    // 2. Don't compete with manual work (a run or a health scan always wins).
    if state.run.lock().unwrap().is_some() || state.health.lock().unwrap().is_some() {
        return;
    }

    // 3. Maintain the filesystem watcher for OnChange libraries, then launch at
    //    most one due library (time- or event-triggered), oldest-due first.
    let libs = library::load_all(&state.libraries_path());
    maintain_fs_watch(&state, &libs);
    let fs_ready = fs_ready_set(&state, &libs);

    let mut watch = schedule::load_state(&state.watch_state_path());
    let now = chrono::Local::now();
    let due = schedule::due_libraries(&libs, &watch, now, system_idle, &fs_ready);
    let Some(first) = due.first() else {
        return;
    };
    let Some(lib) = libs.iter().find(|l| l.id == first.library_id) else {
        return;
    };

    let mut cfg = lib.profile.clone();
    cfg.inputs = lib.roots.clone();
    let cfg = match finalize_config(&state, cfg) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("skipping unattended run of {}: {e}", lib.name);
            return;
        }
    };
    // Mark before launching so a crash mid-run can't re-fire instantly on restart,
    // and clear its filesystem dirtiness so a settled OnChange doesn't re-fire.
    watch.mark(&lib.id, now.timestamp() as f64);
    let _ = schedule::save_state(&state.watch_state_path(), &watch);
    state.fs_dirty.lock().unwrap().remove(&lib.id);

    let source = RunSource::Unattended {
        id: lib.id.clone(),
        name: lib.name.clone(),
    };
    let _ = launch_run(app.clone(), cfg, &state, source);
}

/// The `(id, roots)` pairs of enabled `OnChange` libraries — what the filesystem
/// watcher covers.
fn onchange_libs(libs: &[SavedLibrary]) -> Vec<(String, Vec<PathBuf>)> {
    libs.iter()
        .filter(|l| l.watch.enabled && matches!(l.watch.trigger, Trigger::OnChange { .. }))
        .map(|l| (l.id.clone(), l.roots.clone()))
        .collect()
}

/// Rebuild the filesystem watcher whenever the set of watched `OnChange` roots
/// changes; drop stale dirtiness when nothing is watched anymore.
fn maintain_fs_watch(state: &AppState, libs: &[SavedLibrary]) {
    let pairs = onchange_libs(libs);
    let want = fswatch::signature_of(&pairs);
    let mut guard = state.fs_watch.lock().unwrap();
    let cur = guard
        .as_ref()
        .map(|w| w.signature.clone())
        .unwrap_or_default();
    if cur != want {
        *guard = if pairs.is_empty() {
            None
        } else {
            fswatch::start(&pairs, Arc::clone(&state.fs_dirty))
        };
    }
    if pairs.is_empty() {
        state.fs_dirty.lock().unwrap().clear();
    }
}

/// The enabled `OnChange` libraries whose filesystem activity has settled past
/// their debounce — the set the scheduler treats as event-due.
fn fs_ready_set(state: &AppState, libs: &[SavedLibrary]) -> HashSet<String> {
    let now = chrono::Local::now().timestamp() as f64;
    let dirty = state.fs_dirty.lock().unwrap();
    libs.iter()
        .filter_map(|l| match l.watch.trigger {
            Trigger::OnChange { debounce_secs } if l.watch.enabled => dirty
                .get(&l.id)
                .filter(|&&t| now - t >= debounce_secs as f64)
                .map(|_| l.id.clone()),
            _ => None,
        })
        .collect()
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

/// Undo a re-encode by restoring the original from the holding folder: the encoded
/// file is sent to the Recycle Bin (recoverable), the held original is moved back
/// to its original name/location, and the manifest row is dropped. Only Holding-mode
/// rows carry the held/original paths, so only those can be restored here.
#[tauri::command]
pub async fn restore_original(path: String, state: State<'_, AppState>) -> Result<(), String> {
    guard_locked(&state)?;
    let db = state.db_path();
    tauri::async_runtime::spawn_blocking(move || {
        let m = Manifest::open(&db).map_err(|e| e.to_string())?;
        // (held_path, orig_path): where the original sits now, and where it came
        // from. Recorded only for Holding-mode encodes — the one restorable case.
        let (held, orig) = m.restore_paths(&path).ok_or_else(|| {
            "This file's original wasn't kept in a holding folder, so it can't be restored here."
                .to_string()
        })?;
        let held = PathBuf::from(held);
        let orig = PathBuf::from(orig);
        if !held.exists() {
            return Err("Original not found in the holding folder.".to_string());
        }
        if orig.exists() {
            return Err(
                "A file already exists at the original path; not overwriting it.".to_string(),
            );
        }
        // Recycle the encoded file (the row's current path) first (recoverable),
        // then move the held original back to its original name.
        let encoded = PathBuf::from(&path);
        if encoded.exists() {
            trash::delete(&encoded).map_err(|e| format!("could not recycle encoded file: {e}"))?;
        }
        move_across(&held, &orig).map_err(|e| format!("could not restore original: {e}"))?;
        // Keep the row: re-point it at the restored original so its VMAF cache and
        // history survive (a re-encode reuses the cached CRF instead of re-searching).
        let _ = m.revert_to_source(&path, &orig.to_string_lossy());
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
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&value).unwrap_or_default(),
    )
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
    let mut out =
        String::from("path,status,size,src_codec,height,out_size,saved_bytes,updated_at,error\n");
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
        let ffmpeg_version = if present {
            ffmpeg_version(&ff.ffmpeg)
        } else {
            None
        };
        let detection = present.then(|| encoders::detect(&ff.ffmpeg, VALIDATE_TIMEOUT));
        EnvInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpus: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(0),
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
pub fn save_settings(
    settings: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<(), String> {
    guard_locked(&state)?;
    let path = state.settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

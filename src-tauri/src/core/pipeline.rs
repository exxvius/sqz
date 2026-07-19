//! Per-file orchestration: probe → skip? → encode → verify → swap → record.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::abort::{AbortConfig, AbortJudge, AbortProjection};
use super::config::Config;
use super::encode::{build_args, build_remux_args, run_encode, ProgressSample};
use super::encoders::Encoder;
use super::estimate::{predict_skip, SkipKind};
use super::ffbin::FfBin;
use super::manifest::{mtime_secs, Manifest, StatusUpdate};
use super::paths::temp_dir_for;
use super::probe::{probe, MediaInfo};
use super::report::{Outcome, ProcessResult, Reporter};
use super::util::human_bytes;
use super::verify::{verify_output, VerifyReason};

/// True if `path` already uses the run's target container extension.
fn is_target_container(path: &Path, cfg: &Config) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(cfg.container.ext()))
        .unwrap_or(false)
}

fn cleanup(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

fn set(manifest: &Manifest, path: &str, outcome: Outcome, upd: StatusUpdate) {
    if let Some(status) = outcome.manifest_status() {
        let _ = manifest.set_status(path, status, &upd);
    }
}

fn meta_of(info: &MediaInfo) -> StatusUpdate {
    StatusUpdate {
        src_codec: info.codec.clone(),
        height: info.height,
        ..Default::default()
    }
}

/// Record a skip — or, when container normalization is on and the source isn't
/// already `.mkv`, remux it into the target container first.
#[allow(clippy::too_many_arguments)]
fn skip_or_normalize(
    ff: &FfBin,
    cfg: &Config,
    manifest: &Manifest,
    info: &MediaInfo,
    src: &Path,
    path_str: &str,
    size: u64,
    skip: Outcome,
    cancel: &(dyn Fn() -> bool + Sync),
    reporter: &dyn Reporter,
) -> ProcessResult {
    if cfg.normalize_container && !cfg.dry_run && !is_target_container(src, cfg) {
        if let Some(r) = try_remux(
            ff, cfg, manifest, info, src, path_str, size, cancel, reporter,
        ) {
            return r;
        }
    }
    set(manifest, path_str, skip, meta_of(info));
    ProcessResult::new(path_str, skip)
}

/// Remux (stream-copy) a source into the target MKV container. Returns the
/// resulting [`ProcessResult`], or `None` if it couldn't be normalized (caller
/// falls back to recording the plain skip).
#[allow(clippy::too_many_arguments)]
fn try_remux(
    ff: &FfBin,
    cfg: &Config,
    manifest: &Manifest,
    info: &MediaInfo,
    src: &Path,
    path_str: &str,
    size: u64,
    cancel: &(dyn Fn() -> bool + Sync),
    reporter: &dyn Reporter,
) -> Option<ProcessResult> {
    let temp_dir = temp_dir_for(src, cfg).ok()?;
    let out = temp_dir.join(format!(
        "sqz_{}.{}",
        uuid::Uuid::new_v4().simple(),
        cfg.container.ext()
    ));
    let args = build_remux_args(cfg, info, &out);

    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    reporter.on_file_start(path_str, &name, info.duration, size);
    let mut on_progress = |sample: ProgressSample| reporter.on_file_progress(path_str, sample);
    let enc = run_encode(&ff.ffmpeg, &args, cancel, &mut on_progress, None);
    reporter.on_file_end(path_str);

    if enc.cancelled {
        cleanup(&out);
        return Some(ProcessResult::new(path_str, Outcome::Cancelled));
    }
    if enc.returncode != Some(0) {
        cleanup(&out);
        return None; // couldn't remux; keep the original as a plain skip
    }
    let playable = probe(&ff.ffprobe, &out, Duration::from_secs(120))
        .map(|oi| oi.duration.is_some())
        .unwrap_or(false);
    let out_size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    if !playable || out_size == 0 {
        cleanup(&out);
        return None;
    }
    if super::replace::replace_original(cfg, src, &out).is_err() {
        cleanup(&out);
        return None;
    }
    let saved = size as i64 - out_size as i64;
    set(
        manifest,
        path_str,
        Outcome::Normalized,
        StatusUpdate {
            out_size: Some(out_size),
            saved_bytes: Some(saved),
            ..meta_of(info)
        },
    );
    let mut r = ProcessResult::new(path_str, Outcome::Normalized);
    r.saved_bytes = saved;
    r.orig_size = Some(size);
    r.out_size = Some(out_size);
    Some(r)
}

/// Process one file end to end. Returns the run-local [`ProcessResult`]; the
/// durable status is written to the manifest along the way.
#[allow(clippy::too_many_arguments)]
pub fn process_file(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    manifest: &Manifest,
    path_str: &str,
    forced: bool,
    global_cancel: &AtomicBool,
    file_cancel: &AtomicBool,
    reporter: &dyn Reporter,
) -> ProcessResult {
    let src = Path::new(path_str);
    // A per-file force flag (from a "force process" action) OR the run-wide force.
    let force = cfg.force || forced;
    // Stop this file if the whole run is cancelled OR this file was aborted.
    let cancelled = || global_cancel.load(Ordering::Relaxed) || file_cancel.load(Ordering::Relaxed);

    if cancelled() {
        return ProcessResult::new(path_str, Outcome::Cancelled);
    }

    let meta = match std::fs::metadata(src) {
        Ok(m) => m,
        Err(e) => {
            set(
                manifest,
                path_str,
                Outcome::Failed,
                StatusUpdate {
                    error: Some(format!("stat failed: {e}")),
                    ..Default::default()
                },
            );
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };
    let size = meta.len();

    if size == 0 {
        set(
            manifest,
            path_str,
            Outcome::Failed,
            StatusUpdate {
                error: Some("empty file".into()),
                ..Default::default()
            },
        );
        return ProcessResult::new(path_str, Outcome::Failed).with_message("empty file");
    }

    let info = match probe(&ff.ffprobe, src, Duration::from_secs(120)) {
        Ok(i) => i,
        Err(e) => {
            set(
                manifest,
                path_str,
                Outcome::Failed,
                StatusUpdate {
                    error: Some(format!("probe: {e}")),
                    ..Default::default()
                },
            );
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };

    let meta_upd = |o: &MediaInfo| StatusUpdate {
        src_codec: o.codec.clone(),
        height: o.height,
        ..Default::default()
    };

    // Skip checks — shared with the projection via `predict_skip`, so the
    // estimate can never disagree with what a run actually does.
    // • Dolby Vision: re-encoding drops the DV enhancement layer/RPU (a lossless
    //   container-normalize remux still preserves DV), so it records as no-gain.
    // • Already-efficient / marginal both remain their own outcomes.
    match predict_skip(cfg, &info, force) {
        Some(SkipKind::DolbyVision) => {
            return skip_or_normalize(
                ff,
                cfg,
                manifest,
                &info,
                src,
                path_str,
                size,
                Outcome::SkippedNoGain,
                &cancelled,
                reporter,
            );
        }
        Some(SkipKind::AlreadyEfficient) => {
            return skip_or_normalize(
                ff,
                cfg,
                manifest,
                &info,
                src,
                path_str,
                size,
                Outcome::SkippedEfficient,
                &cancelled,
                reporter,
            );
        }
        Some(SkipKind::Marginal) => {
            return skip_or_normalize(
                ff,
                cfg,
                manifest,
                &info,
                src,
                path_str,
                size,
                Outcome::SkippedMarginal,
                &cancelled,
                reporter,
            );
        }
        None => {}
    }

    if cfg.dry_run {
        let msg = format!(
            "would encode ({} {}p, {} bytes)",
            info.codec.as_deref().unwrap_or("?"),
            info.height.unwrap_or(0),
            size
        );
        return ProcessResult::new(path_str, Outcome::DryRun).with_message(msg);
    }

    let temp_dir = match temp_dir_for(src, cfg) {
        Ok(d) => d,
        Err(e) => {
            set(
                manifest,
                path_str,
                Outcome::Failed,
                StatusUpdate {
                    error: Some(format!("temp dir: {e}")),
                    ..meta_upd(&info)
                },
            );
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };

    // NB: we deliberately do not pre-check free space. If the drive fills, the
    // encode simply fails and the original is left untouched (retried next run) —
    // the size gate and verify keep every outcome safe either way.
    let out = temp_dir.join(format!(
        "sqz_{}.{}",
        uuid::Uuid::new_v4().simple(),
        cfg.container.ext()
    ));
    let args = build_args(cfg, &info, encoder, &out);

    // Staged early-abort judge, driven by ffmpeg progress ticks.
    let judge = Mutex::new(AbortJudge::new(AbortConfig::from(cfg, info.duration, size)));
    let abort_pred =
        |sec: f64, out_bytes: Option<u64>| judge.lock().unwrap().observe(sec, out_bytes);

    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    reporter.on_file_start(path_str, &name, info.duration, size);

    let mut on_progress = |sample: ProgressSample| reporter.on_file_progress(path_str, sample);
    let encode_start = std::time::Instant::now();
    let enc = run_encode(
        &ff.ffmpeg,
        &args,
        &cancelled,
        &mut on_progress,
        Some(&abort_pred),
    );
    let encode_ms = encode_start.elapsed().as_millis() as i64;
    reporter.on_file_end(path_str);

    if enc.cancelled {
        cleanup(&out);
        return ProcessResult::new(path_str, Outcome::Cancelled);
    }

    if enc.aborted {
        cleanup(&out);
        let proj = enc.abort_projection.unwrap_or(AbortProjection {
            frac: 0.0,
            projected: 0.0,
        });
        let msg = format!(
            "aborted at {:.0}% — projected {} vs {} original",
            proj.frac * 100.0,
            human_bytes(proj.projected),
            human_bytes(size as f64)
        );
        // Even a no-gain file can be normalized to the target container.
        if cfg.normalize_container && !is_target_container(src, cfg) {
            if let Some(r) = try_remux(
                ff, cfg, manifest, &info, src, path_str, size, &cancelled, reporter,
            ) {
                return r;
            }
        }
        set(
            manifest,
            path_str,
            Outcome::SkippedNoGain,
            StatusUpdate {
                error: Some(msg.clone()),
                ..meta_upd(&info)
            },
        );
        return ProcessResult::new(path_str, Outcome::SkippedNoGain).with_message(msg);
    }

    if enc.returncode != Some(0) {
        cleanup(&out);
        let rc = enc.returncode.unwrap_or(-1);
        let tail: String = enc.stderr_tail.replace('\n', " ");
        let tail_trim: String = tail
            .chars()
            .rev()
            .take(400)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        set(
            manifest,
            path_str,
            Outcome::Failed,
            StatusUpdate {
                error: Some(format!("ffmpeg rc={rc}: {tail_trim}")),
                ..meta_upd(&info)
            },
        );
        let short: String = tail
            .chars()
            .rev()
            .take(160)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return ProcessResult::new(path_str, Outcome::Failed)
            .with_message(format!("ffmpeg rc={rc}: {short}"));
    }

    let vr = verify_output(&ff.ffmpeg, &ff.ffprobe, cfg, &info, &out);
    if !vr.ok {
        cleanup(&out);
        // "No gain" and "below quality floor" both mean: keep the original, not an
        // error. The file may still be container-normalized (stream copy).
        if matches!(vr.reason, VerifyReason::NoGain | VerifyReason::QualityFloor) {
            if cfg.normalize_container && !is_target_container(src, cfg) {
                if let Some(r) = try_remux(
                    ff, cfg, manifest, &info, src, path_str, size, &cancelled, reporter,
                ) {
                    return r;
                }
            }
            let error = (vr.reason == VerifyReason::QualityFloor)
                .then(|| format!("quality floor: {}", vr.detail));
            set(
                manifest,
                path_str,
                Outcome::SkippedNoGain,
                StatusUpdate {
                    out_size: Some(vr.out_size),
                    error,
                    ..meta_upd(&info)
                },
            );
            return ProcessResult::new(path_str, Outcome::SkippedNoGain);
        }
        set(
            manifest,
            path_str,
            Outcome::Failed,
            StatusUpdate {
                error: Some(format!("verify {:?}: {}", vr.reason, vr.detail)),
                ..meta_upd(&info)
            },
        );
        return ProcessResult::new(path_str, Outcome::Failed)
            .with_message(format!("verify {:?}", vr.reason));
    }

    if let Err(e) = super::replace::replace_original(cfg, src, &out) {
        cleanup(&out);
        set(
            manifest,
            path_str,
            Outcome::Failed,
            StatusUpdate {
                error: Some(format!("replace: {e}")),
                ..meta_upd(&info)
            },
        );
        return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
    }

    let saved = size as i64 - vr.out_size as i64;
    set(
        manifest,
        path_str,
        Outcome::Done,
        StatusUpdate {
            out_size: Some(vr.out_size),
            saved_bytes: Some(saved),
            encode_ms: Some(encode_ms),
            ..meta_upd(&info)
        },
    );

    let mut result = ProcessResult::new(path_str, Outcome::Done);
    result.saved_bytes = saved;
    result.orig_size = Some(size);
    result.out_size = Some(vr.out_size);
    result
}

/// Register a discovered file's current size/mtime into the manifest (resume).
pub fn scan_into_manifest(manifest: &Manifest, cfg: &Config, path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = manifest.upsert_scanned(
            &path.to_string_lossy(),
            meta.len(),
            mtime_secs(&meta),
            cfg.force,
            cfg.retry_failed,
        );
    }
}

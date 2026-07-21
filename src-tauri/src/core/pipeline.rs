//! Per-file orchestration: probe → skip? → encode → verify → swap → record.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::abort::{AbortConfig, AbortJudge, AbortProjection};
use super::config::{Config, HealthGate, VerifyDepth};
use super::decode::decode_probe_progress;
use super::encode::{build_args_q, build_remux_args, run_encode, EncodeResult, ProgressSample};
use super::encoders::Encoder;
use super::estimate::{predict_skip, SkipKind};
use super::ffbin::FfBin;
use super::health::HealthState;
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

/// Last `n` chars of an ffmpeg stderr tail, newlines flattened — a compact excerpt
/// for a status message or fallback note.
fn tail_excerpt(tail: &str, n: usize) -> String {
    let flat = tail.replace('\n', " ");
    let flat = flat.trim();
    let skip = flat.chars().count().saturating_sub(n);
    flat.chars()
        .skip(skip)
        .collect::<String>()
        .trim()
        .to_string()
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
    let out_ext = cfg.container.ext().to_string();
    set(
        manifest,
        path_str,
        Outcome::Normalized,
        StatusUpdate {
            out_size: Some(out_size),
            saved_bytes: Some(saved),
            out_ext: Some(out_ext.clone()),
            ..meta_of(info)
        },
    );
    // The original was replaced, so any prior health scan of it is now stale —
    // drop it from the Library (a rescan will pick up the new file).
    let _ = manifest.clear_health(path_str);
    let mut r = ProcessResult::new(path_str, Outcome::Normalized);
    r.saved_bytes = saved;
    r.orig_size = Some(size);
    r.out_size = Some(out_size);
    r.out_ext = Some(out_ext);
    Some(r)
}

/// Resolve the CRF-like quality for one file. Preset mode returns the run's fixed
/// value. VMAF mode returns a cached per-title CRF when the file is unchanged,
/// else runs the sample-encode search, caches the result, and reports it. A
/// failed/cancelled search falls back to the preset quality — never a hard error.
///
/// Returns the CRF plus an optional note to attach to the file if it later
/// succeeds (e.g. the VMAF target was unreachable and the preset was used).
#[allow(clippy::too_many_arguments)]
fn resolve_quality(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    manifest: &Manifest,
    info: &MediaInfo,
    size: u64,
    temp_dir: &Path,
    path_str: &str,
    cancel: &(dyn Fn() -> bool + Sync),
    reporter: &dyn Reporter,
) -> (i32, Option<String>) {
    let Some(target) = cfg.vmaf_target else {
        return (cfg.resolved_quality(), None);
    };
    let mtime = std::fs::metadata(Path::new(path_str))
        .map(|m| mtime_secs(&m))
        .unwrap_or(0.0);

    if let Some(crf) = manifest.cached_vmaf_crf(path_str, size, mtime, target) {
        reporter.on_quality_resolved(path_str, target, crf, None);
        return (crf, None);
    }
    let on_search = |frac: f64| reporter.on_search_progress(path_str, frac);
    match super::vmaf::resolve_crf(ff, cfg, encoder, info, target, temp_dir, cancel, &on_search) {
        // Target reached: use and cache the searched CRF.
        Some(r) if r.met_target => {
            let _ = manifest.set_vmaf_crf(path_str, r.crf, target);
            reporter.on_quality_resolved(path_str, target, r.crf, Some(r.vmaf));
            (r.crf, None)
        }
        // Target unreachable even at best quality — the searched CRF would be
        // near-lossless (often bigger than the source, then early-aborted for no
        // gain). Use the preset quality instead so the file still shrinks sensibly.
        Some(r) => {
            let preset = cfg.resolved_quality();
            let _ = manifest.set_vmaf_crf(path_str, preset, target);
            reporter.on_quality_resolved(path_str, target, preset, Some(r.vmaf));
            let note = format!(
                "VMAF {target:.0} unreachable for this source (best ~{:.1} at CRF {}); \
                 encoded at the preset quality (CRF {preset}) instead of bloating the file.",
                r.vmaf, r.crf
            );
            (preset, Some(note))
        }
        None => (cfg.resolved_quality(), None),
    }
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
            // Pre-encode health gate: when on, an unreadable source is a deliberate
            // skip (recorded `unreadable`, flagged in the Library), not an encode
            // failure. Gate `Off` keeps the legacy plain-failure behavior exactly.
            if cfg.health_gate != HealthGate::Off {
                let _ = manifest.record_health(
                    path_str,
                    HealthState::Unreadable.as_str(),
                    Some("ffprobe could not read this file"),
                    None,
                    None,
                );
                set(
                    manifest,
                    path_str,
                    Outcome::SkippedUnhealthy,
                    StatusUpdate {
                        error: Some(format!("unreadable: {e}")),
                        ..Default::default()
                    },
                );
                return ProcessResult::new(path_str, Outcome::SkippedUnhealthy)
                    .with_message(format!("skipped — unreadable: {e}"));
            }
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

    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Open the file's Live card first, so the pre-encode work (the Deep health
    // gate below, then any VMAF search) is visible on it rather than looking like
    // a stall. This is why the gate runs *after* on_file_start, not before it.
    reporter.on_file_start(path_str, &name, info.duration, size);

    // Deep health gate: decode-probe the *source* before spending an encode, so
    // silent corruption (truncation/garble a structural probe can't see) is caught
    // and skipped/flagged, never encoded. It reports smooth progress on the now-open
    // Live card ("Checking health…") and is cancellable. Reuses the shared detector,
    // so a gated run can never disagree with a standalone scan. (Unreachable in
    // dry-run — that returns above, before the card opens.)
    if cfg.health_gate == HealthGate::Deep {
        reporter.on_health_progress(path_str, 0.0);
        let on_health = |f: f64| reporter.on_health_progress(path_str, f);
        let (ok, detail) = decode_probe_progress(
            &ff.ffmpeg,
            src,
            VerifyDepth::Fast,
            info.duration,
            &cancelled,
            &on_health,
        );
        if cancelled() {
            reporter.on_file_end(path_str);
            return ProcessResult::new(path_str, Outcome::Cancelled);
        }
        if !ok {
            reporter.on_file_end(path_str);
            let _ = manifest.record_health(
                path_str,
                HealthState::Corrupt.as_str(),
                Some("decode errors — likely truncated or corrupted"),
                None,
                None,
            );
            set(
                manifest,
                path_str,
                Outcome::SkippedUnhealthy,
                StatusUpdate {
                    error: Some(format!("corrupt source: {detail}")),
                    ..meta_upd(&info)
                },
            );
            return ProcessResult::new(path_str, Outcome::SkippedUnhealthy)
                .with_message(format!("skipped — corrupt source: {detail}"));
        }
    }

    // Resolve the target quality for this file. Preset mode returns the fixed
    // value instantly; VMAF mode searches (or reuses a cached CRF) for the
    // smallest file that still hits the perceptual target, then reports the choice.
    let (quality, quality_note) = resolve_quality(
        ff, cfg, encoder, manifest, &info, size, &temp_dir, path_str, &cancelled, reporter,
    );
    // A VMAF search can be aborted mid-flight; if so, don't start the real encode.
    if cancelled() {
        reporter.on_file_end(path_str);
        return ProcessResult::new(path_str, Outcome::Cancelled);
    }
    // Run one encode with a given config + encoder — its own fresh early-abort
    // judge and progress sink, so it can be retried cleanly for the fallback tiers.
    let attempt = |c: &Config, enc: &Encoder| -> EncodeResult {
        let args = build_args_q(c, &info, enc, &ff.caps, &out, quality);
        let judge = Mutex::new(AbortJudge::new(AbortConfig::from(c, info.duration, size)));
        let abort_pred =
            |sec: f64, out_bytes: Option<u64>| judge.lock().unwrap().observe(sec, out_bytes);
        let mut on_progress = |sample: ProgressSample| reporter.on_file_progress(path_str, sample);
        run_encode(
            &ff.ffmpeg,
            &args,
            &cancelled,
            &mut on_progress,
            Some(&abort_pred),
        )
    };
    // A real encode failure (non-zero, and not a cancel or an intentional early
    // abort). An `aborted` result is "no gain", not a failure — never retried.
    let failed = |e: &EncodeResult| e.returncode != Some(0) && !e.cancelled && !e.aborted;

    let encode_start = std::time::Instant::now();
    let mut enc = attempt(cfg, encoder);
    // Capture *why* the preferred (tier-1) pipeline failed before any retry — so if
    // a fallback then succeeds, the reason isn't lost (the file won't fail).
    let preferred_err = failed(&enc).then(|| tail_excerpt(&enc.stderr_tail, 300));
    let mut fallback_stage: Option<String> = None;

    // Tier 2 — a hardware encode that fails with no output on an edge-case source
    // (e.g. some VR geometries the GPU decode/scale path rejects) almost always
    // failed in the *CUDA decode/scale*, not the encoder itself. Keep the fast
    // hardware encoder but drop to software decode + software scale, which is the
    // compatible path. This is what stops GPU encodes from falling all the way to
    // slow software for what is really a decode/scale quirk.
    if failed(&enc) && encoder.family.is_hardware() && cfg.hardware_decode {
        cleanup(&out);
        tracing::warn!(
            encoder = %encoder.name,
            "GPU-resident encode failed; retrying with software decode + hardware encode"
        );
        let sw_decode = Config {
            hardware_decode: false,
            ..cfg.clone()
        };
        enc = attempt(&sw_decode, encoder);
        fallback_stage = Some(format!(
            "software decode + {} (hardware) encode",
            encoder.name
        ));
    }
    // Tier 3 — the hardware encoder itself can't handle this source; last resort is
    // the software encoder (which also takes the all-software decode/scale path).
    if failed(&enc) && encoder.family.is_hardware() {
        if let Some(sw) = super::encoders::software_encoder(cfg.codec) {
            if sw.name != encoder.name {
                cleanup(&out);
                tracing::warn!(
                    encoder = %encoder.name,
                    fallback = %sw.name,
                    "hardware encode still failing; retrying with software encoder"
                );
                enc = attempt(cfg, &sw);
                fallback_stage = Some(format!("software encoder {}", sw.name));
            }
        }
    }
    // The note attached to the file if it ultimately succeeds. Combines a VMAF
    // "target unreachable → used preset" note with any encode-pipeline fallback.
    let encode_note = match (&fallback_stage, &preferred_err) {
        (Some(stage), Some(err)) => Some(format!(
            "Fell back to {stage}. Preferred (GPU) pipeline failed — {err}"
        )),
        (Some(stage), None) => Some(format!("Fell back to {stage}.")),
        _ => None,
    };
    let fallback_note = match (quality_note, encode_note) {
        (Some(q), Some(e)) => Some(format!("{q} {e}")),
        (q, e) => q.or(e),
    };
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
        // Log the full ffmpeg tail so the actual encoder error (e.g. the specific
        // NVENC reason) is recoverable — the manifest only keeps a short excerpt.
        tracing::warn!(path = %path_str, rc, "encode failed after fallbacks:\n{}", enc.stderr_tail);
        set(
            manifest,
            path_str,
            Outcome::Failed,
            StatusUpdate {
                error: Some(format!(
                    "ffmpeg rc={rc}: {}",
                    tail_excerpt(&enc.stderr_tail, 400)
                )),
                ..meta_upd(&info)
            },
        );
        return ProcessResult::new(path_str, Outcome::Failed).with_message(format!(
            "ffmpeg rc={rc}: {}",
            tail_excerpt(&enc.stderr_tail, 160)
        ));
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

    let final_path = match super::replace::replace_original(cfg, src, &out) {
        Ok(p) => p,
        Err(e) => {
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
    };
    // The output's container extension — the source may have had a different one
    // (e.g. .mp4 → .mkv), so the UI resolves the current on-disk file from this.
    let out_ext = final_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string());

    let saved = size as i64 - vr.out_size as i64;
    set(
        manifest,
        path_str,
        Outcome::Done,
        StatusUpdate {
            out_size: Some(vr.out_size),
            saved_bytes: Some(saved),
            encode_ms: Some(encode_ms),
            fallback: fallback_note,
            out_ext: out_ext.clone(),
            ..meta_upd(&info)
        },
    );
    // The output already passed verify_output's decode, so the final file *is*
    // freshly-verified healthy. With the gate on, record that — a run populates the
    // Library health view for free (the encoded output verified healthy at both
    // ends). With the gate Off, keep the legacy behavior: just drop the now-stale
    // scan of the replaced original. (codec/height were already set on the Done
    // status above, so the health record only carries the verdict.)
    if cfg.health_gate == HealthGate::Off {
        let _ = manifest.clear_health(path_str);
    } else {
        let _ = manifest.record_health(path_str, HealthState::Healthy.as_str(), None, None, None);
    }

    let mut result = ProcessResult::new(path_str, Outcome::Done);
    result.saved_bytes = saved;
    result.orig_size = Some(size);
    result.out_size = Some(vr.out_size);
    result.out_ext = out_ext;
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

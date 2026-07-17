//! Per-file orchestration: probe → skip? → encode → verify → swap → record.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::config::Config;
use super::encode::{build_args, run_encode, AbortProjection};
use super::encoders::Encoder;
use super::ffbin::FfBin;
use super::manifest::{mtime_secs, Manifest, StatusUpdate};
use super::paths::temp_dir_for;
use super::probe::{probe, MediaInfo};
use super::report::{Outcome, ProcessResult, Reporter};
use super::util::human_bytes;
use super::verify::{verify_output, VerifyReason};

/// Skip files already in the target codec at/under the height cap. Avoids
/// pointless re-encodes and prevents reprocessing our own output.
pub fn is_already_efficient(cfg: &Config, info: &MediaInfo) -> bool {
    let codec_ok = info
        .codec
        .as_deref()
        .map(|c| cfg.codec.probe_names().contains(&c))
        .unwrap_or(false);
    codec_ok && info.height.map(|h| h <= cfg.max_height).unwrap_or(false)
}

/// Predict, before encoding, whether the re-encode is worth it. Downscaled files
/// always are (big savings), so only same-resolution re-encodes are candidates.
/// A source already at low bits-per-pixel won't shrink meaningfully.
pub fn predicted_marginal(cfg: &Config, info: &MediaInfo) -> bool {
    match info.height {
        Some(h) if h > cfg.max_height => return false, // will downscale → worth it
        None => return false,
        _ => {}
    }
    match info.bits_per_pixel() {
        Some(bpp) => bpp < cfg.marginal_bpp,
        None => false, // can't predict → don't skip
    }
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

/// Process one file end to end. Returns the run-local [`ProcessResult`]; the
/// durable status is written to the manifest along the way.
pub fn process_file(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    manifest: &Manifest,
    path_str: &str,
    cancel: &AtomicBool,
    reporter: &dyn Reporter,
) -> ProcessResult {
    let src = Path::new(path_str);

    if cancel.load(Ordering::Relaxed) {
        return ProcessResult::new(path_str, Outcome::Cancelled);
    }

    let meta = match std::fs::metadata(src) {
        Ok(m) => m,
        Err(e) => {
            set(manifest, path_str, Outcome::Failed, StatusUpdate {
                error: Some(format!("stat failed: {e}")),
                ..Default::default()
            });
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };
    let size = meta.len();

    if size == 0 {
        set(manifest, path_str, Outcome::Failed, StatusUpdate {
            error: Some("empty file".into()),
            ..Default::default()
        });
        return ProcessResult::new(path_str, Outcome::Failed).with_message("empty file");
    }

    let info = match probe(&ff.ffprobe, src, Duration::from_secs(120)) {
        Ok(i) => i,
        Err(e) => {
            set(manifest, path_str, Outcome::Failed, StatusUpdate {
                error: Some(format!("probe: {e}")),
                ..Default::default()
            });
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };

    let meta_upd = |o: &MediaInfo| StatusUpdate {
        src_codec: o.codec.clone(),
        height: o.height,
        ..Default::default()
    };

    if !cfg.force && is_already_efficient(cfg, &info) {
        set(manifest, path_str, Outcome::SkippedEfficient, meta_upd(&info));
        return ProcessResult::new(path_str, Outcome::SkippedEfficient);
    }

    if !cfg.force && cfg.skip_marginal && predicted_marginal(cfg, &info) {
        set(manifest, path_str, Outcome::SkippedMarginal, meta_upd(&info));
        return ProcessResult::new(path_str, Outcome::SkippedMarginal);
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
            set(manifest, path_str, Outcome::Failed, StatusUpdate {
                error: Some(format!("temp dir: {e}")),
                ..meta_upd(&info)
            });
            return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
        }
    };

    // NB: we deliberately do not pre-check free space. If the drive fills, the
    // encode simply fails and the original is left untouched (retried next run) —
    // the size gate and verify keep every outcome safe either way.
    let out = temp_dir.join(format!("sqz_{}.mkv", uuid::Uuid::new_v4().simple()));
    let args = build_args(cfg, &info, encoder, &out);

    // Early-abort predicate: project final size from bytes written past the
    // check point; abort if it clearly won't beat the size gate.
    let duration = info.duration.unwrap_or(0.0);
    let threshold = size as f64 * (1.0 - cfg.min_savings);
    let abort_pred = move |sec: f64, out_bytes: Option<u64>| -> Option<AbortProjection> {
        if !cfg.early_abort || duration <= 0.0 {
            return None;
        }
        let out_bytes = out_bytes?;
        if out_bytes == 0 {
            return None;
        }
        let frac = sec / duration;
        if frac < cfg.abort_check_at {
            return None;
        }
        let projected = out_bytes as f64 / frac;
        (projected > threshold).then_some(AbortProjection { frac, projected })
    };

    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    reporter.on_file_start(path_str, &name, info.duration, size);

    let mut on_progress = |sec: f64, out_bytes: Option<u64>| {
        reporter.on_file_progress(path_str, sec, out_bytes);
    };
    let enc = run_encode(&ff.ffmpeg, &args, cancel, &mut on_progress, Some(&abort_pred));
    reporter.on_file_end(path_str);

    if enc.cancelled {
        cleanup(&out);
        return ProcessResult::new(path_str, Outcome::Cancelled);
    }

    if enc.aborted {
        cleanup(&out);
        let proj = enc.abort_projection.unwrap_or(AbortProjection { frac: 0.0, projected: 0.0 });
        let msg = format!(
            "aborted at {:.0}% — projected {} vs {} original",
            proj.frac * 100.0,
            human_bytes(proj.projected),
            human_bytes(size as f64)
        );
        set(manifest, path_str, Outcome::SkippedNoGain, StatusUpdate {
            error: Some(msg.clone()),
            ..meta_upd(&info)
        });
        return ProcessResult::new(path_str, Outcome::SkippedNoGain).with_message(msg);
    }

    if enc.returncode != Some(0) {
        cleanup(&out);
        let rc = enc.returncode.unwrap_or(-1);
        let tail: String = enc.stderr_tail.replace('\n', " ");
        let tail_trim: String = tail.chars().rev().take(400).collect::<String>().chars().rev().collect();
        set(manifest, path_str, Outcome::Failed, StatusUpdate {
            error: Some(format!("ffmpeg rc={rc}: {tail_trim}")),
            ..meta_upd(&info)
        });
        let short: String = tail.chars().rev().take(160).collect::<String>().chars().rev().collect();
        return ProcessResult::new(path_str, Outcome::Failed)
            .with_message(format!("ffmpeg rc={rc}: {short}"));
    }

    let vr = verify_output(&ff.ffmpeg, &ff.ffprobe, cfg, &info, &out);
    if !vr.ok {
        cleanup(&out);
        if vr.reason == VerifyReason::NoGain {
            set(manifest, path_str, Outcome::SkippedNoGain, StatusUpdate {
                out_size: Some(vr.out_size),
                ..meta_upd(&info)
            });
            return ProcessResult::new(path_str, Outcome::SkippedNoGain);
        }
        set(manifest, path_str, Outcome::Failed, StatusUpdate {
            error: Some(format!("verify {:?}: {}", vr.reason, vr.detail)),
            ..meta_upd(&info)
        });
        return ProcessResult::new(path_str, Outcome::Failed)
            .with_message(format!("verify {:?}", vr.reason));
    }

    if let Err(e) = super::replace::replace_original(cfg, src, &out) {
        cleanup(&out);
        set(manifest, path_str, Outcome::Failed, StatusUpdate {
            error: Some(format!("replace: {e}")),
            ..meta_upd(&info)
        });
        return ProcessResult::new(path_str, Outcome::Failed).with_message(e.to_string());
    }

    let saved = size as i64 - vr.out_size as i64;
    set(manifest, path_str, Outcome::Done, StatusUpdate {
        out_size: Some(vr.out_size),
        saved_bytes: Some(saved),
        ..meta_upd(&info)
    });

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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::Codec;
    use super::super::probe::MediaInfo;
    use std::path::PathBuf;

    fn info(codec: &str, height: Option<u32>, bpp_bitrate: Option<u64>) -> MediaInfo {
        MediaInfo {
            path: PathBuf::from("x.mkv"),
            codec: Some(codec.into()),
            width: Some(1920),
            height,
            pix_fmt: Some("yuv420p".into()),
            duration: Some(60.0),
            video_bitrate: bpp_bitrate,
            fps: Some(30.0),
            size: Some(50_000_000),
            sub_codecs: vec![],
        }
    }

    #[test]
    fn already_efficient_matches_target_codec_under_cap() {
        let cfg = Config { codec: Codec::Av1, max_height: 1080, ..Config::default() };
        assert!(is_already_efficient(&cfg, &info("av1", Some(1080), None)));
        assert!(!is_already_efficient(&cfg, &info("av1", Some(2160), None))); // too tall
        assert!(!is_already_efficient(&cfg, &info("h264", Some(720), None))); // wrong codec
    }

    #[test]
    fn downscale_targets_are_never_marginal() {
        let cfg = Config { skip_marginal: true, max_height: 1080, ..Config::default() };
        // 4K source → will downscale → always worth it regardless of bpp.
        assert!(!predicted_marginal(&cfg, &info("h264", Some(2160), Some(1_000_000))));
    }

    #[test]
    fn low_bpp_same_res_is_marginal() {
        let cfg = Config { skip_marginal: true, marginal_bpp: 0.05, ..Config::default() };
        // Very low bitrate → low bpp → marginal.
        assert!(predicted_marginal(&cfg, &info("h264", Some(1080), Some(50_000))));
        // High bitrate → high bpp → worth encoding.
        assert!(!predicted_marginal(&cfg, &info("h264", Some(1080), Some(20_000_000))));
    }
}

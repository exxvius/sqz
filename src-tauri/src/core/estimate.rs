//! Reclaimable-space projection: turn the manifest's realized-savings history
//! into a before-the-run estimate of how much a run will actually reclaim.
//!
//! Two tiers. Tier 1 is instant — it multiplies the candidate bytes by the
//! global historical savings ratio (or a conservative static prior when there's
//! no history yet). Tier 2 is probe-refined — it buckets each file by
//! `(codec, resolution)`, blends the per-bucket ratio toward the global one via
//! shrinkage, and excludes the files the run would skip.
//!
//! Everything here is pure and unit-testable. The skip predicates live here too
//! and are the *same* ones the pipeline runs, so a projection can never disagree
//! with what a real run does.

use std::collections::HashMap;

use serde::Serialize;

use super::config::{BitDepth, Codec, Config};
use super::manifest::BucketAggRow;
use super::probe::MediaInfo;

/// Shrinkage constant: a bucket needs roughly this many samples before its own
/// ratio outweighs the global prior. Small buckets lean on the global number
/// instead of a noisy local one.
const SHRINKAGE_K: f64 = 20.0;

// ---- Skip predicates (shared with the pipeline) ----------------------------

/// Skip files already in the target codec at/under the height cap *and* already
/// at the requested bit depth. Avoids pointless re-encodes and prevents
/// reprocessing our own output — but a file at the wrong bit depth (e.g. an 8-bit
/// source when 10-bit is requested) is still a worthwhile re-encode.
pub fn is_already_efficient(cfg: &Config, info: &MediaInfo) -> bool {
    let codec_ok = info
        .codec
        .as_deref()
        .map(|c| cfg.codec.probe_names().contains(&c))
        .unwrap_or(false);
    let height_ok = info.height.map(|h| h <= cfg.max_height).unwrap_or(false);
    let depth_ok = match cfg.bit_depth {
        BitDepth::Source => true,
        BitDepth::Eight => info.bit_depth() == 8,
        BitDepth::Ten => info.bit_depth() == 10,
    };
    codec_ok && height_ok && depth_ok
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

/// Why the run would skip a file without re-encoding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    DolbyVision,
    AlreadyEfficient,
    Marginal,
}

/// Predict whether `process_file` will skip this file (no re-encode). Mirrors the
/// exact order and conditions of the skip checks in `pipeline::process_file`, so
/// the projection stays honest. `force` short-circuits every skip (a forced run
/// re-encodes everything).
pub fn predict_skip(cfg: &Config, info: &MediaInfo, force: bool) -> Option<SkipKind> {
    if force {
        return None;
    }
    if cfg.skip_dolby_vision && info.dolby_vision {
        return Some(SkipKind::DolbyVision);
    }
    if is_already_efficient(cfg, info) {
        return Some(SkipKind::AlreadyEfficient);
    }
    if cfg.skip_marginal && predicted_marginal(cfg, info) {
        return Some(SkipKind::Marginal);
    }
    None
}

// ---- Projection types -------------------------------------------------------

/// A `(codec, resolution)` slice of the projection.
#[derive(Debug, Clone, Serialize)]
pub struct ReclaimBucket {
    pub src_codec: String,
    pub height_band: String,
    /// Files in this bucket that will actually be re-encoded (skipped excluded).
    pub files: u32,
    /// On-disk bytes of those non-skipped files.
    pub candidate_bytes: u64,
    pub est_reclaimable_bytes: u64,
    pub est_skipped_files: u32,
    /// History rows backing this bucket's ratio.
    pub sample_size: u32,
    /// 0..1 from `sample_size` (how much the local ratio is trusted).
    pub confidence: f32,
}

/// The whole projection for a candidate input set.
#[derive(Debug, Clone, Serialize)]
pub struct ReclaimProjection {
    /// 1 = instant estimate, 2 = probe-refined.
    pub tier: u8,
    /// All discovered candidate files (including ones that will be skipped).
    pub candidate_files: u32,
    /// On-disk bytes of all discovered candidates.
    pub candidate_bytes: u64,
    pub est_reclaimable_bytes: u64,
    pub est_skipped_files: u32,
    pub buckets: Vec<ReclaimBucket>,
    /// Total `done` history rows the global ratio is drawn from.
    pub based_on_history_rows: u32,
    /// Coarse aggregate confidence: "low" | "fair" | "good".
    pub confidence: String,
    /// True when there's no history and the estimate leans on a static prior.
    pub cold_start: bool,
}

/// A probed candidate reduced to what the projection needs. Keeping it flat makes
/// [`tier2`] trivially unit-testable without constructing a full [`MediaInfo`].
#[derive(Debug, Clone)]
pub struct ProbedFile {
    /// ffprobe codec name, or "unknown" when it couldn't be determined.
    pub src_codec: String,
    /// Height band label, or "unknown".
    pub height_band: String,
    /// Authoritative on-disk size.
    pub bytes: u64,
    /// Whether the run would skip this file (from [`predict_skip`]).
    pub skip: bool,
}

/// Band a pixel height into a coarse resolution class.
pub fn height_band(h: u32) -> &'static str {
    match h {
        0..=720 => "≤720p",
        721..=1080 => "1080p",
        1081..=1440 => "1440p",
        1441..=2160 => "2160p",
        _ => ">2160p",
    }
}

/// Conservative fraction reclaimed, per target codec, when there's no history.
fn static_prior(codec: Codec) -> f64 {
    match codec {
        Codec::Av1 => 0.55,
        Codec::Hevc => 0.45,
        Codec::H264 => 0.30,
    }
}

/// Map a 0..1 confidence to a coarse band. A cold start is always "low".
fn confidence_band(conf: f64, cold_start: bool) -> &'static str {
    if cold_start {
        return "low";
    }
    if conf < 0.34 {
        "low"
    } else if conf < 0.67 {
        "fair"
    } else {
        "good"
    }
}

/// Tier 1 — instant estimate from the global historical ratio (or a static
/// prior). No buckets, no skip detection: that needs a probe.
pub fn tier1(
    candidate_files: u32,
    candidate_bytes: u64,
    global: Option<(f64, u32)>,
    codec: Codec,
) -> ReclaimProjection {
    let (ratio, n, cold) = match global {
        Some((r, n)) => (r, n, false),
        None => (static_prior(codec), 0, true),
    };
    let est = (candidate_bytes as f64 * ratio) as u64;
    let conf = (n as f64) / (n as f64 + SHRINKAGE_K);
    ReclaimProjection {
        tier: 1,
        candidate_files,
        candidate_bytes,
        est_reclaimable_bytes: est,
        est_skipped_files: 0,
        buckets: Vec::new(),
        based_on_history_rows: n,
        confidence: confidence_band(conf, cold).into(),
        cold_start: cold,
    }
}

/// Collapse raw per-`(codec, height)` history aggregates into per-band ratios.
/// `raw` rows are `(codec, height, saved_sum, size_sum, n)`.
pub fn aggregate_bucket_ratios(raw: &[BucketAggRow]) -> HashMap<(String, String), (f64, u32)> {
    let mut acc: HashMap<(String, String), (i64, i64, u32)> = HashMap::new();
    for (codec, height, saved, size, n) in raw {
        let band = height_band(u32::try_from(*height).unwrap_or(0)).to_string();
        let e = acc.entry((codec.clone(), band)).or_insert((0, 0, 0));
        e.0 += saved;
        e.1 += size;
        e.2 += n;
    }
    acc.into_iter()
        .filter_map(|((c, b), (saved, size, n))| {
            if size <= 0 {
                return None;
            }
            let ratio = (saved as f64 / size as f64).clamp(0.0, 1.0);
            Some(((c, b), (ratio, n)))
        })
        .collect()
}

/// Tier 2 — probe-refined estimate. `files` covers *every* discovered candidate
/// (unprobable ones arrive as "unknown"/"unknown", `skip = false`).
pub fn tier2(
    files: &[ProbedFile],
    global: Option<(f64, u32)>,
    bucket_ratios: &HashMap<(String, String), (f64, u32)>,
    codec: Codec,
) -> ReclaimProjection {
    let cold = global.is_none();
    let global_ratio = global
        .map(|(r, _)| r)
        .unwrap_or_else(|| static_prior(codec));
    let global_n = global.map(|(_, n)| n).unwrap_or(0);

    #[derive(Default)]
    struct Agg {
        files: u32,
        bytes: u64,
        skipped: u32,
    }
    let mut aggs: HashMap<(String, String), Agg> = HashMap::new();

    let candidate_files = files.len() as u32;
    let candidate_bytes: u64 = files.iter().map(|f| f.bytes).sum();
    let mut est_skipped_files = 0u32;

    for f in files {
        let a = aggs
            .entry((f.src_codec.clone(), f.height_band.clone()))
            .or_default();
        if f.skip {
            a.skipped += 1;
            est_skipped_files += 1;
        } else {
            a.files += 1;
            a.bytes += f.bytes;
        }
    }

    let mut buckets = Vec::with_capacity(aggs.len());
    let mut est_total = 0u64;
    let mut weighted_conf_num = 0f64; // Σ(bytes · confidence)
    for ((codec_s, band), a) in aggs {
        let (bucket_ratio, n) = bucket_ratios
            .get(&(codec_s.clone(), band.clone()))
            .copied()
            .unwrap_or((global_ratio, 0));
        let w = (n as f64) / (n as f64 + SHRINKAGE_K);
        let blended = w * bucket_ratio + (1.0 - w) * global_ratio;
        let est = (a.bytes as f64 * blended) as u64;
        est_total += est;
        weighted_conf_num += a.bytes as f64 * w;
        buckets.push(ReclaimBucket {
            src_codec: codec_s,
            height_band: band,
            files: a.files,
            candidate_bytes: a.bytes,
            est_reclaimable_bytes: est,
            est_skipped_files: a.skipped,
            sample_size: n,
            confidence: w as f32,
        });
    }
    // Most-reclaimable first, with a stable tiebreak so the table order is fixed.
    buckets.sort_by(|a, b| {
        b.est_reclaimable_bytes
            .cmp(&a.est_reclaimable_bytes)
            .then_with(|| a.src_codec.cmp(&b.src_codec))
            .then_with(|| a.height_band.cmp(&b.height_band))
    });

    let encodable_bytes: u64 = buckets.iter().map(|b| b.candidate_bytes).sum();
    let agg_conf = if encodable_bytes > 0 {
        weighted_conf_num / encodable_bytes as f64
    } else {
        0.0
    };

    ReclaimProjection {
        tier: 2,
        candidate_files,
        candidate_bytes,
        est_reclaimable_bytes: est_total,
        est_skipped_files,
        buckets,
        based_on_history_rows: global_n,
        confidence: confidence_band(agg_conf, cold).into(),
        cold_start: cold,
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::Codec;
    use super::*;
    use std::path::PathBuf;

    fn info(codec: &str, height: Option<u32>, bitrate: Option<u64>) -> MediaInfo {
        MediaInfo {
            path: PathBuf::from("x.mkv"),
            codec: Some(codec.into()),
            width: Some(1920),
            height,
            pix_fmt: Some("yuv420p".into()),
            duration: Some(60.0),
            video_bitrate: bitrate,
            fps: Some(30.0),
            size: Some(50_000_000),
            sub_codecs: vec![],
            color_primaries: None,
            color_transfer: None,
            color_space: None,
            color_range: None,
            dolby_vision: false,
        }
    }

    #[test]
    fn already_efficient_matches_target_codec_under_cap() {
        let cfg = Config {
            codec: Codec::Av1,
            max_height: 1080,
            ..Config::default()
        };
        assert!(is_already_efficient(&cfg, &info("av1", Some(1080), None)));
        assert!(!is_already_efficient(&cfg, &info("av1", Some(2160), None))); // too tall
        assert!(!is_already_efficient(&cfg, &info("h264", Some(720), None))); // wrong codec
    }

    #[test]
    fn wrong_bit_depth_is_not_already_efficient() {
        // Target AV1 1080p, but now demanding 10-bit output.
        let cfg = Config {
            codec: Codec::Av1,
            max_height: 1080,
            bit_depth: BitDepth::Ten,
            ..Config::default()
        };
        // An 8-bit AV1 1080p file is NOT efficient — it should re-encode to 10-bit.
        let eight_bit = info("av1", Some(1080), None); // helper builds yuv420p (8-bit)
        assert!(!is_already_efficient(&cfg, &eight_bit));
        // A 10-bit AV1 1080p file already satisfies the target → efficient.
        let mut ten_bit = info("av1", Some(1080), None);
        ten_bit.pix_fmt = Some("yuv420p10le".into());
        assert!(is_already_efficient(&cfg, &ten_bit));
        // With Source depth, an 8-bit file stays efficient (depth isn't a driver).
        let src_cfg = Config {
            bit_depth: BitDepth::Source,
            ..cfg
        };
        assert!(is_already_efficient(&src_cfg, &eight_bit));
    }

    #[test]
    fn downscale_targets_are_never_marginal() {
        let cfg = Config {
            skip_marginal: true,
            max_height: 1080,
            ..Config::default()
        };
        assert!(!predicted_marginal(
            &cfg,
            &info("h264", Some(2160), Some(1_000_000))
        ));
    }

    #[test]
    fn low_bpp_same_res_is_marginal() {
        let cfg = Config {
            skip_marginal: true,
            marginal_bpp: 0.05,
            ..Config::default()
        };
        assert!(predicted_marginal(
            &cfg,
            &info("h264", Some(1080), Some(50_000))
        ));
        assert!(!predicted_marginal(
            &cfg,
            &info("h264", Some(1080), Some(20_000_000))
        ));
    }

    #[test]
    fn predict_skip_mirrors_pipeline_order() {
        // Already-efficient AV1 under the cap is skipped as efficient.
        let cfg = Config {
            codec: Codec::Av1,
            max_height: 1080,
            ..Config::default()
        };
        assert_eq!(
            predict_skip(&cfg, &info("av1", Some(1080), None), false),
            Some(SkipKind::AlreadyEfficient)
        );
        // Dolby Vision wins even over already-efficient.
        let mut dv = info("av1", Some(1080), None);
        dv.dolby_vision = true;
        assert_eq!(predict_skip(&cfg, &dv, false), Some(SkipKind::DolbyVision));
        // Force short-circuits everything.
        assert_eq!(predict_skip(&cfg, &dv, true), None);
        // A fat h264 source is not skipped.
        assert_eq!(
            predict_skip(&cfg, &info("h264", Some(1080), Some(20_000_000)), false),
            None
        );
    }

    #[test]
    fn height_bands_cover_the_ranges() {
        assert_eq!(height_band(480), "≤720p");
        assert_eq!(height_band(720), "≤720p");
        assert_eq!(height_band(1080), "1080p");
        assert_eq!(height_band(1440), "1440p");
        assert_eq!(height_band(2160), "2160p");
        assert_eq!(height_band(4320), ">2160p");
    }

    #[test]
    fn tier1_uses_global_ratio_when_present() {
        let p = tier1(10, 1_000, Some((0.5, 40)), Codec::Av1);
        assert_eq!(p.tier, 1);
        assert_eq!(p.est_reclaimable_bytes, 500);
        assert!(!p.cold_start);
        assert_eq!(p.based_on_history_rows, 40);
    }

    #[test]
    fn tier1_falls_back_to_static_prior_cold() {
        let p = tier1(10, 1_000, None, Codec::Av1);
        assert!(p.cold_start);
        assert_eq!(p.confidence, "low");
        assert_eq!(p.est_reclaimable_bytes, 550); // 0.55 AV1 prior
    }

    #[test]
    fn tier2_excludes_skipped_bytes_and_buckets() {
        let files = vec![
            ProbedFile {
                src_codec: "h264".into(),
                height_band: "1080p".into(),
                bytes: 1_000,
                skip: false,
            },
            ProbedFile {
                src_codec: "av1".into(),
                height_band: "1080p".into(),
                bytes: 2_000,
                skip: true,
            },
        ];
        let mut ratios = HashMap::new();
        ratios.insert(("h264".to_string(), "1080p".to_string()), (0.5, 100));
        let p = tier2(&files, Some((0.4, 100)), &ratios, Codec::Av1);
        assert_eq!(p.candidate_files, 2);
        assert_eq!(p.candidate_bytes, 3_000);
        assert_eq!(p.est_skipped_files, 1);
        // Only the 1000-byte h264 file counts (the av1 one is skipped). Its bucket
        // ratio (0.5, n=100) blends toward the global (0.4): w = 100/120 ≈ 0.833,
        // so blended ≈ 0.483 → ~483 reclaimable.
        assert!((475..=490).contains(&p.est_reclaimable_bytes));
    }

    #[test]
    fn tier2_blends_small_buckets_toward_global() {
        // A bucket with no history leans entirely on the global ratio.
        let files = vec![ProbedFile {
            src_codec: "h264".into(),
            height_band: "1080p".into(),
            bytes: 1_000,
            skip: false,
        }];
        let p = tier2(&files, Some((0.3, 100)), &HashMap::new(), Codec::Av1);
        assert_eq!(p.est_reclaimable_bytes, 300);
        let bucket = &p.buckets[0];
        assert_eq!(bucket.sample_size, 0);
        assert_eq!(bucket.confidence, 0.0);
    }

    #[test]
    fn aggregate_bucket_ratios_bands_and_ratios() {
        let raw = vec![
            ("h264".to_string(), 1080, 500, 1000, 30),
            ("h264".to_string(), 1000, 500, 1000, 10), // same band (1080p)
        ];
        let out = aggregate_bucket_ratios(&raw);
        let (ratio, n) = out[&("h264".to_string(), "1080p".to_string())];
        assert_eq!(n, 40);
        assert!((ratio - 0.5).abs() < 1e-9);
    }
}

//! VMAF-targeted per-title CRF search (opt-in "target a perceptual quality,
//! not a fixed CRF").
//!
//! Splits into a **pure** binary search ([`search_crf`]) over a `measure(crf)`
//! seam, and the **impure** sampling + `libvmaf` measurement ([`resolve_crf`])
//! that drives it. Engine tests cover the pure parts (search + sample-window
//! selection); the FFmpeg-spawning measurement is validated on real files, per
//! the project's "engine tests never spawn FFmpeg" rule.
//!
//! Model: the search ranges over the same "lower = better" quality value
//! [`super::encode::encoder_rate_args`] already consumes, so every encoder family
//! is supported for free. The chosen CRF is cached in the manifest keyed on
//! `(path, size, mtime, target)`, so a re-run never re-searches an unchanged file.

use std::path::Path;
use std::process::Stdio;

use super::config::{Codec, Config};
use super::encode::{encoder_rate_args, needs_downscale, pix_fmt, software_scale_vf};
use super::encoders::Encoder;
use super::ffbin::FfBin;
use super::probe::MediaInfo;
use super::util::command_no_window;

/// Number of samples taken across a title, and each sample's length (seconds).
pub const VMAF_SAMPLES: usize = 3;
pub const VMAF_SAMPLE_SECS: f64 = 15.0;
/// Default target if VMAF mode is on but no value was supplied (the UI always
/// sends one; this is a floor for headless/config-only use).
pub const VMAF_DEFAULT_TARGET: f64 = 95.0;
/// Skip this fraction of the head and tail when choosing samples, so intros and
/// credits (atypically simple frames) don't skew the score.
const EDGE_SKIP_FRAC: f64 = 0.05;

/// Inclusive CRF-like search bounds (lower = better quality / bigger file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrfBounds {
    pub min: i32,
    pub max: i32,
}

/// Result of a per-title CRF search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmafResult {
    /// Chosen quality value (fed to `encoder_rate_args`).
    pub crf: i32,
    /// Measured VMAF at `crf` (mean across samples).
    pub vmaf: f64,
    /// Sample-encode rounds spent (telemetry / cost surfacing).
    pub probes: u32,
}

/// Per-codec CRF search bounds, centered near the codec's balanced base so the
/// search starts close to the plausible answer and converges in a few probes.
pub fn bounds_for(codec: Codec) -> CrfBounds {
    let base = codec.base_quality();
    CrfBounds {
        min: (base - 12).max(1),
        max: base + 15,
    }
}

/// Upper bound on the number of CRF probes a binary search over `bounds` can
/// spend: `floor(log2(width)) + 1`, plus one for the best-effort fallback probe.
/// Used to size the search progress bar so it never overshoots.
pub fn max_probes(bounds: CrfBounds) -> u32 {
    let width = (bounds.max - bounds.min + 1).max(1) as u32;
    (u32::BITS - width.leading_zeros()) + 1
}

/// Pick `samples` evenly-spread windows of `secs` seconds across `duration`,
/// skipping the head/tail edge. Pure and unit-tested.
///
/// Returns `(start, len)` pairs. A file shorter than one sample collapses to a
/// single whole-file window; every window is clamped inside `[0, duration]`.
pub fn sample_windows(duration: f64, samples: usize, secs: f64) -> Vec<(f64, f64)> {
    if duration <= 0.0 {
        return Vec::new();
    }
    if duration <= secs {
        return vec![(0.0, duration)];
    }
    let n = samples.max(1);
    let edge = duration * EDGE_SKIP_FRAC;
    let last_start = (duration - edge - secs).max(edge);
    if n == 1 {
        let start = ((edge + last_start) / 2.0).clamp(0.0, duration - secs);
        return vec![(start, secs)];
    }
    (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64; // 0.0 ..= 1.0
            let start = (edge + t * (last_start - edge)).clamp(0.0, duration - secs);
            (start, secs)
        })
        .collect()
}

/// Highest CRF (smallest file) whose measured VMAF is `>= target`.
///
/// Pure: `measure` is the only side-effecting seam. Binary-searches the integer
/// CRF range, memoizing probes. If even `bounds.min` (best quality) can't reach
/// `target`, returns that best-effort point with its score rather than failing,
/// so the caller still produces the highest-quality encode it can. A `None` from
/// `measure` (measurement failed) aborts the search and returns `None`, letting
/// the caller fall back to the preset quality.
pub fn search_crf(
    bounds: CrfBounds,
    target: f64,
    measure: &dyn Fn(i32) -> Option<f64>,
) -> Option<VmafResult> {
    let (mut lo, mut hi) = (bounds.min, bounds.max);
    let mut cache: Vec<(i32, f64)> = Vec::new();
    let mut probes = 0u32;
    let mut best: Option<(i32, f64)> = None;

    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let v = measure_cached(mid, measure, &mut cache, &mut probes)?;
        if v >= target {
            best = Some((mid, v));
            lo = mid + 1;
        } else {
            hi = mid - 1;
        }
    }

    let (crf, vmaf) = match best {
        Some(b) => b,
        None => {
            // Nothing met the target; encode at best quality (min CRF) anyway.
            let v = measure_cached(bounds.min, measure, &mut cache, &mut probes)?;
            (bounds.min, v)
        }
    };
    Some(VmafResult { crf, vmaf, probes })
}

fn measure_cached(
    crf: i32,
    measure: &dyn Fn(i32) -> Option<f64>,
    cache: &mut Vec<(i32, f64)>,
    probes: &mut u32,
) -> Option<f64> {
    if let Some(&(_, v)) = cache.iter().find(|(c, _)| *c == crf) {
        return Some(v);
    }
    let v = measure(crf)?;
    *probes += 1;
    cache.push((crf, v));
    Some(v)
}

/// Resolve the per-title CRF for `info` that hits `target` VMAF, via sample-encode
/// and `libvmaf` measurement. Returns `None` (caller falls back to preset quality)
/// on cancellation or any measurement failure — never a hard error.
///
/// `on_progress(done, total)` is called after every completed sample-encode so the
/// UI can advance a determinate bar during the search. `total` is a stable upper
/// bound (max probes × samples), so `done` never exceeds it and the bar only moves
/// forward.
#[allow(clippy::too_many_arguments)]
pub fn resolve_crf(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    info: &MediaInfo,
    target: f64,
    temp_dir: &Path,
    cancel: &(dyn Fn() -> bool + Sync),
    on_progress: &(dyn Fn(u32, u32) + Sync),
) -> Option<VmafResult> {
    let duration = info.duration.unwrap_or(0.0);
    let windows = sample_windows(duration, VMAF_SAMPLES, VMAF_SAMPLE_SECS);
    if windows.is_empty() {
        return None;
    }
    let bounds = bounds_for(cfg.codec);
    let total = max_probes(bounds) * windows.len() as u32;
    let done = std::cell::Cell::new(0u32);
    on_progress(0, total);

    let measure = |crf: i32| -> Option<f64> {
        if cancel() {
            return None;
        }
        let mut scores = Vec::with_capacity(windows.len());
        for (i, &(start, len)) in windows.iter().enumerate() {
            if cancel() {
                return None;
            }
            let dist = temp_dir.join(format!(
                "sqz_vmaf_{}_{}_{}.mkv",
                uuid::Uuid::new_v4().simple(),
                crf,
                i
            ));
            let encoded = encode_sample(ff, cfg, encoder, info, start, len, crf, &dist);
            let score = encoded
                .then(|| measure_sample(ff, cfg, info, start, len, &dist))
                .flatten();
            let _ = std::fs::remove_file(&dist);
            let s = score?;
            // Advance the search bar as each sample completes, clamped to `total`.
            done.set((done.get() + 1).min(total));
            on_progress(done.get(), total);
            scores.push(s);
        }
        (!scores.is_empty()).then(|| scores.iter().sum::<f64>() / scores.len() as f64)
    };

    search_crf(bounds, target, &measure)
}

/// Encode one sample window at `crf` into `out` (video only). Mirrors the real
/// encode's scaling, rate control, and pixel format so the measured quality
/// reflects what will ship. Returns whether a non-empty output was produced.
#[allow(clippy::too_many_arguments)]
fn encode_sample(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    info: &MediaInfo,
    start: f64,
    len: f64,
    crf: i32,
    out: &Path,
) -> bool {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-ss".into(),
        format!("{start}"),
        "-t".into(),
        format!("{len}"),
        "-i".into(),
        info.path.to_string_lossy().into_owned(),
        "-map".into(),
        "0:v:0".into(),
    ];
    if needs_downscale(cfg, info) {
        a.push("-vf".into());
        a.push(software_scale_vf(cfg));
    }
    a.push("-c:v".into());
    a.push(encoder.name.clone());
    a.extend(encoder_rate_args(cfg, encoder, crf));
    a.push("-pix_fmt".into());
    a.push(pix_fmt(cfg, info, encoder).into());
    a.push("-an".into());
    a.push(out.to_string_lossy().into_owned());

    let ok = command_no_window(&ff.ffmpeg)
        .args(&a)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false)
}

/// Measure VMAF of the distorted sample `dist` against the matching source
/// window. Follows the SSIM convention in `verify.rs`: distorted is input 0,
/// reference is input 1; the reference is scaled to the distorted geometry so the
/// score reflects the shipped resolution.
fn measure_sample(
    ff: &FfBin,
    cfg: &Config,
    info: &MediaInfo,
    start: f64,
    len: f64,
    dist: &Path,
) -> Option<f64> {
    let filter = if needs_downscale(cfg, info) {
        format!("[1:v]{}[ref];[0:v][ref]libvmaf", software_scale_vf(cfg))
    } else {
        "[0:v][1:v]libvmaf".to_string()
    };

    let out = command_no_window(&ff.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-v", "info", "-i"])
        .arg(dist)
        .args(["-ss", &format!("{start}"), "-t", &format!("{len}"), "-i"])
        .arg(&info.path)
        .args(["-filter_complex", &filter, "-an", "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .output()
        .ok()?;

    parse_vmaf_score(&String::from_utf8_lossy(&out.stderr))
}

/// Parse the `VMAF score: NN.NN` line libvmaf prints to stderr, locale-invariantly
/// (ffmpeg always uses '.'), taking the last occurrence.
fn parse_vmaf_score(stderr: &str) -> Option<f64> {
    let idx = stderr.rfind("VMAF score:")?;
    let rest = &stderr[idx + "VMAF score:".len()..];
    let num: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_highest_crf_meeting_target() {
        // Synthetic monotone curve: higher CRF → lower VMAF.
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(CrfBounds { min: 0, max: 63 }, 90.0, &measure).unwrap();
        // 100 - crf >= 90  ⇒  crf <= 10; highest is 10.
        assert_eq!(r.crf, 10);
        assert!((r.vmaf - 90.0).abs() < 1e-9);
    }

    #[test]
    fn search_clamps_to_min_when_target_unreachable() {
        // Even best quality in-range (crf=20 → vmaf 80) misses target 95.
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(CrfBounds { min: 20, max: 45 }, 95.0, &measure).unwrap();
        assert_eq!(r.crf, 20);
        assert!((r.vmaf - 80.0).abs() < 1e-9);
    }

    #[test]
    fn search_clamps_to_max_when_everything_passes() {
        // Every CRF in range clears a very low target → smallest file (max CRF).
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(CrfBounds { min: 20, max: 45 }, 10.0, &measure).unwrap();
        assert_eq!(r.crf, 45);
    }

    #[test]
    fn search_probe_count_is_logarithmic() {
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(CrfBounds { min: 20, max: 45 }, 72.0, &measure).unwrap();
        // A 26-wide range converges in ~log2(26) ≈ 5 probes, never linearly.
        assert!(r.probes <= 6, "probes={}", r.probes);
        assert_eq!(r.crf, 28); // 100 - 28 = 72
    }

    #[test]
    fn search_propagates_measurement_failure() {
        let measure = |_: i32| None;
        assert!(search_crf(CrfBounds { min: 20, max: 45 }, 90.0, &measure).is_none());
    }

    #[test]
    fn sample_windows_spread_across_the_middle() {
        let w = sample_windows(600.0, 3, 15.0);
        assert_eq!(w.len(), 3);
        // Edge-skipped: first start is after the 5% head, last ends before the tail.
        assert!(w[0].0 >= 30.0 - 1e-9);
        assert!(w[2].0 + w[2].1 <= 570.0 + 1e-9);
        // Strictly increasing starts, each a full-length window.
        assert!(w[0].0 < w[1].0 && w[1].0 < w[2].0);
        assert!(w.iter().all(|&(_, len)| (len - 15.0).abs() < 1e-9));
    }

    #[test]
    fn sample_windows_collapse_for_short_files() {
        let w = sample_windows(10.0, 3, 15.0);
        assert_eq!(w, vec![(0.0, 10.0)]);
        assert!(sample_windows(0.0, 3, 15.0).is_empty());
    }

    #[test]
    fn sample_windows_stay_in_bounds() {
        let w = sample_windows(40.0, 3, 15.0);
        for (start, len) in w {
            assert!(start >= 0.0);
            assert!(start + len <= 40.0 + 1e-9);
        }
    }

    #[test]
    fn max_probes_bounds_the_search() {
        // A 28-wide range binary-searches in ≤ floor(log2(28))+1 = 5 probes, +1
        // fallback = 6. It must be an upper bound on what `search_crf` actually spends.
        let bounds = CrfBounds { min: 18, max: 45 };
        let cap = max_probes(bounds);
        assert_eq!(cap, 6);
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(bounds, 72.0, &measure).unwrap();
        assert!(r.probes <= cap, "probes {} > cap {}", r.probes, cap);
    }

    #[test]
    fn bounds_center_on_codec_base() {
        let av1 = bounds_for(Codec::Av1); // base 30
        assert!(av1.min < 30 && av1.max > 30);
        assert!(av1.min >= 1);
    }

    #[test]
    fn parses_vmaf_score_from_ffmpeg_stderr() {
        let s = "frame= ...\n[Parsed_libvmaf_0 @ 0x55] VMAF score: 96.421337\n";
        assert!((parse_vmaf_score(s).unwrap() - 96.421337).abs() < 1e-6);
        assert!(parse_vmaf_score("no score here").is_none());
    }
}

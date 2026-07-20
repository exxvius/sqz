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

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::config::{Codec, Config};
use super::encode::{encoder_rate_args, needs_downscale, pix_fmt, software_scale_vf};
use super::encoders::Encoder;
use super::ffbin::FfBin;
use super::probe::MediaInfo;
use super::util::command_no_window;

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
    /// Whether a CRF meeting the target was actually found. `false` means the
    /// target was unreachable even at the best quality (`crf == bounds.min`), so
    /// `vmaf` is the best achievable — the caller should not treat `crf` as a real
    /// answer (encoding at it would be near-lossless, often bigger than the source).
    pub met_target: bool,
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

/// Choose how many sample windows to take and how long each is, from the source
/// resolution. Large frames (4K/8K/VR) cost far more to decode, encode, and score,
/// so we take fewer, shorter samples there to keep the search time tractable —
/// trading a little VMAF stability for a search that actually finishes. Pure.
pub fn sample_plan(width: Option<u32>, height: Option<u32>) -> (usize, f64) {
    let px = width.unwrap_or(1920) as u64 * height.unwrap_or(1080) as u64;
    const MP: u64 = 1_000_000;
    // Always take enough windows (>=4) that one pathological scene — an intro,
    // title card, fade or scene-cut that VMAF scores oddly low and flat across CRF
    // — can be trimmed as an outlier instead of dragging the whole result into a
    // bottom-out. Windows shorten as the frame grows, to bound decode/encode cost.
    match px {
        p if p > 16 * MP => (4, 6.0),  // 6K/8K/VR
        p if p > 4 * MP => (4, 10.0),  // > 1080p up to ~4K
        _ => (4, 12.0),                // <= 1080p
    }
}

/// Aggregate per-sample VMAF into one figure for the search. With enough samples
/// (>=4), trim the single worst window: a scene that scores pathologically low and
/// *flat across CRF* is a measurement artifact (intro/title/scene-cut), and with
/// few samples that one outlier would bottom the search out. Fewer samples fall
/// back to a plain mean.
pub fn aggregate_vmaf(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    if scores.len() >= 4 {
        let mut s = scores.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let kept = &s[1..]; // drop the lowest
        return kept.iter().sum::<f64>() / kept.len() as f64;
    }
    scores.iter().sum::<f64>() / scores.len() as f64
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

    let (crf, vmaf, met_target) = match best {
        Some((c, v)) => (c, v, true),
        None => {
            // Nothing met the target; report the best achievable (min CRF) so the
            // caller can decide (it should NOT encode near-lossless — see below).
            let v = measure_cached(bounds.min, measure, &mut cache, &mut probes)?;
            (bounds.min, v, false)
        }
    };
    Some(VmafResult {
        crf,
        vmaf,
        probes,
        met_target,
    })
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

/// Resolve the per-title CRF for `info` that hits `target` VMAF. Returns `None`
/// (caller falls back to preset quality) on cancellation or any measurement
/// failure — never a hard error.
///
/// **Extract-once:** each window is decoded a single time into a frame-exact,
/// output-resolution lossless-ish reference; every probe then encodes *that*
/// reference and scores against it. Distorted and reference therefore share
/// identical frames — no independent `-ss` re-seek drift, which previously made
/// VMAF read far too low on high-motion/VFR sources (bottoming the search out at
/// max quality). It also decodes the heavy source once, not per probe.
///
/// `on_progress(frac)` advances continuously (0.0–1.0) — through extraction and
/// through each probe's encode+measure — against a stable upper bound, so it only
/// moves forward.
#[allow(clippy::too_many_arguments)]
pub fn resolve_crf(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    info: &MediaInfo,
    target: f64,
    temp_dir: &Path,
    cancel: &(dyn Fn() -> bool + Sync),
    on_progress: &dyn Fn(f64),
) -> Option<VmafResult> {
    let duration = info.duration.unwrap_or(0.0);
    // Auto plan from the resolution, overridden by the user's explicit choices
    // (a speed vs. accuracy knob): 0 means "auto" for either dimension.
    let (auto_n, auto_secs) = sample_plan(info.width, info.height);
    let samples = if cfg.vmaf_samples > 0 { cfg.vmaf_samples } else { auto_n };
    let secs = if cfg.vmaf_sample_secs > 0.0 { cfg.vmaf_sample_secs } else { auto_secs };
    let windows = sample_windows(duration, samples, secs);
    if windows.is_empty() {
        return None;
    }
    let bounds = bounds_for(cfg.codec);
    let n = windows.len();
    tracing::info!(
        target,
        codec = ?cfg.codec,
        windows = ?windows,
        crf_range = ?(bounds.min, bounds.max),
        "VMAF search: sampling {} window(s)", n
    );
    // Work units: one to extract each window's reference, then one per sample
    // across the worst-case probe count.
    let total_units = (n + max_probes(bounds) as usize * n) as f64;
    let completed = std::cell::Cell::new(0u32);
    on_progress(0.0);

    // Phase 1 — extract each window's reference once (the heavy source decode).
    let mut refs: Vec<(PathBuf, f64)> = Vec::with_capacity(n);
    for &(start, len) in &windows {
        if cancel() {
            cleanup_refs(&refs);
            return None;
        }
        let refp = temp_dir.join(format!("sqz_vref_{}.mkv", uuid::Uuid::new_v4().simple()));
        let base = completed.get() as f64;
        let mut on_ext = |p: f64| on_progress(((base + p) / total_units).min(0.999));
        if !extract_ref(ff, cfg, encoder, info, start, len, &refp, cancel, &mut on_ext) {
            let _ = std::fs::remove_file(&refp);
            cleanup_refs(&refs);
            return None;
        }
        refs.push((refp, len));
        completed.set(completed.get() + 1);
        on_progress((completed.get() as f64 / total_units).min(0.999));
    }

    // Phase 2 — search: encode each reference at the candidate CRF, score against
    // that same reference (perfectly aligned).
    let measure = |crf: i32| -> Option<f64> {
        if cancel() {
            return None;
        }
        let mut scores = Vec::with_capacity(refs.len());
        for (refp, len) in &refs {
            if cancel() {
                return None;
            }
            let dist = temp_dir.join(format!("sqz_vmaf_{}.mkv", uuid::Uuid::new_v4().simple()));
            let base = completed.get() as f64;
            let mut on_enc = |p: f64| on_progress(((base + 0.5 * p) / total_units).min(0.999));
            let encoded =
                encode_from_ref(ff, cfg, encoder, info, refp, *len, crf, &dist, cancel, &mut on_enc);
            if !encoded {
                let _ = std::fs::remove_file(&dist);
                return None;
            }
            let mut on_meas = |p: f64| on_progress(((base + 0.5 + 0.5 * p) / total_units).min(0.999));
            let score = measure_pair(ff, refp, &dist, *len, cancel, &mut on_meas);
            let _ = std::fs::remove_file(&dist);
            let s = score?;
            completed.set(completed.get() + 1);
            on_progress((completed.get() as f64 / total_units).min(0.999));
            scores.push(s);
        }
        if scores.is_empty() {
            return None;
        }
        let vmaf = aggregate_vmaf(&scores);
        // Log the aggregate AND every per-sample score, so a suspicious result (e.g.
        // one window pinned low and flat across CRF) is visible and checkable.
        tracing::info!(crf, vmaf, per_sample = ?scores, "VMAF search: probe");
        Some(vmaf)
    };

    let result = search_crf(bounds, target, &measure);
    if let Some(r) = &result {
        tracing::info!(
            crf = r.crf,
            vmaf = r.vmaf,
            probes = r.probes,
            met_target = r.met_target,
            "VMAF search: resolved"
        );
    }
    cleanup_refs(&refs);
    result
}

/// Remove temp reference files, ignoring errors.
fn cleanup_refs(refs: &[(PathBuf, f64)]) {
    for (p, _) in refs {
        let _ = std::fs::remove_file(p);
    }
}

/// Spawn ffmpeg, streaming `-progress pipe:1` (stdout) into `on_sec` (seconds
/// processed so far) while draining stderr concurrently — so a slow/verbose child
/// (libvmaf on a 4K clip) can't fill the stderr pipe and deadlock. Kills the child
/// promptly if `cancel` fires. Returns `(success, stderr)`, or `None` if cancelled
/// or unspawnable.
fn run_ff(
    mut cmd: Command,
    cancel: &dyn Fn() -> bool,
    on_sec: &mut dyn FnMut(f64),
) -> Option<(bool, String)> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;

    // One thread parses progress into an atomic; another drains stderr to a
    // buffer. Both keep their pipes empty so ffmpeg never blocks writing.
    let latest_ms = Arc::new(AtomicU64::new(0));
    let stdout = child.stdout.take();
    let lw = Arc::clone(&latest_ms);
    let prog = std::thread::spawn(move || {
        if let Some(s) = stdout {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                if let Some(("out_time_us", v)) = line.split_once('=') {
                    if let Ok(us) = v.trim().parse::<i64>() {
                        lw.store((us.max(0) / 1000) as u64, Ordering::Relaxed);
                    }
                }
            }
        }
    });
    let stderr = child.stderr.take();
    let errt = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut s) = stderr {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    });

    let mut last_ms = u64::MAX;
    let ok = loop {
        if cancel() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = prog.join();
            let _ = errt.join();
            return None;
        }
        // Report only when ffmpeg's position advanced (it updates ~5×/s, we poll
        // ~25×/s), so we don't emit redundant progress events.
        let ms = latest_ms.load(Ordering::Relaxed);
        if ms != last_ms {
            last_ms = ms;
            on_sec(ms as f64 / 1000.0);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => {
                let _ = prog.join();
                let _ = errt.join();
                return None;
            }
        }
    };
    let _ = prog.join();
    let err = errt.join().unwrap_or_default();
    Some((ok, err))
}

/// libvmaf thread count — use the machine's parallelism so 4K measurement isn't
/// single-threaded (otherwise the slowest part of the search by far).
fn vmaf_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Decode-side hardware acceleration for sample work — GPU-decodes big sources
/// (4K/8K) instead of chewing CPU, which is the dominant cost on VR-scale files.
/// `-hwaccel auto` decodes to system memory and falls back to software on its own,
/// so it composes with the software scale/VMAF filters. Gated on the run's existing
/// `hardware_decode` setting.
fn hwaccel_args(cfg: &Config) -> &'static [&'static str] {
    if cfg.hardware_decode {
        &["-hwaccel", "auto"]
    } else {
        &[]
    }
}

/// Extract one window from the (possibly huge) source ONCE into a frame-exact,
/// output-resolution reference. Hardware-decodes the heavy source and applies the
/// same downscale the real encode will, so the reference frames are exactly what
/// the target encoder will see. Encoded near-losslessly (libx264 CRF 10 ultrafast)
/// so it's small/fast yet a faithful VMAF reference, at the same pixel format the
/// distorted encode will use. Streams progress; cancellable.
#[allow(clippy::too_many_arguments)]
fn extract_ref(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    info: &MediaInfo,
    start: f64,
    len: f64,
    out: &Path,
    cancel: &dyn Fn() -> bool,
    report: &mut dyn FnMut(f64),
) -> bool {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-stats_period".into(),
        "0.2".into(),
    ];
    a.extend(hwaccel_args(cfg).iter().map(|s| s.to_string()));
    a.extend(
        [
            "-ss",
            &format!("{start}"),
            "-t",
            &format!("{len}"),
            "-i",
            &info.path.to_string_lossy(),
            "-map",
            "0:v:0",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    if needs_downscale(cfg, info) {
        a.push("-vf".into());
        a.push(software_scale_vf(cfg));
    }
    a.extend(
        ["-c:v", "libx264", "-preset", "ultrafast", "-crf", "10", "-pix_fmt"]
            .iter()
            .map(|s| s.to_string()),
    );
    a.push(pix_fmt(cfg, info, encoder).into());
    a.push("-an".into());
    a.push(out.to_string_lossy().into_owned());

    let mut cmd = command_no_window(&ff.ffmpeg);
    cmd.args(&a);
    let span = len.max(0.001);
    let mut on_sec = |sec: f64| report((sec / span).clamp(0.0, 1.0));
    let ok = matches!(run_ff(cmd, cancel, &mut on_sec), Some((true, _)));
    ok && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false)
}

/// Encode a pre-extracted reference at `crf` with the target encoder (no scaling —
/// the reference is already at output resolution and pixel format, exactly what the
/// real encode ingests). Streams progress; cancellable.
#[allow(clippy::too_many_arguments)]
fn encode_from_ref(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    info: &MediaInfo,
    reference: &Path,
    len: f64,
    crf: i32,
    out: &Path,
    cancel: &dyn Fn() -> bool,
    report: &mut dyn FnMut(f64),
) -> bool {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-stats_period".into(),
        "0.2".into(),
        "-i".into(),
        reference.to_string_lossy().into_owned(),
        "-map".into(),
        "0:v:0".into(),
        "-c:v".into(),
        encoder.name.clone(),
    ];
    a.extend(encoder_rate_args(cfg, encoder, crf));
    a.push("-pix_fmt".into());
    a.push(pix_fmt(cfg, info, encoder).into());
    a.push("-an".into());
    a.push(out.to_string_lossy().into_owned());

    let mut cmd = command_no_window(&ff.ffmpeg);
    cmd.args(&a);
    let span = len.max(0.001);
    let mut on_sec = |sec: f64| report((sec / span).clamp(0.0, 1.0));
    let ok = matches!(run_ff(cmd, cancel, &mut on_sec), Some((true, _)));
    ok && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false)
}

/// Score the distorted encode against its own reference — same frames, same
/// resolution, so they're perfectly aligned (no scaling, no re-seek). Distorted is
/// input 0, reference input 1 (ffmpeg's libvmaf convention, as in `verify.rs`).
/// Multi-threaded; progress-streamed; cancellable.
fn measure_pair(
    ff: &FfBin,
    reference: &Path,
    dist: &Path,
    len: f64,
    cancel: &dyn Fn() -> bool,
    report: &mut dyn FnMut(f64),
) -> Option<f64> {
    let filter = format!("[0:v][1:v]libvmaf=n_threads={}", vmaf_threads());
    let mut cmd = command_no_window(&ff.ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-progress", "pipe:1", "-i"])
        .arg(dist)
        .arg("-i")
        .arg(reference)
        .args(["-filter_complex", &filter, "-an", "-f", "null", "-"]);

    let span = len.max(0.001);
    let mut on_sec = |sec: f64| report((sec / span).clamp(0.0, 1.0));
    let (ok, err) = run_ff(cmd, cancel, &mut on_sec)?;
    ok.then(|| parse_vmaf_score(&err)).flatten()
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
        assert!(r.met_target);
    }

    #[test]
    fn search_clamps_to_min_when_target_unreachable() {
        // Even best quality in-range (crf=20 → vmaf 80) misses target 95.
        let measure = |crf: i32| Some(100.0 - crf as f64);
        let r = search_crf(CrfBounds { min: 20, max: 45 }, 95.0, &measure).unwrap();
        assert_eq!(r.crf, 20);
        assert!((r.vmaf - 80.0).abs() < 1e-9);
        assert!(!r.met_target); // unreachable — the caller must not encode at min
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
    fn sample_plan_takes_enough_windows_to_trim_outliers() {
        // Every tier takes >=4 windows so one bad window can be trimmed; larger
        // frames use shorter windows to bound cost.
        assert_eq!(sample_plan(Some(1920), Some(1080)), (4, 12.0));
        assert_eq!(sample_plan(Some(3840), Some(2160)), (4, 10.0));
        assert_eq!(sample_plan(Some(7680), Some(4320)), (4, 6.0));
        assert_eq!(sample_plan(None, None), (4, 12.0));
    }

    #[test]
    fn aggregate_trims_one_low_outlier() {
        // One pathological window is dropped; the rest are averaged.
        let m = aggregate_vmaf(&[78.0, 96.0, 95.0, 97.0]);
        assert!((m - 96.0).abs() < 1e-9); // mean of 95, 96, 97
        // Fewer than four → plain mean (nothing trimmed).
        assert!((aggregate_vmaf(&[78.0, 96.0]) - 87.0).abs() < 1e-9);
        assert_eq!(aggregate_vmaf(&[]), 0.0);
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

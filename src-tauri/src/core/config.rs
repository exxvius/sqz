//! Run configuration and its defaults.
//!
//! Precedence: built-in defaults < persisted settings < per-run overrides from
//! the UI. Everything here is UI-agnostic and directly testable.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Downscale only when a source is taller than this; never upscale.
pub const DEFAULT_MAX_HEIGHT: u32 = 1080;
/// Require the output to be at least this fraction smaller, else keep original.
pub const DEFAULT_MIN_SAVINGS: f64 = 0.10;
/// Concurrent FFmpeg jobs (2–3 is ideal on a single hardware encoder).
pub const DEFAULT_WORKERS: usize = 2;
/// Staged early-abort checkpoints (progress fractions) and thresholds.
/// Stage 1: at 5%, bail if the projection is badly bloated (+25% or more).
/// Stage 2: at 10%, bail if already under the savings gate AND getting worse.
/// Stage 3: 25%–75%, bail the moment the projection drops under the gate.
/// Stage 4: after 75%, only bail if savings fall under a small floor (3%).
pub const DEFAULT_ABORT_STAGE1_AT: f64 = 0.05;
pub const DEFAULT_ABORT_BLOAT_MARGIN: f64 = 0.25;
pub const DEFAULT_ABORT_CHECK_AT: f64 = 0.10;
pub const DEFAULT_ABORT_LATE_AT: f64 = 0.75;
pub const DEFAULT_ABORT_LATE_MIN_SAVINGS: f64 = 0.03;
/// Predictive skip threshold (bits per pixel); sources already below this are
/// unlikely to shrink meaningfully.
pub const DEFAULT_MARGINAL_BPP: f64 = 0.05;

/// Decode-verify probe window (seconds) unless `paranoid` is set.
pub const DECODE_PROBE_SECONDS: u32 = 5;
/// Duration match tolerances (the looser of the two wins).
pub const DURATION_TOLERANCE_S: f64 = 1.0;
pub const DURATION_TOLERANCE_FRAC: f64 = 0.005;

pub const TEMP_DIRNAME: &str = ".sqz_tmp";
pub const HOLDING_DIRNAME: &str = ".sqz_originals";

/// Container extensions treated as input.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "m4v", "mpg", "mpeg", "ts", "m2ts",
    "mts", "webm", "vob", "3gp", "3g2", "divx", "ogv", "rm", "rmvb", "asf", "f4v",
    "m2v", "mxf",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Av1,
    Hevc,
    H264,
}

impl Codec {
    /// FFmpeg canonical `codec_name` values ffprobe reports for this target,
    /// used to decide "already in the target codec".
    pub fn probe_names(self) -> &'static [&'static str] {
        match self {
            Codec::Av1 => &["av1"],
            Codec::Hevc => &["hevc", "h265"],
            Codec::H264 => &["h264", "avc"],
        }
    }

    /// Balanced base quality (CRF-like, lower = better). Preset offsets apply.
    pub fn base_quality(self) -> i32 {
        match self {
            Codec::Av1 => 30,
            Codec::Hevc => 25,
            Codec::H264 => 23,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QualityPreset {
    MaxSavings,
    Balanced,
    HighQuality,
    VisuallyLossless,
}

impl QualityPreset {
    /// Offset applied to the codec's base quality. Higher CRF = smaller/worse,
    /// so "max savings" is a positive offset and quality presets are negative.
    pub fn quality_offset(self) -> i32 {
        match self {
            QualityPreset::MaxSavings => 6,
            QualityPreset::Balanced => 0,
            QualityPreset::HighQuality => -5,
            QualityPreset::VisuallyLossless => -10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnSuccess {
    /// Send the original to the OS Recycle Bin / Trash (recoverable). Default.
    Recycle,
    /// Move the original into a mirrored holding folder.
    Holding,
    /// Permanently delete the original (still verified-first + recoverable
    /// during the swap window).
    Delete,
}

/// Resolved settings for one processing run. `#[serde(default)]` lets the UI
/// send only the fields it wants to override.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub inputs: Vec<PathBuf>,
    pub codec: Codec,
    pub quality: QualityPreset,
    /// Optional raw quality override (advanced mode). When set, it replaces the
    /// preset-derived value directly (CRF-like, lower = better).
    pub quality_override: Option<i32>,
    /// Optional explicit encoder id (e.g. "av1_nvenc"); `None` = auto-select.
    pub encoder_override: Option<String>,
    pub workers: usize,
    pub min_savings: f64,
    pub max_height: u32,
    pub temp_dir: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    pub on_success: OnSuccess,
    pub holding_dir: Option<PathBuf>,
    pub paranoid: bool,
    pub hwaccel_decode: bool,
    pub dry_run: bool,
    pub force: bool,
    pub skip_marginal: bool,
    pub marginal_bpp: f64,
    pub early_abort: bool,
    pub abort_stage1_at: f64,
    pub abort_bloat_margin: f64,
    pub abort_check_at: f64,
    pub abort_late_at: f64,
    pub abort_late_min_savings: f64,
    pub retry_failed: bool,
    /// Remux skipped/aborted files into the target container (`.mkv`) without
    /// re-encoding, so an entire library ends up in one format.
    pub normalize_container: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            codec: Codec::Av1,
            quality: QualityPreset::Balanced,
            quality_override: None,
            encoder_override: None,
            workers: DEFAULT_WORKERS,
            min_savings: DEFAULT_MIN_SAVINGS,
            max_height: DEFAULT_MAX_HEIGHT,
            temp_dir: None,
            db_path: None,
            on_success: OnSuccess::Recycle,
            holding_dir: None,
            paranoid: false,
            hwaccel_decode: false,
            dry_run: false,
            force: false,
            skip_marginal: false,
            marginal_bpp: DEFAULT_MARGINAL_BPP,
            early_abort: true,
            abort_stage1_at: DEFAULT_ABORT_STAGE1_AT,
            abort_bloat_margin: DEFAULT_ABORT_BLOAT_MARGIN,
            abort_check_at: DEFAULT_ABORT_CHECK_AT,
            abort_late_at: DEFAULT_ABORT_LATE_AT,
            abort_late_min_savings: DEFAULT_ABORT_LATE_MIN_SAVINGS,
            retry_failed: true,
            normalize_container: false,
        }
    }
}

impl Config {
    /// Resolved CRF-like quality for this run (override wins over the preset).
    pub fn resolved_quality(&self) -> i32 {
        self.quality_override
            .unwrap_or_else(|| self.codec.base_quality() + self.quality.quality_offset())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..1.0).contains(&self.min_savings) {
            return Err("min_savings must be in [0, 1)".into());
        }
        if !(0.0 < self.abort_check_at && self.abort_check_at < 1.0) {
            return Err("abort_check_at must be in (0, 1)".into());
        }
        if self.workers < 1 {
            return Err("workers must be >= 1".into());
        }
        if matches!(self.on_success, OnSuccess::Holding) && self.holding_dir.is_none() {
            return Err("on_success=holding requires holding_dir".into());
        }
        Ok(())
    }

    /// True if `ext` (without dot, any case) is a recognized video extension.
    pub fn is_video_ext(ext: &str) -> bool {
        let e = ext.to_ascii_lowercase();
        VIDEO_EXTENSIONS.contains(&e.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_quality_uses_codec_base() {
        let mut cfg = Config::default();
        cfg.codec = Codec::Av1;
        assert_eq!(cfg.resolved_quality(), 30);
        cfg.codec = Codec::Hevc;
        assert_eq!(cfg.resolved_quality(), 25);
    }

    #[test]
    fn override_wins_over_preset() {
        let cfg = Config {
            quality_override: Some(18),
            ..Config::default()
        };
        assert_eq!(cfg.resolved_quality(), 18);
    }

    #[test]
    fn quality_presets_move_the_right_direction() {
        let hq = Config {
            quality: QualityPreset::HighQuality,
            ..Config::default()
        };
        let save = Config {
            quality: QualityPreset::MaxSavings,
            ..Config::default()
        };
        assert!(hq.resolved_quality() < save.resolved_quality());
    }

    #[test]
    fn validate_rejects_bad_savings() {
        let cfg = Config {
            min_savings: 1.5,
            ..Config::default()
        };
        assert!(cfg.validate().is_err());
    }
}

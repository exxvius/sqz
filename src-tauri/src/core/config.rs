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

/// Decode-verify probe window (seconds) for the fast verification depth.
pub const DECODE_PROBE_SECONDS: u32 = 5;
/// Default audio bitrate (kbit/s) when transcoding audio.
pub const DEFAULT_AUDIO_KBPS: u32 = 128;
/// Holding retention: 0 means keep originals forever.
pub const DEFAULT_HOLDING_RETENTION_DAYS: u32 = 0;
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

/// Output container. MKV is the safe default (holds anything); MP4 is offered
/// for players/TVs that need it, with the format's stream limits handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mkv,
    Mp4,
}

impl Container {
    /// Lowercase file extension for this container (no dot).
    pub fn ext(self) -> &'static str {
        match self {
            Container::Mkv => "mkv",
            Container::Mp4 => "mp4",
        }
    }
}

/// What to do with audio streams on re-encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    /// Stream-copy audio untouched (default, lossless, fastest).
    Copy,
    /// Transcode to Opus (best efficiency; not valid in MP4).
    Opus,
    /// Transcode to AAC (broad compatibility).
    Aac,
}

/// How deeply to verify an output before trusting it enough to replace the
/// original. Stricter costs more time but closes more silent-corruption paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyDepth {
    /// Decode the first and last few seconds of video.
    Fast,
    /// Fully decode the video stream.
    Thorough,
    /// Fully decode every stream (video + audio) and hash it.
    Checksummed,
}

/// Order in which pending files are processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Order {
    /// Default resume order (by when the file was last touched).
    Smart,
    /// Biggest files first — reclaims space fastest.
    LargestFirst,
    SmallestFirst,
    OldestFirst,
    NewestFirst,
}

impl Order {
    /// SQL `ORDER BY` fragment (column + direction) for the claim query.
    pub fn sql(self) -> &'static str {
        match self {
            Order::Smart => "updated_at ASC",
            Order::LargestFirst => "size DESC",
            Order::SmallestFirst => "size ASC",
            Order::OldestFirst => "mtime ASC",
            Order::NewestFirst => "mtime DESC",
        }
    }
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
    /// Delete held originals older than this many days (0 = keep forever).
    pub holding_retention_days: u32,
    /// Output container (defaults to MKV).
    pub container: Container,
    /// Audio handling (defaults to stream copy).
    pub audio_mode: AudioMode,
    pub audio_bitrate_kbps: u32,
    /// Verification depth. `paranoid` (legacy) forces at least `Thorough`.
    pub verify_depth: VerifyDepth,
    /// Optional minimum SSIM (0..1) the output must reach vs the source, else the
    /// original is kept. `None` disables the perceptual-quality gate.
    pub ssim_floor: Option<f64>,
    /// Skip Dolby Vision sources rather than risk dropping the DV layer.
    pub skip_dolby_vision: bool,
    /// Processing order for pending files.
    pub order: Order,
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
            holding_retention_days: DEFAULT_HOLDING_RETENTION_DAYS,
            container: Container::Mkv,
            audio_mode: AudioMode::Copy,
            audio_bitrate_kbps: DEFAULT_AUDIO_KBPS,
            verify_depth: VerifyDepth::Fast,
            ssim_floor: None,
            skip_dolby_vision: true,
            order: Order::Smart,
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

    /// Effective verification depth: the legacy `paranoid` flag forces at least
    /// `Thorough`, so older UIs/settings keep their stronger guarantee.
    pub fn resolved_verify_depth(&self) -> VerifyDepth {
        match (self.paranoid, self.verify_depth) {
            (true, VerifyDepth::Fast) => VerifyDepth::Thorough,
            (_, depth) => depth,
        }
    }

    /// Effective worker count. `workers == 0` means "auto": a sensible fraction
    /// of the detected cores, clamped so a big machine doesn't spawn a hundred
    /// FFmpeg processes and a tiny one still makes progress.
    pub fn resolved_workers(&self) -> usize {
        if self.workers >= 1 {
            return self.workers;
        }
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        (cores / 2).clamp(1, 8)
    }

    /// Whether Opus audio is valid in the chosen container (MP4 cannot mux Opus
    /// in a broadly compatible way, so we fall back to AAC there).
    pub fn effective_audio_mode(&self) -> AudioMode {
        match (self.container, self.audio_mode) {
            (Container::Mp4, AudioMode::Opus) => AudioMode::Aac,
            (_, mode) => mode,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..1.0).contains(&self.min_savings) {
            return Err("min_savings must be in [0, 1)".into());
        }
        if !(0.0 < self.abort_check_at && self.abort_check_at < 1.0) {
            return Err("abort_check_at must be in (0, 1)".into());
        }
        // workers == 0 is the "auto" sentinel; any positive value is explicit.
        if let Some(floor) = self.ssim_floor {
            if !(0.0..=1.0).contains(&floor) {
                return Err("ssim_floor must be in [0, 1]".into());
            }
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

    #[test]
    fn paranoid_forces_at_least_thorough() {
        let cfg = Config { paranoid: true, verify_depth: VerifyDepth::Fast, ..Config::default() };
        assert_eq!(cfg.resolved_verify_depth(), VerifyDepth::Thorough);
        // An explicit stronger depth is preserved.
        let cfg = Config { paranoid: true, verify_depth: VerifyDepth::Checksummed, ..Config::default() };
        assert_eq!(cfg.resolved_verify_depth(), VerifyDepth::Checksummed);
        // Without paranoid, the chosen depth stands.
        let cfg = Config { paranoid: false, verify_depth: VerifyDepth::Fast, ..Config::default() };
        assert_eq!(cfg.resolved_verify_depth(), VerifyDepth::Fast);
    }

    #[test]
    fn auto_workers_resolves_to_a_sane_positive_count() {
        let cfg = Config { workers: 0, ..Config::default() };
        let n = cfg.resolved_workers();
        assert!((1..=8).contains(&n));
        // Explicit worker counts pass through untouched.
        let cfg = Config { workers: 5, ..Config::default() };
        assert_eq!(cfg.resolved_workers(), 5);
    }

    #[test]
    fn opus_falls_back_to_aac_in_mp4() {
        let cfg = Config { container: Container::Mp4, audio_mode: AudioMode::Opus, ..Config::default() };
        assert_eq!(cfg.effective_audio_mode(), AudioMode::Aac);
        let cfg = Config { container: Container::Mkv, audio_mode: AudioMode::Opus, ..Config::default() };
        assert_eq!(cfg.effective_audio_mode(), AudioMode::Opus);
    }

    #[test]
    fn container_extensions_and_order_sql() {
        assert_eq!(Container::Mkv.ext(), "mkv");
        assert_eq!(Container::Mp4.ext(), "mp4");
        assert_eq!(Order::LargestFirst.sql(), "size DESC");
        assert_eq!(Order::OldestFirst.sql(), "mtime ASC");
    }

    #[test]
    fn validate_rejects_ssim_floor_out_of_range() {
        let cfg = Config { ssim_floor: Some(1.5), ..Config::default() };
        assert!(cfg.validate().is_err());
        let cfg = Config { ssim_floor: Some(0.95), ..Config::default() };
        assert!(cfg.validate().is_ok());
    }
}

//! FFprobe wrapper → typed [`MediaInfo`].

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

use super::util::command_no_window;

/// mp4/mov timed-text subtitle codecs MKV cannot hold via stream copy; they must
/// be converted (e.g. to srt) on output.
const TEXT_MP4_SUB_CODECS: &[&str] = &["mov_text", "tx3g"];

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("ffprobe failed to launch: {0}")]
    Launch(String),
    #[error("ffprobe rc={0}: {1}")]
    NonZero(i32, String),
    #[error("ffprobe produced no output")]
    Empty,
    #[error("ffprobe returned invalid JSON: {0}")]
    Json(String),
    #[error("no video stream found")]
    NoVideo,
}

/// Typed probe result, including a few computed properties.
#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pix_fmt: Option<String>,
    pub duration: Option<f64>,
    /// bits/sec, best-effort.
    pub video_bitrate: Option<u64>,
    pub fps: Option<f64>,
    pub size: Option<u64>,
    pub sub_codecs: Vec<String>,
    /// Color characteristics, carried through on re-encode so HDR is never
    /// silently reinterpreted as SDR. `None`/"unknown" ⇒ leave to the encoder.
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub color_space: Option<String>,
    pub color_range: Option<String>,
    /// Dolby Vision present (a DOVI configuration record / DV codec tag). Its RPU
    /// enhancement layer is easily dropped on re-encode, so we can skip such files.
    pub dolby_vision: bool,
}

impl MediaInfo {
    /// True for a source that needs a >8-bit output format (10- or 12-bit).
    pub fn is_10bit(&self) -> bool {
        let pf = self.pix_fmt.as_deref().unwrap_or("");
        pf.contains("10") || pf.contains("12")
    }

    /// True for a 12-bit source specifically (a superset check of [`is_10bit`]).
    pub fn is_12bit(&self) -> bool {
        self.pix_fmt.as_deref().unwrap_or("").contains("12")
    }

    /// True when the transfer function marks the source as HDR (PQ or HLG).
    pub fn is_hdr(&self) -> bool {
        matches!(
            self.color_transfer.as_deref(),
            Some("smpte2084") | Some("arib-std-b67")
        )
    }

    pub fn has_text_mp4_subs(&self) -> bool {
        self.sub_codecs
            .iter()
            .any(|c| TEXT_MP4_SUB_CODECS.contains(&c.as_str()))
    }

    /// Bitrate normalized by resolution and framerate (bits per pixel). Low means
    /// the source is already efficiently encoded. `None` when any input is unknown.
    pub fn bits_per_pixel(&self) -> Option<f64> {
        let (br, w, h, fps) = (self.video_bitrate?, self.width?, self.height?, self.fps?);
        let pixels_per_sec = (w as f64) * (h as f64) * fps;
        if pixels_per_sec <= 0.0 {
            return None;
        }
        Some(br as f64 / pixels_per_sec)
    }
}

fn to_u64(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.trim().parse::<u64>().ok())
}

fn to_u32(v: Option<&serde_json::Value>) -> Option<u32> {
    to_u64(v).and_then(|n| u32::try_from(n).ok())
}

fn to_f64(v: Option<&serde_json::Value>) -> Option<f64> {
    let v = v?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    v.as_str().and_then(|s| s.trim().parse::<f64>().ok())
}

/// Detect Dolby Vision on a video stream: either a DOVI configuration record in
/// its side-data list, or a DV codec tag (dvhe/dvh1/dav1/dvav).
fn detect_dolby_vision(video: &serde_json::Value) -> bool {
    let tag_is_dv = video
        .get("codec_tag_string")
        .and_then(serde_json::Value::as_str)
        .map(|t| matches!(t, "dvhe" | "dvh1" | "dav1" | "dvav"))
        .unwrap_or(false);
    if tag_is_dv {
        return true;
    }
    video
        .get("side_data_list")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter().any(|sd| {
                sd.get("side_data_type")
                    .and_then(serde_json::Value::as_str)
                    .map(|t| t.to_ascii_lowercase().contains("dovi") || t.to_ascii_lowercase().contains("dolby vision"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// A color-characteristic string, or `None` when ffprobe reports it as absent or
/// a placeholder ("unknown"/"reserved"/"N/A") we must not pass to the encoder.
fn color_field(v: Option<&serde_json::Value>) -> Option<String> {
    let s = v?.as_str()?.trim();
    if s.is_empty() || matches!(s.to_ascii_lowercase().as_str(), "unknown" | "reserved" | "n/a") {
        return None;
    }
    Some(s.to_string())
}

/// Parse an ffprobe frame-rate string like "30000/1001" or "25/1".
fn parse_fps(v: Option<&serde_json::Value>) -> Option<f64> {
    let s = v?.as_str()?;
    if s.is_empty() || s == "0/0" || s == "N/A" {
        return None;
    }
    if let Some((num, den)) = s.split_once('/') {
        let (num, den) = (num.parse::<f64>().ok()?, den.parse::<f64>().ok()?);
        if den == 0.0 {
            return None;
        }
        return Some(num / den);
    }
    s.parse::<f64>().ok()
}

/// Return [`MediaInfo`] for a media file, or a [`ProbeError`].
pub fn probe(ffprobe: &Path, path: &Path, timeout: Duration) -> Result<MediaInfo, ProbeError> {
    let mut cmd = command_no_window(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path);

    // ffprobe is quick; a hard timeout guards against a hung process on odd input.
    let output = run_with_timeout(cmd, timeout).map_err(|e| ProbeError::Launch(e.to_string()))?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let err = String::from_utf8_lossy(&output.stderr);
        let err: String = err.trim().chars().take(400).collect();
        return Err(ProbeError::NonZero(code, err));
    }
    if output.stdout.is_empty() {
        return Err(ProbeError::Empty);
    }

    let data: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| ProbeError::Json(e.to_string()))?;

    let fmt = &data["format"];
    let empty: Vec<serde_json::Value> = Vec::new();
    let streams = data["streams"].as_array().unwrap_or(&empty);

    // Prefer a real video stream over embedded cover art (an `attached_pic`
    // mjpeg/png stream on audio files reports codec_type=="video" too, and
    // treating it as the video would mis-drive codec/resolution decisions).
    let is_video = |s: &&serde_json::Value| s["codec_type"] == "video";
    let is_cover = |s: &&serde_json::Value| {
        s.get("disposition")
            .and_then(|d| d.get("attached_pic"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
            != 0
    };
    let video = streams
        .iter()
        .find(|s| is_video(s) && !is_cover(s))
        .or_else(|| streams.iter().find(is_video))
        .ok_or(ProbeError::NoVideo)?;

    let duration = to_f64(video.get("duration")).or_else(|| to_f64(fmt.get("duration")));
    let size = to_u64(fmt.get("size"));

    // Best-effort video bitrate: stream tag first, then a BPS tag, else derive
    // from the whole-file size/duration (upper bound; fine for the heuristic).
    let mut v_bitrate = to_u64(video.get("bit_rate"));
    if v_bitrate.is_none() {
        v_bitrate = to_u64(video.get("tags").and_then(|t| t.get("BPS")));
    }
    if v_bitrate.is_none() {
        if let (Some(sz), Some(dur)) = (size, duration) {
            if dur > 0.0 {
                v_bitrate = Some((sz as f64 * 8.0 / dur) as u64);
            }
        }
    }

    let fps = parse_fps(video.get("avg_frame_rate")).or_else(|| parse_fps(video.get("r_frame_rate")));

    let sub_codecs = streams
        .iter()
        .filter(|s| s["codec_type"] == "subtitle")
        .filter_map(|s| s["codec_name"].as_str().map(str::to_string))
        .collect();

    Ok(MediaInfo {
        path: path.to_path_buf(),
        codec: video["codec_name"].as_str().map(str::to_string),
        width: to_u32(video.get("width")),
        height: to_u32(video.get("height")),
        pix_fmt: video["pix_fmt"].as_str().map(str::to_string),
        duration,
        video_bitrate: v_bitrate,
        fps,
        size,
        sub_codecs,
        color_primaries: color_field(video.get("color_primaries")),
        color_transfer: color_field(video.get("color_transfer")),
        color_space: color_field(video.get("color_space")),
        color_range: color_field(video.get("color_range")),
        dolby_vision: detect_dolby_vision(video),
    })
}

/// Run a command with a wall-clock timeout, killing it if it overruns.
fn run_with_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()?;

    let start = std::time::Instant::now();
    loop {
        if let Some(_status) = child.try_wait()? {
            return child.wait_with_output();
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "ffprobe timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> MediaInfo {
        MediaInfo {
            path: PathBuf::from("x.mp4"),
            codec: Some("h264".into()),
            width: Some(1920),
            height: Some(1080),
            pix_fmt: Some("yuv420p".into()),
            duration: Some(60.0),
            video_bitrate: Some(5_000_000),
            fps: Some(30.0),
            size: Some(40_000_000),
            sub_codecs: vec![],
            color_primaries: None,
            color_transfer: None,
            color_space: None,
            color_range: None,
            dolby_vision: false,
        }
    }

    #[test]
    fn detects_10bit_from_pix_fmt() {
        let mut m = base();
        assert!(!m.is_10bit());
        m.pix_fmt = Some("yuv420p10le".into());
        assert!(m.is_10bit());
    }

    #[test]
    fn detects_12bit_and_still_reads_as_high_depth() {
        let mut m = base();
        m.pix_fmt = Some("yuv420p12le".into());
        assert!(m.is_12bit());
        assert!(m.is_10bit()); // 12-bit is a superset of "needs >8-bit output"
    }

    #[test]
    fn hdr_detected_from_transfer() {
        let mut m = base();
        assert!(!m.is_hdr());
        m.color_transfer = Some("smpte2084".into());
        assert!(m.is_hdr());
        m.color_transfer = Some("arib-std-b67".into());
        assert!(m.is_hdr());
        m.color_transfer = Some("bt709".into());
        assert!(!m.is_hdr());
    }

    #[test]
    fn color_field_rejects_placeholders() {
        use serde_json::json;
        assert_eq!(color_field(Some(&json!("bt2020"))), Some("bt2020".into()));
        assert_eq!(color_field(Some(&json!("unknown"))), None);
        assert_eq!(color_field(Some(&json!("N/A"))), None);
        assert_eq!(color_field(Some(&json!(""))), None);
        assert_eq!(color_field(None), None);
    }

    #[test]
    fn bits_per_pixel_matches_formula() {
        let m = base();
        let bpp = m.bits_per_pixel().unwrap();
        let expected = 5_000_000.0 / (1920.0 * 1080.0 * 30.0);
        assert!((bpp - expected).abs() < 1e-9);
    }

    #[test]
    fn bits_per_pixel_none_when_unknown() {
        let mut m = base();
        m.fps = None;
        assert!(m.bits_per_pixel().is_none());
    }

    #[test]
    fn text_mp4_subs_detected() {
        let mut m = base();
        m.sub_codecs = vec!["mov_text".into()];
        assert!(m.has_text_mp4_subs());
        m.sub_codecs = vec!["subrip".into()];
        assert!(!m.has_text_mp4_subs());
    }
}

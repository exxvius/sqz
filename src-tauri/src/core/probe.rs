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
}

impl MediaInfo {
    pub fn is_10bit(&self) -> bool {
        let pf = self.pix_fmt.as_deref().unwrap_or("");
        pf.contains("10") || pf.contains("12")
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

    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
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

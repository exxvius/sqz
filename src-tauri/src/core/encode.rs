//! Build and run the FFmpeg encode command, with rate-control flags that adapt
//! to the selected encoder family.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::config::Config;
use super::encoders::{Encoder, EncoderFamily};
use super::probe::MediaInfo;
use super::util::command_no_window;

/// Outcome of one encode. `returncode == None` ⇒ cancelled or aborted.
#[derive(Debug, Default)]
pub struct EncodeResult {
    pub returncode: Option<i32>,
    pub stderr_tail: String,
    pub cancelled: bool,
    /// Killed early because it wasn't going to beat the size gate.
    pub aborted: bool,
    /// The projection that triggered an early abort, if any.
    pub abort_projection: Option<AbortProjection>,
}

/// Recorded when an in-progress encode is projected to miss the size gate.
#[derive(Debug, Clone, Copy)]
pub struct AbortProjection {
    pub frac: f64,
    pub projected: f64,
}

/// NVENC wants 4:2:0; pick 10-bit (p010le) for 10/12-bit sources else 8-bit.
fn pix_fmt(info: &MediaInfo) -> &'static str {
    if info.is_10bit() {
        "p010le"
    } else {
        "yuv420p"
    }
}

/// Copy subtitles into MKV, converting mp4 timed-text which MKV can't copy.
fn subtitle_args(info: &MediaInfo) -> [&'static str; 2] {
    if info.has_text_mp4_subs() {
        ["-c:s", "srt"]
    } else {
        ["-c:s", "copy"]
    }
}

/// Map VideoToolbox's constant-quality scalar (1..100, higher = better) from the
/// CRF-like value (lower = better). Approximate; VT has no true CRF.
fn videotoolbox_quality(q: i32) -> i32 {
    let vt = 100.0 - (q as f64 / 63.0 * 100.0);
    vt.round().clamp(1.0, 100.0) as i32
}

/// Family-specific rate-control + preset flags for a target quality `q`
/// (CRF-like, lower = better).
fn encoder_rate_args(encoder: &Encoder, q: i32) -> Vec<String> {
    let q = q.to_string();
    match encoder.family {
        EncoderFamily::Nvenc => vec![
            "-preset".into(), "p6".into(),
            "-rc".into(), "vbr".into(),
            "-cq".into(), q,
            "-b:v".into(), "0".into(),
        ],
        EncoderFamily::Qsv => vec![
            "-preset".into(), "slower".into(),
            "-global_quality".into(), q,
        ],
        EncoderFamily::Amf => vec![
            "-quality".into(), "quality".into(),
            "-rc".into(), "cqp".into(),
            "-qp_i".into(), q.clone(),
            "-qp_p".into(), q,
        ],
        EncoderFamily::VideoToolbox => {
            let vt = videotoolbox_quality(q.parse().unwrap_or(50));
            vec!["-q:v".into(), vt.to_string()]
        }
        EncoderFamily::Software => match encoder.name.as_str() {
            "libsvtav1" => vec!["-preset".into(), "6".into(), "-crf".into(), q],
            "libx265" | "libx264" => vec!["-preset".into(), "slow".into(), "-crf".into(), q],
            _ => vec!["-crf".into(), q],
        },
    }
}

/// Build the FFmpeg argument list (excluding the program path itself).
pub fn build_args(cfg: &Config, info: &MediaInfo, encoder: &Encoder, out_path: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];

    // CUDA decode only helps (and is only valid) for the NVENC path.
    if cfg.hwaccel_decode && encoder.family == EncoderFamily::Nvenc {
        a.push("-hwaccel".into());
        a.push("cuda".into());
    }

    a.push("-i".into());
    a.push(info.path.to_string_lossy().into_owned());

    // Map real streams only (all optional): video EXCLUDING attached-pic cover
    // art, audio, subtitles, attachments (fonts); copy metadata + chapters.
    // Data/timecode streams are intentionally dropped (MKV can't mux them).
    a.extend(
        [
            "-map", "0:V?", "-map", "0:a?", "-map", "0:s?", "-map", "0:t?",
            "-map_metadata", "0", "-map_chapters", "0",
        ]
        .iter()
        .map(|s| s.to_string()),
    );

    if let Some(h) = info.height {
        if h > cfg.max_height {
            a.push("-vf".into());
            a.push(format!("scale=-2:{}:flags=lanczos", cfg.max_height));
        }
    }

    a.push("-c".into());
    a.push("copy".into());
    a.push("-c:v".into());
    a.push(encoder.name.clone());
    a.extend(encoder_rate_args(encoder, cfg.resolved_quality()));
    a.push("-pix_fmt".into());
    a.push(pix_fmt(info).into());

    a.extend(subtitle_args(info).iter().map(|s| s.to_string()));

    a.push("-max_muxing_queue_size".into());
    a.push("1024".into());
    a.push(out_path.to_string_lossy().into_owned());
    a
}

/// Progress callback: `(encoded_seconds, current_output_bytes)`. `Send` so the
/// scoped progress thread can own the borrow.
pub type ProgressCb<'a> = &'a mut (dyn FnMut(f64, Option<u64>) + Send);
/// Abort predicate: return `Some(projection)` to kill the encode early. `Sync`
/// so a shared `&` is `Send` into the progress thread.
pub type AbortCb<'a> = &'a (dyn Fn(f64, Option<u64>) -> Option<AbortProjection> + Sync);

/// Run FFmpeg, terminating promptly if `cancel` is set or `should_abort` fires.
///
/// Uses scoped threads so the stderr/progress pumps can borrow `on_progress`
/// and `should_abort` directly — no `'static`/`Arc` gymnastics, and everything
/// is joined before this returns.
pub fn run_encode(
    ffmpeg: &Path,
    args: &[String],
    cancel: &AtomicBool,
    on_progress: ProgressCb<'_>,
    should_abort: Option<AbortCb<'_>>,
) -> EncodeResult {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return EncodeResult {
                returncode: Some(1),
                stderr_tail: format!("failed to launch ffmpeg: {e}"),
                ..Default::default()
            }
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let abort_flag = AtomicBool::new(false);
    let tail: Mutex<VecDeque<String>> = Mutex::new(VecDeque::with_capacity(40));
    let projection: Mutex<Option<AbortProjection>> = Mutex::new(None);

    let mut outcome = thread::scope(|scope| {
        // stderr pump → 40-line ring buffer.
        scope.spawn(|| {
            if let Some(s) = stderr {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    let mut t = tail.lock().unwrap();
                    if t.len() == 40 {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            }
        });

        // progress pump → parse `total_size` / `out_time_us`, fire callback + abort.
        scope.spawn(|| {
            let mut last_size: Option<u64> = None;
            if let Some(s) = stdout {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    if let Some(raw) = line.strip_prefix("total_size=") {
                        last_size = raw.trim().parse::<u64>().ok();
                    } else if let Some(raw) = line.strip_prefix("out_time_us=") {
                        if let Ok(us) = raw.trim().parse::<i64>() {
                            let sec = us as f64 / 1_000_000.0;
                            on_progress(sec, last_size);
                            if let Some(pred) = should_abort {
                                if !abort_flag.load(Ordering::Relaxed) {
                                    if let Some(proj) = pred(sec, last_size) {
                                        *projection.lock().unwrap() = Some(proj);
                                        abort_flag.store(true, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        // Supervise: poll, and kill on cancel or abort.
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    break EncodeResult {
                        returncode: status.code(),
                        ..Default::default()
                    };
                }
                Ok(None) => {}
                Err(e) => {
                    break EncodeResult {
                        returncode: Some(1),
                        stderr_tail: format!("wait failed: {e}"),
                        ..Default::default()
                    };
                }
            }

            let cancelled = cancel.load(Ordering::Relaxed);
            let aborted = abort_flag.load(Ordering::Relaxed);
            if cancelled || aborted {
                let _ = child.kill();
                let _ = child.wait();
                break EncodeResult {
                    returncode: None,
                    cancelled,
                    aborted: aborted && !cancelled,
                    ..Default::default()
                };
            }
            thread::sleep(Duration::from_millis(300));
        }
    });

    outcome.stderr_tail = tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
    outcome.abort_projection = *projection.lock().unwrap();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{Codec, Config};
    use super::super::encoders::Encoder;
    use std::path::PathBuf;

    fn info(height: u32, ten_bit: bool) -> MediaInfo {
        MediaInfo {
            path: PathBuf::from("in.mkv"),
            codec: Some("h264".into()),
            width: Some(1920),
            height: Some(height),
            pix_fmt: Some(if ten_bit { "yuv420p10le".into() } else { "yuv420p".into() }),
            duration: Some(60.0),
            video_bitrate: Some(8_000_000),
            fps: Some(30.0),
            size: Some(60_000_000),
            sub_codecs: vec![],
        }
    }

    fn enc(name: &str, fam: EncoderFamily) -> Encoder {
        Encoder { name: name.into(), family: fam }
    }

    #[test]
    fn nvenc_uses_cq_flags() {
        let cfg = Config { codec: Codec::Av1, ..Config::default() };
        let a = build_args(&cfg, &info(1080, false), &enc("av1_nvenc", EncoderFamily::Nvenc), Path::new("o.mkv"));
        let joined = a.join(" ");
        assert!(joined.contains("-c:v av1_nvenc"));
        assert!(joined.contains("-cq 30"));
        assert!(joined.contains("-rc vbr"));
    }

    #[test]
    fn software_uses_crf() {
        let cfg = Config { codec: Codec::Hevc, ..Config::default() };
        let a = build_args(&cfg, &info(1080, false), &enc("libx265", EncoderFamily::Software), Path::new("o.mkv"));
        let joined = a.join(" ");
        assert!(joined.contains("-c:v libx265"));
        assert!(joined.contains("-crf 25"));
        assert!(joined.contains("-preset slow"));
    }

    #[test]
    fn downscale_only_when_taller() {
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let tall = build_args(&cfg, &info(2160, false), &e, Path::new("o.mkv")).join(" ");
        let short = build_args(&cfg, &info(720, false), &e, Path::new("o.mkv")).join(" ");
        assert!(tall.contains("scale=-2:1080"));
        assert!(!short.contains("scale="));
    }

    #[test]
    fn ten_bit_selects_p010le() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, true), &e, Path::new("o.mkv")).join(" ");
        assert!(a.contains("-pix_fmt p010le"));
    }

    #[test]
    fn keeps_streams_and_drops_data() {
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, Path::new("o.mkv")).join(" ");
        assert!(a.contains("-map 0:V?"));
        assert!(a.contains("-map 0:a?"));
        assert!(a.contains("-map 0:s?"));
        assert!(a.contains("-map_chapters 0"));
    }
}

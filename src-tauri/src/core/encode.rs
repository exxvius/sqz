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

use super::abort::AbortProjection;
use super::config::{AudioMode, BitDepth, Config, Container};
use super::encoders::{Encoder, EncoderFamily};
use super::hwcaps::HwCaps;
use super::probe::MediaInfo;
use super::util::command_no_window;

/// Source codecs NVDEC reliably decodes on Maxwell-and-newer GPUs. A source
/// outside this set (rare/old formats) takes the software-decode path — never a
/// hard failure. Names are ffprobe `codec_name` values.
const NVDEC_CODECS: &[&str] = &[
    "h264",
    "hevc",
    "av1",
    "vp9",
    "vp8",
    "mpeg2video",
    "mpeg4",
    "vc1",
];

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

/// A single ffmpeg progress tick, parsed from `-progress pipe:1`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProgressSample {
    /// Encoded position, seconds.
    pub sec: f64,
    /// Output bytes written so far.
    pub out_bytes: Option<u64>,
    /// Encoding rate, frames/sec.
    pub fps: Option<f64>,
    /// Realtime multiple (e.g. 3.2 = 3.2× realtime).
    pub speed: Option<f64>,
    /// Current output bitrate, kbit/s.
    pub bitrate_kbps: Option<f64>,
}

/// The bit depth the run *wants* for the output: the configured override, or the
/// source's own depth when set to `Source`.
fn target_bit_depth(cfg: &Config, info: &MediaInfo) -> u8 {
    match cfg.bit_depth {
        BitDepth::Eight => 8,
        BitDepth::Ten => 10,
        BitDepth::Source => info.bit_depth(),
    }
}

/// Hardware H.264 encoders (NVENC/QSV/AMF) are 8-bit only; every other encoder we
/// target (all AV1/HEVC, plus libx264/libx265) can do 10-bit.
fn supports_10bit(encoder: &Encoder) -> bool {
    !matches!(
        encoder.name.as_str(),
        "h264_nvenc" | "h264_qsv" | "h264_amf"
    )
}

/// Bit depth a chosen pixel format actually carries (for step-down logging).
fn pf_bit_depth(pf: &str) -> u8 {
    if pf.contains("12") {
        12
    } else if pf.contains("10") || pf == "p010le" {
        10
    } else {
        8
    }
}

/// Choose the 4:2:0 output pixel format for the target bit depth, stepping down
/// (never silently up-truncating) to whatever the chosen encoder can carry:
///
/// - 8-bit → `yuv420p`.
/// - 10-bit → `yuv420p10le` (software) / `p010le` (hardware), or `yuv420p` when
///   the encoder can't do >8-bit (hardware H.264).
/// - 12-bit → `yuv420p12le` only on libx265; otherwise stepped down to 10-bit.
///
/// Any step-down is logged by [`build_args`] so it is never silent.
pub(crate) fn pix_fmt(cfg: &Config, info: &MediaInfo, encoder: &Encoder) -> &'static str {
    let target = target_bit_depth(cfg, info);
    if target <= 8 {
        return "yuv420p";
    }
    if target >= 12 && encoder.name == "libx265" {
        return "yuv420p12le";
    }
    // Want 10-bit (or 12-bit stepped down to 10). Encoders that can't do >8-bit
    // fall back to 8-bit rather than failing the encode.
    if !supports_10bit(encoder) {
        return "yuv420p";
    }
    match encoder.family {
        EncoderFamily::Software => "yuv420p10le",
        _ => "p010le",
    }
}

/// The CUDA surface format NVDEC/scale_cuda/NVENC use for a given bit depth:
/// 8-bit → semi-planar `nv12`, 10-bit → `p010le`.
fn cuda_frame_fmt(depth: u8) -> &'static str {
    if depth >= 10 {
        "p010le"
    } else {
        "nv12"
    }
}

/// True if a downscale to `max_height` is needed for this source.
pub(crate) fn needs_downscale(cfg: &Config, info: &MediaInfo) -> bool {
    info.height.map(|h| h > cfg.max_height).unwrap_or(false)
}

/// The software `-vf` scale expression used when downscaling on the CPU path.
/// Shared so the VMAF sampler scales its reference exactly as the real encode
/// does (otherwise the measured score wouldn't reflect what ships).
pub(crate) fn software_scale_vf(cfg: &Config) -> String {
    format!(
        "scale=-2:{}:flags={}",
        cfg.max_height,
        cfg.scale_filter.flags()
    )
}

/// Decide whether the fully GPU-resident CUDA pipeline (NVDEC → CUDA frames →
/// GPU scale → NVENC, zero host copies) is both valid and beneficial for this
/// file. When it returns false the caller takes the software-decode path, which
/// handles everything correctly — so this is purely a fast-path opt-in.
///
/// Gated on: the setting, an NVENC encoder, a build with `cuda` hwaccel, a source
/// NVDEC can decode, a ≤10-bit target (NVDEC/NVENC cap), no Dolby Vision, and — if
/// the frame needs resizing or a depth change — an available GPU scaler to do it
/// without leaving the GPU.
fn use_cuda_pipeline(cfg: &Config, info: &MediaInfo, encoder: &Encoder, caps: &HwCaps) -> bool {
    if !cfg.hardware_decode
        || encoder.family != EncoderFamily::Nvenc
        || !caps.cuda
        || info.dolby_vision
        || info.is_12bit()
        || target_bit_depth(cfg, info) > 10
    {
        return false;
    }
    let decodable = info
        .codec
        .as_deref()
        .map(|c| NVDEC_CODECS.contains(&c))
        .unwrap_or(false);
    if !decodable {
        return false;
    }
    // A resize or a bit-depth change must run on the GPU too; that needs a GPU
    // scaler. Pure passthrough (no resize, no depth change) needs none.
    let needs_gpu_filter =
        needs_downscale(cfg, info) || target_bit_depth(cfg, info) != info.bit_depth();
    !needs_gpu_filter || caps.gpu_scale()
}

/// The GPU video-filter string for the CUDA pipeline, or `None` when the frames
/// can pass straight from NVDEC to NVENC untouched. Resizes with `scale_cuda`/
/// `scale_npp`, and sets the output surface format only when the depth changes
/// (otherwise the native format passes through, for max filter compatibility).
fn cuda_vf(cfg: &Config, info: &MediaInfo, caps: &HwCaps) -> Option<String> {
    let scaler = caps.scaler()?;
    let downscale = needs_downscale(cfg, info);
    let target = target_bit_depth(cfg, info);
    let depth_change = target != info.bit_depth();
    if !downscale && !depth_change {
        return None;
    }
    let dims = if downscale {
        format!("-2:{}", cfg.max_height)
    } else {
        "iw:ih".to_string()
    };
    let fmt = if depth_change {
        format!(":format={}", cuda_frame_fmt(target))
    } else {
        String::new()
    };
    Some(format!("{scaler}={dims}{fmt}"))
}

/// True for a color-characteristic value worth passing to the encoder.
fn meaningful_color(v: &str) -> bool {
    let v = v.trim();
    !v.is_empty()
        && !matches!(
            v.to_ascii_lowercase().as_str(),
            "unknown" | "reserved" | "n/a"
        )
}

/// Explicit `-color_*` flags echoing the source's characteristics, so the output
/// is tagged correctly (an HDR source must not be re-tagged/interpreted as SDR).
fn color_args(info: &MediaInfo) -> Vec<String> {
    let mut a = Vec::new();
    let pairs = [
        ("-color_primaries", info.color_primaries.as_deref()),
        ("-color_trc", info.color_transfer.as_deref()),
        ("-colorspace", info.color_space.as_deref()),
        ("-color_range", info.color_range.as_deref()),
    ];
    for (flag, val) in pairs {
        if let Some(v) = val {
            if meaningful_color(v) {
                a.push(flag.to_string());
                a.push(v.to_string());
            }
        }
    }
    a
}

/// Subtitle codec flags for the target container. MKV copies subs (converting
/// mp4 timed-text to SRT, which MKV can't copy); MP4 needs `mov_text` for text
/// subs (bitmap subs it can't hold will fail the encode loudly, keeping the
/// original — never silently dropped).
fn subtitle_args(cfg: &Config, info: &MediaInfo) -> [&'static str; 2] {
    match cfg.container {
        Container::Mkv => {
            if info.has_text_mp4_subs() {
                ["-c:s", "srt"]
            } else {
                ["-c:s", "copy"]
            }
        }
        Container::Mp4 => ["-c:s", "mov_text"],
    }
}

/// Audio codec flags for the run's effective audio mode (empty = leave the
/// container-default stream copy in place).
fn audio_args(cfg: &Config) -> Vec<String> {
    let bitrate = format!("{}k", cfg.audio_bitrate_kbps.max(1));
    match cfg.effective_audio_mode() {
        AudioMode::Copy => Vec::new(),
        AudioMode::Opus => vec!["-c:a".into(), "libopus".into(), "-b:a".into(), bitrate],
        AudioMode::Aac => vec!["-c:a".into(), "aac".into(), "-b:a".into(), bitrate],
    }
}

/// Map VideoToolbox's constant-quality scalar (1..100, higher = better) from the
/// CRF-like value (lower = better). Approximate; VT has no true CRF.
fn videotoolbox_quality(q: i32) -> i32 {
    let vt = 100.0 - (q as f64 / 63.0 * 100.0);
    vt.round().clamp(1.0, 100.0) as i32
}

/// Family-specific rate-control + preset flags for a target quality `q`
/// (CRF-like, lower = better). The NVENC `-preset` comes from `cfg.encoder_speed`.
pub(crate) fn encoder_rate_args(cfg: &Config, encoder: &Encoder, q: i32) -> Vec<String> {
    let q = q.to_string();
    match encoder.family {
        EncoderFamily::Nvenc => vec![
            "-preset".into(),
            cfg.encoder_speed.nvenc_preset().into(),
            "-rc".into(),
            "vbr".into(),
            "-cq".into(),
            q,
            "-b:v".into(),
            "0".into(),
        ],
        EncoderFamily::Qsv => vec![
            "-preset".into(),
            "slower".into(),
            "-global_quality".into(),
            q,
        ],
        EncoderFamily::Amf => vec![
            "-quality".into(),
            "quality".into(),
            "-rc".into(),
            "cqp".into(),
            "-qp_i".into(),
            q.clone(),
            "-qp_p".into(),
            q,
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
///
/// Chooses the fastest valid pipeline: a fully GPU-resident CUDA path (NVDEC →
/// CUDA frames → GPU scale → NVENC, no host copies) when [`use_cuda_pipeline`]
/// allows, otherwise software decode (with a software scaler and NVENC upload, or
/// an all-CPU encode). `caps` is the resolved build's probed GPU capabilities.
pub fn build_args(
    cfg: &Config,
    info: &MediaInfo,
    encoder: &Encoder,
    caps: &HwCaps,
    out_path: &Path,
) -> Vec<String> {
    build_args_q(cfg, info, encoder, caps, out_path, cfg.resolved_quality())
}

/// Like [`build_args`] but with the target quality resolved by the caller. VMAF
/// mode substitutes a per-title CRF here; preset mode passes
/// [`Config::resolved_quality`]. Lifting the decision out of `build_args` is the
/// only seam VMAF mode needs — everything downstream is unchanged.
pub fn build_args_q(
    cfg: &Config,
    info: &MediaInfo,
    encoder: &Encoder,
    caps: &HwCaps,
    out_path: &Path,
    quality: i32,
) -> Vec<String> {
    let cuda = use_cuda_pipeline(cfg, info, encoder, caps);

    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];

    // GPU-resident decode: NVDEC decodes straight into CUDA frames that stay on
    // the device through scaling and into NVENC — no GPU→CPU→GPU round trip.
    if cuda {
        a.extend(
            ["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"]
                .iter()
                .map(|s| s.to_string()),
        );
    }

    a.push("-i".into());
    a.push(info.path.to_string_lossy().into_owned());

    // Map real streams only (all optional): video EXCLUDING attached-pic cover
    // art, audio, subtitles; copy metadata + chapters. Attachments (fonts) are
    // mapped for MKV only — MP4 can't hold them. Data/timecode streams are
    // intentionally dropped (neither container muxes them cleanly).
    a.extend(
        ["-map", "0:V?", "-map", "0:a?", "-map", "0:s?"]
            .iter()
            .map(|s| s.to_string()),
    );
    if cfg.container == Container::Mkv {
        a.push("-map".into());
        a.push("0:t?".into());
    }
    a.extend(
        ["-map_metadata", "0", "-map_chapters", "0"]
            .iter()
            .map(|s| s.to_string()),
    );

    // Scaling: on the GPU with scale_cuda/scale_npp (frames never leave the GPU),
    // or the software scaler on the CPU path.
    if cuda {
        if let Some(vf) = cuda_vf(cfg, info, caps) {
            a.push("-vf".into());
            a.push(vf);
        }
    } else if needs_downscale(cfg, info) {
        a.push("-vf".into());
        a.push(software_scale_vf(cfg));
    }

    a.push("-c".into());
    a.push("copy".into());
    a.push("-c:v".into());
    a.push(encoder.name.clone());
    a.extend(encoder_rate_args(cfg, encoder, quality));

    // Pixel format: only on the software path. On the CUDA path the surface format
    // is already correct — native from NVDEC, or set by the GPU scaler's `format=`
    // — and forcing `-pix_fmt` there would insert a needless conversion.
    if !cuda {
        let pf = pix_fmt(cfg, info, encoder);
        let (target, out_depth) = (target_bit_depth(cfg, info), pf_bit_depth(pf));
        if out_depth < target {
            tracing::warn!(
                encoder = %encoder.name,
                target, out_depth,
                "requested {target}-bit output stepped down to {out_depth}-bit: {} can't carry it",
                encoder.name
            );
        }
        a.push("-pix_fmt".into());
        a.push(pf.into());
    }

    // Preserve HDR/color characteristics on re-encode.
    if info.is_hdr() {
        tracing::info!(trc = ?info.color_transfer, "preserving HDR color metadata on re-encode");
    }
    a.extend(color_args(info));

    a.extend(audio_args(cfg));
    a.extend(subtitle_args(cfg, info).iter().map(|s| s.to_string()));

    a.push("-max_muxing_queue_size".into());
    a.push("1024".into());
    if cfg.container == Container::Mp4 {
        // Move the moov atom to the front so the file is streamable/seekable.
        a.push("-movflags".into());
        a.push("+faststart".into());
    }
    a.push(out_path.to_string_lossy().into_owned());
    a
}

/// Build args to remux a source into the target container without re-encoding
/// (stream copy). Used to normalize a library to one format.
pub fn build_remux_args(cfg: &Config, info: &MediaInfo, out_path: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-y".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-i".into(),
        info.path.to_string_lossy().into_owned(),
    ];
    a.extend(
        ["-map", "0:V?", "-map", "0:a?", "-map", "0:s?"]
            .iter()
            .map(|s| s.to_string()),
    );
    if cfg.container == Container::Mkv {
        a.push("-map".into());
        a.push("0:t?".into());
    }
    a.extend(
        ["-map_metadata", "0", "-map_chapters", "0", "-c", "copy"]
            .iter()
            .map(|s| s.to_string()),
    );
    a.extend(subtitle_args(cfg, info).iter().map(|s| s.to_string()));
    a.push("-max_muxing_queue_size".into());
    a.push("1024".into());
    if cfg.container == Container::Mp4 {
        a.push("-movflags".into());
        a.push("+faststart".into());
    }
    a.push(out_path.to_string_lossy().into_owned());
    a
}

/// Progress callback, one call per ffmpeg progress block. `Send` so the scoped
/// progress thread can own the borrow.
pub type ProgressCb<'a> = &'a mut (dyn FnMut(ProgressSample) + Send);
/// Abort predicate: return `Some(projection)` to kill the encode early. `Sync`
/// so a shared `&` is `Send` into the progress thread.
pub type AbortCb<'a> = &'a (dyn Fn(f64, Option<u64>) -> Option<AbortProjection> + Sync);

/// Parse a numeric ffmpeg progress value, stripping a trailing unit and treating
/// "N/A" as unknown.
fn parse_num(raw: &str) -> Option<f64> {
    let s = raw.trim().trim_end_matches("x").trim_end_matches("kbits/s");
    if s.is_empty() || s.eq_ignore_ascii_case("N/A") {
        return None;
    }
    s.parse::<f64>().ok()
}

/// Run FFmpeg, terminating promptly if `cancel` is set or `should_abort` fires.
///
/// Uses scoped threads so the stderr/progress pumps can borrow `on_progress`
/// and `should_abort` directly — no `'static`/`Arc` gymnastics, and everything
/// is joined before this returns.
pub fn run_encode(
    ffmpeg: &Path,
    args: &[String],
    cancel: &(dyn Fn() -> bool + Sync),
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

        // progress pump → accumulate a block's key=value fields, then fire the
        // callback + abort check on the block terminator (`progress=...`).
        scope.spawn(|| {
            let mut sample = ProgressSample::default();
            if let Some(s) = stdout {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    let Some((key, val)) = line.split_once('=') else {
                        continue;
                    };
                    match key {
                        "total_size" => sample.out_bytes = val.trim().parse::<u64>().ok(),
                        "out_time_us" => {
                            if let Ok(us) = val.trim().parse::<i64>() {
                                sample.sec = us as f64 / 1_000_000.0;
                            }
                        }
                        "fps" => sample.fps = parse_num(val),
                        "speed" => sample.speed = parse_num(val),
                        "bitrate" => sample.bitrate_kbps = parse_num(val),
                        "progress" => {
                            on_progress(sample);
                            if let Some(pred) = should_abort {
                                if !abort_flag.load(Ordering::Relaxed) {
                                    if let Some(proj) = pred(sample.sec, sample.out_bytes) {
                                        *projection.lock().unwrap() = Some(proj);
                                        abort_flag.store(true, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                        _ => {}
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

            let cancelled = cancel();
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

    outcome.stderr_tail = tail
        .lock()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    outcome.abort_projection = *projection.lock().unwrap();
    outcome
}

#[cfg(test)]
mod tests {
    use super::super::config::{
        AudioMode, BitDepth, Codec, Config, Container, EncoderSpeed, ScaleFilter,
    };
    use super::super::hwcaps::HwCaps;
    use super::*;

    /// Empty caps → forces the software-decode path (what the pre-GPU tests expect).
    fn sw() -> HwCaps {
        HwCaps::default()
    }
    /// Full CUDA caps → enables the GPU-resident path.
    fn cuda() -> HwCaps {
        HwCaps {
            cuda: true,
            scale_cuda: true,
            scale_npp: false,
        }
    }
    use super::super::encoders::Encoder;
    use std::path::PathBuf;

    fn info(height: u32, ten_bit: bool) -> MediaInfo {
        MediaInfo {
            path: PathBuf::from("in.mkv"),
            codec: Some("h264".into()),
            width: Some(1920),
            height: Some(height),
            pix_fmt: Some(if ten_bit {
                "yuv420p10le".into()
            } else {
                "yuv420p".into()
            }),
            duration: Some(60.0),
            video_bitrate: Some(8_000_000),
            fps: Some(30.0),
            size: Some(60_000_000),
            sub_codecs: vec![],
            color_primaries: None,
            color_transfer: None,
            color_space: None,
            color_range: None,
            dolby_vision: false,
        }
    }

    fn enc(name: &str, fam: EncoderFamily) -> Encoder {
        Encoder {
            name: name.into(),
            family: fam,
        }
    }

    #[test]
    fn nvenc_uses_cq_flags() {
        let cfg = Config {
            codec: Codec::Av1,
            ..Config::default()
        };
        let a = build_args(
            &cfg,
            &info(1080, false),
            &enc("av1_nvenc", EncoderFamily::Nvenc),
            &sw(),
            Path::new("o.mkv"),
        );
        let joined = a.join(" ");
        assert!(joined.contains("-c:v av1_nvenc"));
        assert!(joined.contains("-cq 30"));
        assert!(joined.contains("-rc vbr"));
    }

    #[test]
    fn nvenc_preset_defaults_to_p4_and_follows_encoder_speed() {
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        // Default speed = Balanced → p4.
        let a = build_args(
            &Config::default(),
            &info(1080, false),
            &e,
            &sw(),
            Path::new("o.mkv"),
        )
        .join(" ");
        assert!(a.contains("-preset p4"));
        // Each speed maps to its NVENC preset.
        for (speed, pn) in [
            (EncoderSpeed::Best, "p7"),
            (EncoderSpeed::Good, "p5"),
            (EncoderSpeed::Fastest, "p1"),
        ] {
            let cfg = Config {
                encoder_speed: speed,
                ..Config::default()
            };
            let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mkv")).join(" ");
            assert!(a.contains(&format!("-preset {pn}")), "{speed:?} → {pn}");
        }
    }

    #[test]
    fn software_uses_crf() {
        let cfg = Config {
            codec: Codec::Hevc,
            ..Config::default()
        };
        let a = build_args(
            &cfg,
            &info(1080, false),
            &enc("libx265", EncoderFamily::Software),
            &sw(),
            Path::new("o.mkv"),
        );
        let joined = a.join(" ");
        assert!(joined.contains("-c:v libx265"));
        assert!(joined.contains("-crf 25"));
        assert!(joined.contains("-preset slow"));
    }

    #[test]
    fn downscale_only_when_taller() {
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let tall = build_args(&cfg, &info(2160, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        let short = build_args(&cfg, &info(720, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(tall.contains("scale=-2:1080:flags=lanczos"));
        assert!(!short.contains("scale="));
    }

    #[test]
    fn scale_filter_threads_into_the_vf() {
        let cfg = Config {
            scale_filter: ScaleFilter::Area,
            ..Config::default()
        };
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(2160, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("scale=-2:1080:flags=area"));
    }

    #[test]
    fn ten_bit_selects_p010le() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, true), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-pix_fmt p010le"));
    }

    #[test]
    fn software_ten_bit_uses_planar_form() {
        let e = enc("libx265", EncoderFamily::Software);
        assert_eq!(
            pix_fmt(&Config::default(), &info(1080, true), &e),
            "yuv420p10le"
        );
    }

    #[test]
    fn twelve_bit_preserved_on_libx265_but_stepped_down_elsewhere() {
        let cfg = Config::default(); // Source depth
        let mut m = info(1080, false);
        m.pix_fmt = Some("yuv420p12le".into());
        // libx265 keeps 12-bit.
        assert_eq!(
            pix_fmt(&cfg, &m, &enc("libx265", EncoderFamily::Software)),
            "yuv420p12le"
        );
        // SVT-AV1 is 8/10-bit only → step down to 10-bit, never truncate to 8.
        assert_eq!(
            pix_fmt(&cfg, &m, &enc("libsvtav1", EncoderFamily::Software)),
            "yuv420p10le"
        );
        // Hardware caps at 10-bit → p010le.
        assert_eq!(
            pix_fmt(&cfg, &m, &enc("av1_nvenc", EncoderFamily::Nvenc)),
            "p010le"
        );
    }

    #[test]
    fn forced_ten_bit_upgrades_an_eight_bit_source() {
        let cfg = Config {
            bit_depth: BitDepth::Ten,
            ..Config::default()
        };
        // Software AV1/HEVC → planar 10-bit; hardware → p010le.
        assert_eq!(
            pix_fmt(
                &cfg,
                &info(1080, false),
                &enc("libsvtav1", EncoderFamily::Software)
            ),
            "yuv420p10le"
        );
        assert_eq!(
            pix_fmt(
                &cfg,
                &info(1080, false),
                &enc("hevc_nvenc", EncoderFamily::Nvenc)
            ),
            "p010le"
        );
        // End to end, the pix_fmt flag is emitted.
        let a = build_args(
            &cfg,
            &info(1080, false),
            &enc("av1_nvenc", EncoderFamily::Nvenc),
            &sw(),
            Path::new("o.mkv"),
        )
        .join(" ");
        assert!(a.contains("-pix_fmt p010le"));
    }

    #[test]
    fn forced_eight_bit_downgrades_a_ten_bit_source() {
        let cfg = Config {
            bit_depth: BitDepth::Eight,
            ..Config::default()
        };
        assert_eq!(
            pix_fmt(
                &cfg,
                &info(1080, true),
                &enc("libx265", EncoderFamily::Software)
            ),
            "yuv420p"
        );
    }

    #[test]
    fn ten_bit_steps_down_for_hardware_h264() {
        // NVENC H.264 is 8-bit only — forcing 10-bit must fall back, not fail.
        let cfg = Config {
            bit_depth: BitDepth::Ten,
            ..Config::default()
        };
        assert_eq!(
            pix_fmt(
                &cfg,
                &info(1080, false),
                &enc("h264_nvenc", EncoderFamily::Nvenc)
            ),
            "yuv420p"
        );
        // libx264 (software) can do 10-bit.
        assert_eq!(
            pix_fmt(
                &cfg,
                &info(1080, false),
                &enc("libx264", EncoderFamily::Software)
            ),
            "yuv420p10le"
        );
    }

    #[test]
    fn hdr_color_metadata_is_passed_through() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let mut m = info(2160, true);
        m.color_primaries = Some("bt2020".into());
        m.color_transfer = Some("smpte2084".into());
        m.color_space = Some("bt2020nc".into());
        m.color_range = Some("tv".into());
        let a = build_args(&cfg, &m, &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-color_primaries bt2020"));
        assert!(a.contains("-color_trc smpte2084"));
        assert!(a.contains("-colorspace bt2020nc"));
        assert!(a.contains("-color_range tv"));
    }

    #[test]
    fn unknown_color_values_are_dropped() {
        let cfg = Config::default();
        let e = enc("libx265", EncoderFamily::Software);
        let mut m = info(1080, false);
        m.color_primaries = Some("unknown".into());
        m.color_transfer = Some("bt709".into());
        let a = build_args(&cfg, &m, &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-color_primaries"));
        assert!(a.contains("-color_trc bt709"));
    }

    #[test]
    fn mp4_container_uses_movtext_faststart_and_drops_attachments() {
        let cfg = Config {
            container: Container::Mp4,
            ..Config::default()
        };
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mp4")).join(" ");
        assert!(a.contains("-c:s mov_text"));
        assert!(a.contains("-movflags +faststart"));
        assert!(!a.contains("0:t?")); // no attachment mapping in MP4
    }

    #[test]
    fn audio_transcode_emits_codec_and_bitrate() {
        let cfg = Config {
            audio_mode: AudioMode::Opus,
            audio_bitrate_kbps: 160,
            ..Config::default()
        };
        let e = enc("libx265", EncoderFamily::Software);
        let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-c:a libopus"));
        assert!(a.contains("-b:a 160k"));
    }

    #[test]
    fn mp4_opus_downgrades_to_aac() {
        let cfg = Config {
            container: Container::Mp4,
            audio_mode: AudioMode::Opus,
            ..Config::default()
        };
        let e = enc("h264_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mp4")).join(" ");
        assert!(a.contains("-c:a aac"));
        assert!(!a.contains("libopus"));
    }

    #[test]
    fn copy_audio_adds_no_audio_codec_flag() {
        let cfg = Config::default();
        let e = enc("libx265", EncoderFamily::Software);
        let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-c:a"));
    }

    #[test]
    fn keeps_streams_and_drops_data() {
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, &sw(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-map 0:V?"));
        assert!(a.contains("-map 0:a?"));
        assert!(a.contains("-map 0:s?"));
        assert!(a.contains("-map_chapters 0"));
    }

    // ---- GPU-resident (CUDA) pipeline ----------------------------------------

    #[test]
    fn cuda_pipeline_is_zero_copy_with_no_downscale() {
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        // h264 1080p source, target codec av1, no resize, no depth change.
        let a = build_args(&cfg, &info(1080, false), &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-hwaccel cuda -hwaccel_output_format cuda"));
        assert!(a.contains("-c:v av1_nvenc"));
        // Frames pass straight through: no filter, and no CPU-side pix_fmt convert.
        assert!(!a.contains("-vf"));
        assert!(!a.contains("-pix_fmt"));
        assert!(!a.contains("scale=")); // no software scaler
    }

    #[test]
    fn cuda_pipeline_scales_on_gpu_when_downscaling() {
        let cfg = Config::default(); // max_height 1080
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(2160, false), &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("-hwaccel_output_format cuda"));
        assert!(a.contains("scale_cuda=-2:1080"));
        assert!(!a.contains("scale=-2:1080:flags")); // not the software scaler
        assert!(!a.contains("-pix_fmt"));
    }

    #[test]
    fn cuda_falls_back_to_software_scaler_when_no_gpu_scaler() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        // cuda hwaccel present but NO gpu scaler → downscale must go software.
        let caps = HwCaps {
            cuda: true,
            scale_cuda: false,
            scale_npp: false,
        };
        let a = build_args(&cfg, &info(2160, false), &e, &caps, Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-hwaccel_output_format cuda"));
        assert!(a.contains("scale=-2:1080:flags="));
        assert!(a.contains("-pix_fmt"));
    }

    #[test]
    fn cuda_used_for_passthrough_even_without_gpu_scaler() {
        // No resize/depth change needs no scaler, so the GPU path still applies.
        let cfg = Config::default();
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let caps = HwCaps {
            cuda: true,
            scale_cuda: false,
            scale_npp: false,
        };
        let a = build_args(&cfg, &info(1080, false), &e, &caps, Path::new("o.mkv")).join(" ");
        assert!(a.contains("-hwaccel_output_format cuda"));
    }

    #[test]
    fn unsupported_source_codec_takes_software_decode() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let mut m = info(1080, false);
        m.codec = Some("prores".into()); // NVDEC can't decode it
        let a = build_args(&cfg, &m, &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-hwaccel"));
        assert!(a.contains("-pix_fmt"));
    }

    #[test]
    fn software_encoder_never_uses_cuda() {
        let cfg = Config::default();
        let e = enc("libx265", EncoderFamily::Software);
        let a = build_args(&cfg, &info(2160, false), &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-hwaccel"));
        assert!(a.contains("scale=-2:1080:flags="));
    }

    #[test]
    fn hardware_decode_off_forces_software() {
        let cfg = Config {
            hardware_decode: false,
            ..Config::default()
        };
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-hwaccel"));
    }

    #[test]
    fn cuda_forced_ten_bit_sets_scaler_format() {
        // 8-bit source, forced 10-bit, no downscale → GPU format-only conversion.
        let cfg = Config {
            bit_depth: BitDepth::Ten,
            ..Config::default()
        };
        let e = enc("av1_nvenc", EncoderFamily::Nvenc);
        let a = build_args(&cfg, &info(1080, false), &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(a.contains("scale_cuda=iw:ih:format=p010le"));
        assert!(!a.contains("-pix_fmt"));
    }

    #[test]
    fn twelve_bit_source_uses_software_even_on_cuda() {
        let cfg = Config::default();
        let e = enc("hevc_nvenc", EncoderFamily::Nvenc);
        let mut m = info(1080, false);
        m.pix_fmt = Some("yuv420p12le".into());
        let a = build_args(&cfg, &m, &e, &cuda(), Path::new("o.mkv")).join(" ");
        assert!(!a.contains("-hwaccel")); // NVDEC/NVENC top out at 10-bit
    }
}

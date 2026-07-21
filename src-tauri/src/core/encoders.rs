//! Encoder auto-detection and validation.
//!
//! Detects which FFmpeg encoders exist *and validates them* (a present encoder
//! can still fail if the driver or hardware isn't usable), then picks the best
//! one for the target codec — hardware first, CPU software as the
//! always-available fallback.

use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;

use super::config::Codec;
use super::util::command_no_window;

/// The rate-control "family" an encoder belongs to. Determines which quality/
/// preset flags [`super::encode`] emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EncoderFamily {
    Nvenc,
    Amf,
    Qsv,
    VideoToolbox,
    Software,
}

impl EncoderFamily {
    pub fn label(self) -> &'static str {
        match self {
            EncoderFamily::Nvenc => "NVIDIA (NVENC)",
            EncoderFamily::Amf => "AMD (AMF)",
            EncoderFamily::Qsv => "Intel (QSV)",
            EncoderFamily::VideoToolbox => "Apple (VideoToolbox)",
            EncoderFamily::Software => "CPU (software)",
        }
    }

    pub fn is_hardware(self) -> bool {
        !matches!(self, EncoderFamily::Software)
    }
}

/// A concrete FFmpeg encoder id + its family, e.g. `av1_nvenc` / Nvenc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Encoder {
    pub name: String,
    pub family: EncoderFamily,
}

impl Encoder {
    fn new(name: &str, family: EncoderFamily) -> Self {
        Self {
            name: name.to_string(),
            family,
        }
    }
}

/// Candidate encoders for each codec, ordered by preference (hardware first,
/// software last so there is always a working fallback).
pub fn candidates(codec: Codec) -> Vec<Encoder> {
    use EncoderFamily::*;
    match codec {
        Codec::Av1 => vec![
            Encoder::new("av1_nvenc", Nvenc),
            Encoder::new("av1_qsv", Qsv),
            Encoder::new("av1_amf", Amf),
            // No AV1 encode on VideoToolbox as of writing.
            Encoder::new("libsvtav1", Software),
        ],
        Codec::Hevc => vec![
            Encoder::new("hevc_nvenc", Nvenc),
            Encoder::new("hevc_qsv", Qsv),
            Encoder::new("hevc_amf", Amf),
            Encoder::new("hevc_videotoolbox", VideoToolbox),
            Encoder::new("libx265", Software),
        ],
        Codec::H264 => vec![
            Encoder::new("h264_nvenc", Nvenc),
            Encoder::new("h264_qsv", Qsv),
            Encoder::new("h264_amf", Amf),
            Encoder::new("h264_videotoolbox", VideoToolbox),
            Encoder::new("libx264", Software),
        ],
    }
}

/// The software (CPU) encoder for a codec — always available as a last-resort
/// fallback when a hardware encode fails on an edge-case source (e.g. a VR
/// resolution the GPU decode/scale path rejects).
pub fn software_encoder(codec: Codec) -> Option<Encoder> {
    candidates(codec)
        .into_iter()
        .find(|e| e.family == EncoderFamily::Software)
}

/// Parse `ffmpeg -encoders` into the set of encoder names the build exposes.
pub fn list_available(ffmpeg: &Path) -> HashSet<String> {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-encoders"]);
    let out = match cmd.stdout(Stdio::piped()).stderr(Stdio::null()).output() {
        Ok(o) => o,
        Err(_) => return HashSet::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut names = HashSet::new();
    // Lines look like: " V....D av1_nvenc  NVIDIA NVENC av1 encoder (codec av1)"
    for line in text.lines() {
        let trimmed = line.trim_start();
        // Skip the header until the "------" separator has passed.
        if let Some((flags, rest)) = trimmed.split_once(char::is_whitespace) {
            if flags.len() >= 6 && flags.starts_with('V') {
                if let Some(name) = rest.trim().split_whitespace().next() {
                    names.insert(name.to_string());
                }
            }
        }
    }
    names
}

/// Confirm an encoder actually works by encoding a short synthetic clip to null.
/// Presence in `-encoders` does not guarantee a usable driver/hardware.
///
/// `-pix_fmt yuv420p` is required: `testsrc` emits yuv444p, which several
/// hardware encoders (notably av1_nvenc) reject with a misleading "no capable
/// devices" error — the same 4:2:0 format the real encode uses avoids that.
pub fn validate(ffmpeg: &Path, encoder: &str, timeout: Duration) -> bool {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=256x256:rate=30:duration=1",
        "-pix_fmt",
        "yuv420p",
        "-c:v",
        encoder,
        "-f",
        "null",
        "-",
    ]);
    let mut child = match cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {}
            Err(_) => return false,
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Per-codec detection summary surfaced to the UI's encoder panel.
#[derive(Debug, Clone, Serialize)]
pub struct CodecSupport {
    pub codec: Codec,
    /// Encoders that are present AND validated, best-first.
    pub usable: Vec<Encoder>,
    /// The one that would be auto-selected (first usable), if any.
    pub selected: Option<Encoder>,
}

/// Full detection report for all target codecs.
#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub codecs: Vec<CodecSupport>,
    /// True if any hardware encoder validated on this machine.
    pub has_hardware: bool,
}

/// Detect + validate everything. `validate_timeout` bounds each probe encode.
pub fn detect(ffmpeg: &Path, validate_timeout: Duration) -> Detection {
    let available = list_available(ffmpeg);
    let mut codecs = Vec::new();
    let mut has_hardware = false;

    for codec in [Codec::Av1, Codec::Hevc, Codec::H264] {
        let mut usable = Vec::new();
        for cand in candidates(codec) {
            if !available.contains(&cand.name) {
                continue;
            }
            // Software encoders are trusted without a probe (always usable);
            // hardware encoders must pass the one-frame validation.
            let ok = match cand.family {
                EncoderFamily::Software => true,
                _ => validate(ffmpeg, &cand.name, validate_timeout),
            };
            if ok {
                if cand.family.is_hardware() {
                    has_hardware = true;
                }
                usable.push(cand);
            }
        }
        let selected = usable.first().cloned();
        codecs.push(CodecSupport {
            codec,
            usable,
            selected,
        });
    }

    Detection {
        codecs,
        has_hardware,
    }
}

/// Resolve the encoder to use for a run: honor an explicit override if it is
/// usable, else auto-select the best validated candidate for the codec.
pub fn select(
    ffmpeg: &Path,
    codec: Codec,
    override_name: Option<&str>,
    validate_timeout: Duration,
) -> Option<Encoder> {
    let available = list_available(ffmpeg);
    let cands = candidates(codec);

    if let Some(name) = override_name {
        if let Some(c) = cands.iter().find(|c| c.name == name) {
            let ok = matches!(c.family, EncoderFamily::Software)
                || validate(ffmpeg, &c.name, validate_timeout);
            if available.contains(&c.name) && ok {
                return Some(c.clone());
            }
        }
    }

    for c in cands {
        if !available.contains(&c.name) {
            continue;
        }
        let ok = matches!(c.family, EncoderFamily::Software)
            || validate(ffmpeg, &c.name, validate_timeout);
        if ok {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_candidates_prefer_hardware_then_software() {
        let c = candidates(Codec::Av1);
        assert_eq!(c.first().unwrap().family, EncoderFamily::Nvenc);
        assert_eq!(c.last().unwrap().family, EncoderFamily::Software);
        assert_eq!(c.last().unwrap().name, "libsvtav1");
    }

    #[test]
    fn every_codec_has_a_software_fallback() {
        for codec in [Codec::Av1, Codec::Hevc, Codec::H264] {
            assert!(candidates(codec)
                .iter()
                .any(|e| e.family == EncoderFamily::Software));
        }
    }

    #[test]
    fn software_encoder_resolves_per_codec() {
        assert_eq!(software_encoder(Codec::Av1).unwrap().name, "libsvtav1");
        assert_eq!(software_encoder(Codec::Hevc).unwrap().name, "libx265");
        assert_eq!(software_encoder(Codec::H264).unwrap().name, "libx264");
        for codec in [Codec::Av1, Codec::Hevc, Codec::H264] {
            assert_eq!(
                software_encoder(codec).unwrap().family,
                EncoderFamily::Software
            );
        }
    }
}

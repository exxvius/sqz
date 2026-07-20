//! FFmpeg hardware-pipeline capability probe.
//!
//! Whether a GPU-resident encode is possible depends on what the *specific*
//! FFmpeg build exposes: the `cuda` hwaccel (NVDEC decode into CUDA frames) and a
//! GPU scaler (`scale_cuda` or `scale_npp`). These vary by build, so we detect
//! them once and let [`super::encode::build_args`] choose the fastest valid path.
//!
//! Detection is cheap (two `ffmpeg -…` list calls) and cached on [`FfBin`] so it
//! runs once per resolved toolchain, not per file.

use std::path::Path;
use std::process::Stdio;

use serde::Serialize;

use super::util::command_no_window;

/// What the resolved FFmpeg build can do on the GPU. `Copy` + tiny so it rides
/// along on every `FfBin` clone for free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HwCaps {
    /// `-hwaccel cuda` is available — NVDEC decode straight into CUDA frames.
    pub cuda: bool,
    /// The `scale_cuda` GPU resize filter is available.
    pub scale_cuda: bool,
    /// The `scale_npp` GPU resize filter is available.
    pub scale_npp: bool,
}

impl HwCaps {
    /// A GPU scaler exists (so a downscale can stay on the GPU).
    pub fn gpu_scale(&self) -> bool {
        self.scale_cuda || self.scale_npp
    }

    /// The GPU resize filter name to use (prefer `scale_cuda`; NPP as fallback).
    pub fn scaler(&self) -> Option<&'static str> {
        if self.scale_cuda {
            Some("scale_cuda")
        } else if self.scale_npp {
            Some("scale_npp")
        } else {
            None
        }
    }
}

/// Run a one-shot `ffmpeg <list_flag>` and return its combined stdout, or "".
fn list(ffmpeg: &Path, flag: &str) -> String {
    let mut cmd = command_no_window(ffmpeg);
    cmd.args(["-hide_banner", flag]);
    match cmd.stdout(Stdio::piped()).stderr(Stdio::null()).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

/// True if `name` appears as a whitespace-delimited token in a `-filters` /
/// `-hwaccels` listing (avoids matching it as a substring of another name).
fn has_token(listing: &str, name: &str) -> bool {
    listing.split_whitespace().any(|t| t == name)
}

/// Probe the FFmpeg build's GPU capabilities.
pub fn detect(ffmpeg: &Path) -> HwCaps {
    let accels = list(ffmpeg, "-hwaccels");
    let filters = list(ffmpeg, "-filters");
    HwCaps {
        cuda: has_token(&accels, "cuda"),
        scale_cuda: has_token(&filters, "scale_cuda"),
        scale_npp: has_token(&filters, "scale_npp"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_match_is_exact_not_substring() {
        let filters = " ... scale_cuda      V->V  GPU resizer\n ... scale        V->V  resizer";
        assert!(has_token(filters, "scale_cuda"));
        assert!(has_token(filters, "scale"));
        assert!(!has_token(filters, "scale_npp"));
    }

    #[test]
    fn scaler_prefers_cuda_then_npp() {
        let both = HwCaps { cuda: true, scale_cuda: true, scale_npp: true };
        assert_eq!(both.scaler(), Some("scale_cuda"));
        let npp = HwCaps { cuda: true, scale_cuda: false, scale_npp: true };
        assert_eq!(npp.scaler(), Some("scale_npp"));
        let none = HwCaps { cuda: true, scale_cuda: false, scale_npp: false };
        assert_eq!(none.scaler(), None);
        assert!(!none.gpu_scale());
    }
}

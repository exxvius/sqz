//! Resolve the FFmpeg / FFprobe binaries the app uses.
//!
//! The app ships tiny and does not bundle FFmpeg. Binaries are located, in order:
//!   1. an explicit user-chosen path (bring-your-own)
//!   2. `SQZ_FFMPEG` / `SQZ_FFPROBE` env overrides
//!   3. the app-managed dir the downloader writes to (`<data>/bin`)
//!   4. beside the running executable
//!   5. the bare name on `PATH`

use std::path::{Path, PathBuf};

const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

#[derive(Debug, Clone)]
pub struct FfBin {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

/// Directory the downloader writes ffmpeg/ffprobe into.
pub fn managed_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("bin")
}

fn beside_exe(base: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let cand = exe.parent()?.join(format!("{base}{EXE_SUFFIX}"));
    cand.exists().then_some(cand)
}

fn env_override(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.exists())
}

fn resolve_one(base: &str, env_var: &str, data_dir: &Path, custom: Option<&Path>) -> PathBuf {
    if let Some(p) = custom {
        if p.exists() {
            return p.to_path_buf();
        }
    }
    env_override(env_var)
        .or_else(|| {
            let m = managed_dir(data_dir).join(format!("{base}{EXE_SUFFIX}"));
            m.exists().then_some(m)
        })
        .or_else(|| beside_exe(base))
        .unwrap_or_else(|| PathBuf::from(format!("{base}{EXE_SUFFIX}"))) // PATH fallback
}

impl FfBin {
    pub fn resolve(
        data_dir: &Path,
        custom_ffmpeg: Option<&Path>,
        custom_ffprobe: Option<&Path>,
    ) -> Self {
        Self {
            ffmpeg: resolve_one("ffmpeg", "SQZ_FFMPEG", data_dir, custom_ffmpeg),
            ffprobe: resolve_one("ffprobe", "SQZ_FFPROBE", data_dir, custom_ffprobe),
        }
    }

    /// True if both binaries resolve to something that exists on disk.
    pub fn is_present(&self) -> bool {
        self.ffmpeg.exists() && self.ffprobe.exists()
    }
}

//! Resolve the bundled FFmpeg / FFprobe sidecar binaries.
//!
//! The whole point of sqz's portability is that these are shipped inside the app
//! — no PATH dependency, no user install. Tauri copies `externalBin` next to the
//! main executable (base name, platform exe suffix). Resolution order:
//!   1. `SQZ_FFMPEG` / `SQZ_FFPROBE` env overrides (dev / power users)
//!   2. beside the running executable (the bundled sidecar — the release path)
//!   3. the dev `src-tauri/binaries/<name>-<target-triple>` location
//!   4. bare name on PATH (last-resort dev fallback)

use std::path::{Path, PathBuf};

const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

#[derive(Debug, Clone)]
pub struct FfBin {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

fn beside_exe(base: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let cand = dir.join(format!("{base}{EXE_SUFFIX}"));
    cand.exists().then_some(cand)
}

fn dev_binaries(base: &str) -> Option<PathBuf> {
    // Matches the naming documented in src-tauri/binaries/README.md.
    let triple = current_target_triple();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let cand = root.join(format!("{base}-{triple}{EXE_SUFFIX}"));
    cand.exists().then_some(cand)
}

fn env_override(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.exists())
}

fn resolve_one(base: &str, env_var: &str) -> PathBuf {
    env_override(env_var)
        .or_else(|| beside_exe(base))
        .or_else(|| dev_binaries(base))
        .unwrap_or_else(|| PathBuf::from(format!("{base}{EXE_SUFFIX}"))) // PATH fallback
}

impl FfBin {
    pub fn resolve() -> Self {
        Self {
            ffmpeg: resolve_one("ffmpeg", "SQZ_FFMPEG"),
            ffprobe: resolve_one("ffprobe", "SQZ_FFPROBE"),
        }
    }

    /// True if both binaries resolve to something that exists on disk.
    pub fn is_present(&self) -> bool {
        self.ffmpeg.exists() && self.ffprobe.exists()
    }
}

/// The Rust target triple this build was compiled for (baked in at build time by
/// `build.rs` via `TARGET`, else inferred from `cfg!`).
fn current_target_triple() -> String {
    option_env!("TARGET").map(str::to_string).unwrap_or_else(|| {
        // Best-effort fallback for the common desktop targets.
        let arch = std::env::consts::ARCH;
        if cfg!(target_os = "windows") {
            format!("{arch}-pc-windows-msvc")
        } else if cfg!(target_os = "macos") {
            format!("{arch}-apple-darwin")
        } else {
            format!("{arch}-unknown-linux-gnu")
        }
    })
}

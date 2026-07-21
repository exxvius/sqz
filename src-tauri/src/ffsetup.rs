//! On-demand FFmpeg: locate a user-chosen binary, or download a build into the
//! app's data dir. Keeps the shipped app tiny.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::core::ffbin::{managed_dir, FfBin};

pub const EV_FFMPEG_PROGRESS: &str = "sqz-ffmpeg-progress";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FfConfig {
    pub ffmpeg: Option<String>,
    pub ffprobe: Option<String>,
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ffmpeg.json")
}

pub fn load_config(data_dir: &Path) -> FfConfig {
    fs::read_to_string(config_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(data_dir: &Path, cfg: &FfConfig) -> Result<(), String> {
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(config_path(data_dir), text).map_err(|e| e.to_string())
}

/// Resolve the current FfBin, honoring any saved custom paths.
pub fn current(data_dir: &Path) -> FfBin {
    let cfg = load_config(data_dir);
    let mut ff = FfBin::resolve(
        data_dir,
        cfg.ffmpeg.as_deref().map(Path::new),
        cfg.ffprobe.as_deref().map(Path::new),
    );
    // Probe GPU capabilities once here so the per-file encode path can pick the
    // fastest valid pipeline without re-querying ffmpeg.
    ff.detect_caps();
    ff
}

/// Confirm a binary runs (so we don't save a broken path).
fn runs(path: &Path) -> bool {
    crate::core::util::command_no_window(path)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Save user-chosen ffmpeg/ffprobe paths after validating they run.
pub fn set_custom(data_dir: &Path, ffmpeg: &str, ffprobe: &str) -> Result<(), String> {
    if !runs(Path::new(ffmpeg)) {
        return Err("That ffmpeg binary didn't run (`ffmpeg -version` failed).".into());
    }
    if !runs(Path::new(ffprobe)) {
        return Err("That ffprobe binary didn't run (`ffprobe -version` failed).".into());
    }
    save_config(
        data_dir,
        &FfConfig {
            ffmpeg: Some(ffmpeg.to_string()),
            ffprobe: Some(ffprobe.to_string()),
        },
    )
}

/// Forget any custom paths (revert to managed / PATH resolution).
pub fn clear_custom(data_dir: &Path) -> Result<(), String> {
    save_config(data_dir, &FfConfig::default())
}

#[derive(Serialize, Clone)]
struct Progress<'a> {
    stage: &'a str,
    downloaded: u64,
    total: u64,
}

fn emit(app: &AppHandle, stage: &str, downloaded: u64, total: u64) {
    let _ = app.emit(
        EV_FFMPEG_PROGRESS,
        Progress {
            stage,
            downloaded,
            total,
        },
    );
}

/// Download and install FFmpeg + FFprobe into `<data>/bin`, emitting progress.
pub fn download(app: &AppHandle, data_dir: &Path) -> Result<(), String> {
    let bin = managed_dir(data_dir);
    fs::create_dir_all(&bin).map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        let archive = data_dir.join("ffmpeg-dl.zip");
        download_file(app, url, &archive)?;
        extract_zip(app, &archive, &bin, &["ffmpeg.exe", "ffprobe.exe"])?;
        let _ = fs::remove_file(&archive);
    }

    #[cfg(target_os = "macos")]
    {
        for (which, name) in [("ffmpeg", "ffmpeg"), ("ffprobe", "ffprobe")] {
            let url = format!("https://evermeet.cx/ffmpeg/getrelease/{which}/zip");
            let archive = data_dir.join(format!("{which}-dl.zip"));
            download_file(app, &url, &archive)?;
            extract_zip(app, &archive, &bin, &[name])?;
            let _ = fs::remove_file(&archive);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz";
        let archive = data_dir.join("ffmpeg-dl.tar.xz");
        download_file(app, url, &archive)?;
        extract_tar_xz(app, &archive, &bin, &["ffmpeg", "ffprobe"])?;
        let _ = fs::remove_file(&archive);
    }

    // Verify what we installed actually runs.
    let ff = current(data_dir);
    if !runs(&ff.ffmpeg) || !runs(&ff.ffprobe) {
        return Err(
            "Downloaded FFmpeg, but it did not run. Try again or use your own binary.".into(),
        );
    }
    emit(app, "done", 1, 1);
    Ok(())
}

fn download_file(app: &AppHandle, url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 128 * 1024];
    let mut downloaded = 0u64;
    let mut last = Instant::now();

    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if last.elapsed() > Duration::from_millis(120) {
            emit(app, "download", downloaded, total);
            last = Instant::now();
        }
    }
    emit(app, "extract", total, total);
    Ok(())
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn extract_zip(
    _app: &AppHandle,
    archive: &Path,
    bin: &Path,
    wanted: &[&str],
) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if !entry.is_file() {
            continue;
        }
        let base = Path::new(entry.name())
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string);
        if let Some(base) = base {
            if wanted.contains(&base.as_str()) {
                let out = bin.join(&base);
                let mut outfile = File::create(&out).map_err(|e| e.to_string())?;
                io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
                make_executable(&out);
            }
        }
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn extract_tar_xz(
    _app: &AppHandle,
    archive: &Path,
    bin: &Path,
    wanted: &[&str],
) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| e.to_string())?;
    let xz = xz2::read::XzDecoder::new(file);
    let mut ar = tar::Archive::new(xz);
    for entry in ar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let base = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string));
        if let Some(base) = base {
            if wanted.contains(&base.as_str()) {
                let out = bin.join(&base);
                let mut outfile = File::create(&out).map_err(|e| e.to_string())?;
                io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
                make_executable(&out);
            }
        }
    }
    Ok(())
}

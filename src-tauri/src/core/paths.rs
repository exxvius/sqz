//! Volume-aware scratch/holding path helpers. The scratch dir always lives on
//! the source's volume so the final swap is an atomic same-filesystem rename,
//! using a cross-platform "same volume" check (drive letter on Windows, device
//! id on Unix).

use std::path::{Path, PathBuf};

use super::config::{Config, HOLDING_DIRNAME, TEMP_DIRNAME};

/// Walk up from `path` to the first ancestor that exists on disk.
fn nearest_existing(path: &Path) -> PathBuf {
    let mut p = path;
    loop {
        if p.exists() {
            return p.to_path_buf();
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return p.to_path_buf(),
        }
    }
}

#[cfg(unix)]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(nearest_existing(path)).ok().map(|m| m.dev())
}

#[cfg(windows)]
fn win_volume(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let abs = std::path::absolute(path).ok()?;
    for comp in abs.components() {
        if let Component::Prefix(pre) = comp {
            return match pre.kind() {
                Prefix::Disk(b) | Prefix::VerbatimDisk(b) => {
                    Some((b as char).to_ascii_lowercase().to_string())
                }
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => Some(format!(
                    "\\\\{}\\{}",
                    server.to_string_lossy().to_lowercase(),
                    share.to_string_lossy().to_lowercase()
                )),
                _ => None,
            };
        }
    }
    None
}

/// Volume root for a path used as the base for the shared scratch dir.
#[cfg(windows)]
fn volume_root(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    if let Ok(abs) = std::path::absolute(path) {
        for comp in abs.components() {
            if let Component::Prefix(pre) = comp {
                if let Prefix::Disk(b) | Prefix::VerbatimDisk(b) = pre.kind() {
                    return PathBuf::from(format!("{}:\\", (b as char).to_ascii_uppercase()));
                }
            }
        }
    }
    PathBuf::from("C:\\")
}

/// Topmost ancestor sharing the source's filesystem (its mount point).
#[cfg(unix)]
fn volume_root(path: &Path) -> PathBuf {
    let start = nearest_existing(path);
    let dev = match device_of(&start) {
        Some(d) => d,
        None => return start,
    };
    let mut root = start.clone();
    let mut cur = start;
    while let Some(parent) = cur.parent().map(Path::to_path_buf) {
        match device_of(&parent) {
            Some(d) if d == dev => {
                root = parent.clone();
                cur = parent;
            }
            _ => break,
        }
    }
    root
}

/// True if two paths live on the same volume (so a rename between them is atomic).
pub fn same_volume(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        win_volume(a) == win_volume(b) && win_volume(a).is_some()
    }
    #[cfg(unix)]
    {
        match (device_of(a), device_of(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        a.parent() == b.parent()
    }
}

/// Scratch dir on the SAME volume as `source` so the final swap is atomic.
/// Honors `cfg.temp_dir` only when it lives on the source's volume.
pub fn temp_dir_for(source: &Path, cfg: &Config) -> std::io::Result<PathBuf> {
    let temp = match &cfg.temp_dir {
        Some(t) if same_volume(t, source) => t.clone(),
        _ => volume_root(source).join(TEMP_DIRNAME),
    };
    std::fs::create_dir_all(&temp)?;
    Ok(temp)
}

/// Every temp dir that could be used, for startup orphan cleanup.
pub fn all_temp_dirs(cfg: &Config, sources: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for src in sources {
        if let Ok(d) = temp_dir_for(src, cfg) {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    }
    dirs
}

/// Mirror the source under `holding_dir`, preserving volume tag + structure.
pub fn holding_path_for(source: &Path, cfg: &Config) -> PathBuf {
    let holding = cfg
        .holding_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(HOLDING_DIRNAME));
    let abs = std::path::absolute(source).unwrap_or_else(|_| source.to_path_buf());

    #[cfg(windows)]
    {
        let tag = win_volume(&abs).unwrap_or_else(|| "root".into());
        let rel: PathBuf = abs
            .components()
            .filter(|c| !matches!(c, std::path::Component::Prefix(_) | std::path::Component::RootDir))
            .collect();
        return holding.join(tag).join(rel);
    }
    #[cfg(not(windows))]
    {
        let rel = abs.strip_prefix("/").unwrap_or(&abs);
        holding.join(rel)
    }
}

/// True if `path` lives inside a temp/holding dir we manage (skip on scan).
pub fn is_under_managed_dir(path: &Path, cfg: &Config) -> bool {
    for comp in path.components() {
        if let std::path::Component::Normal(name) = comp {
            let n = name.to_string_lossy().to_lowercase();
            if n == TEMP_DIRNAME.to_lowercase() || n == HOLDING_DIRNAME.to_lowercase() {
                return true;
            }
        }
    }
    if let Some(holding) = &cfg.holding_dir {
        if let (Ok(p), Ok(h)) = (std::path::absolute(path), std::path::absolute(holding)) {
            if p.starts_with(&h) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_managed_dirs() {
        let cfg = Config::default();
        assert!(is_under_managed_dir(Path::new("/media/.sqz_tmp/x.mkv"), &cfg));
        assert!(is_under_managed_dir(Path::new("/media/.sqz_originals/x.mkv"), &cfg));
        assert!(!is_under_managed_dir(Path::new("/media/movies/x.mkv"), &cfg));
    }

    #[test]
    fn a_path_is_on_its_own_volume() {
        let here = std::env::current_dir().unwrap();
        assert!(same_volume(&here, &here));
    }
}

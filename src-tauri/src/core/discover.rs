//! Expand inputs (files + recursed dirs) into a de-duplicated video list.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::config::Config;
use super::paths::is_under_managed_dir;

fn is_video(path: &Path, cfg: &Config) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => Config::is_video_ext(ext) && !is_under_managed_dir(path, cfg),
        None => false,
    }
}

/// Canonical key for de-duplication (absolute, lowercased on case-insensitive FS).
fn key(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Return the sorted, de-duplicated list of video files under `cfg.inputs`.
pub fn discover(cfg: &Config) -> Vec<PathBuf> {
    // BTreeMap keyed by the canonical path gives dedupe + a stable sort.
    let mut found: BTreeMap<String, PathBuf> = BTreeMap::new();

    for raw in &cfg.inputs {
        let base = raw.clone();
        if base.is_file() {
            if is_video(&base, cfg) {
                let resolved = std::path::absolute(&base).unwrap_or(base);
                found.insert(key(&resolved), resolved);
            }
        } else if base.is_dir() {
            for entry in WalkDir::new(&base)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    // Prune managed scratch/holding dirs early.
                    !(e.file_type().is_dir() && is_under_managed_dir(e.path(), cfg))
                })
                .filter_map(Result::ok)
            {
                let p = entry.path();
                if entry.file_type().is_file() && is_video(p, cfg) {
                    let resolved = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
                    found.insert(key(&resolved), resolved);
                }
            }
        }
        // Non-existent inputs are silently skipped (the UI validates first).
    }

    found.into_values().collect()
}

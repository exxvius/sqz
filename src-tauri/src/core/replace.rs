//! Atomically swap a verified output in for the original, disposing the original
//! per policy (recycle / holding / delete). Never lose data.
//!
//! Every path keeps a recovery route:
//!   - recycle: the original goes to the OS Trash (name preserved), restorable;
//!     the encoded file also still exists until the final rename.
//!   - holding: the original is moved to a mirrored holding folder and moved back
//!     if the final rename fails.
//!   - delete: the original is stashed under a same-folder temp name and restored
//!     if the final rename fails; the stash is removed only after success.

use std::path::{Path, PathBuf};

use filetime::{set_file_times, FileTime};
use thiserror::Error;

use super::config::{Config, OnSuccess};
use super::paths::holding_path_for;

#[derive(Debug, Error)]
pub enum ReplaceError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn err(msg: impl Into<String>) -> ReplaceError {
    ReplaceError::Msg(msg.into())
}

/// Same-volume atomic rename.
fn place(encoded: &Path, final_path: &Path) -> std::io::Result<()> {
    std::fs::rename(encoded, final_path)
}

/// Move that tolerates a cross-device target (rename → copy+remove fallback).
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)
        }
    }
}

fn restore_times(final_path: &Path, atime: FileTime, mtime: FileTime) {
    // Best-effort; a metadata-timestamp failure must not fail the swap.
    let _ = set_file_times(final_path, atime, mtime);
}

/// Swap `encoded` in for `src`. Returns the final path (always `.mkv`).
pub fn replace_original(cfg: &Config, src: &Path, encoded: &Path) -> Result<PathBuf, ReplaceError> {
    let final_path = src.with_extension("mkv");
    let meta = std::fs::metadata(src)?;
    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);

    match cfg.on_success {
        OnSuccess::Recycle => {
            // Recoverable from the Trash, so we can dispose by real name first.
            trash::delete(src).map_err(|e| err(format!("could not send to trash: {e}")))?;
            place(encoded, &final_path).map_err(|e| {
                err(format!(
                    "encoded file not placed ({e}); original is in the Recycle Bin/Trash"
                ))
            })?;
        }
        OnSuccess::Holding => {
            let dest = holding_path_for(src, cfg);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            move_path(src, &dest)?;
            if let Err(e) = place(encoded, &final_path) {
                let _ = move_path(&dest, src); // restore
                return Err(err(format!("failed to place encoded file: {e}")));
            }
        }
        OnSuccess::Delete => {
            let stash = src.with_file_name(format!(
                "{}.sqz_old",
                src.file_name().unwrap_or_default().to_string_lossy()
            ));
            if stash.exists() {
                std::fs::remove_file(&stash)?;
            }
            std::fs::rename(src, &stash)?; // original aside (atomic, recoverable)
            if let Err(e) = place(encoded, &final_path) {
                let _ = std::fs::rename(&stash, src); // restore
                return Err(err(format!("failed to place encoded file: {e}")));
            }
            let _ = std::fs::remove_file(&stash); // committed: drop the original
        }
    }

    restore_times(&final_path, atime, mtime);
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("sqz_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_file(p: &Path, bytes: &[u8]) {
        let mut f = std::fs::File::create(p).unwrap();
        f.write_all(bytes).unwrap();
    }

    #[test]
    fn delete_mode_swaps_and_removes_original() {
        let d = tmpdir();
        let src = d.join("clip.mp4");
        let enc = d.join("enc.mkv");
        write_file(&src, b"original-original-original");
        write_file(&enc, b"tiny");

        let cfg = Config {
            on_success: OnSuccess::Delete,
            ..Config::default()
        };
        let final_path = replace_original(&cfg, &src, &enc).unwrap();
        assert_eq!(final_path, d.join("clip.mkv"));
        assert!(final_path.exists());
        assert!(!src.exists());
        assert!(!enc.exists());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"tiny");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn holding_mode_preserves_original_in_holding() {
        let d = tmpdir();
        let holding = d.join("hold");
        let src = d.join("movies").join("clip.mkv");
        std::fs::create_dir_all(src.parent().unwrap()).unwrap();
        let enc = d.join("enc.mkv");
        write_file(&src, b"original-bytes-here");
        write_file(&enc, b"x");

        let cfg = Config {
            on_success: OnSuccess::Holding,
            holding_dir: Some(holding.clone()),
            ..Config::default()
        };
        let final_path = replace_original(&cfg, &src, &enc).unwrap();
        assert!(final_path.exists());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"x");
        // Original still recoverable somewhere under the holding dir.
        let found = walkdir::WalkDir::new(&holding)
            .into_iter()
            .filter_map(Result::ok)
            .any(|e| e.file_type().is_file());
        assert!(found);

        let _ = std::fs::remove_dir_all(&d);
    }
}

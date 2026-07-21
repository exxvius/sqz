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

/// Swap `encoded` in for `src`. Returns the final path (extension = the run's
/// output container).
pub fn replace_original(cfg: &Config, src: &Path, encoded: &Path) -> Result<PathBuf, ReplaceError> {
    let final_path = src.with_extension(cfg.container.ext());
    let meta = std::fs::metadata(src)?;
    let atime = FileTime::from_last_access_time(&meta);
    let mtime = FileTime::from_last_modification_time(&meta);

    // Guard against clobbering an unrelated file: when the source isn't already
    // `.mkv`, the final path differs from the source, and an existing file there
    // is someone else's data. Refuse rather than overwrite it (invariant A). This
    // check runs before any destructive step, so the original is untouched.
    if final_path != *src && final_path.exists() {
        return Err(err(format!(
            "target already exists, refusing to overwrite: {}",
            final_path.display()
        )));
    }

    match cfg.on_success {
        OnSuccess::Recycle => {
            if final_path == *src {
                // Same name (an already-`.mkv` source): the original occupies the
                // target path, so it must be recycled before the encoded file can
                // take its place. Recoverable from the Trash in the interim.
                trash::delete(src).map_err(|e| err(format!("could not send to trash: {e}")))?;
                place(encoded, &final_path).map_err(|e| {
                    err(format!(
                        "encoded file not placed ({e}); original is in the Recycle Bin/Trash"
                    ))
                })?;
            } else {
                // Different name: place the new file first (the original is never
                // at risk), then recycle the original. If recycling fails, roll the
                // placement back so we end up exactly where we started.
                place(encoded, &final_path)?;
                if let Err(e) = trash::delete(src) {
                    let _ = std::fs::remove_file(&final_path);
                    return Err(err(format!("could not send original to trash: {e}")));
                }
            }
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
                "{}{STASH_SUFFIX}",
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

/// Suffix Delete-mode uses to stash an original during the swap window.
const STASH_SUFFIX: &str = ".sqz_old";

/// Finish the interrupted transaction a single `.sqz_old` stash represents. If
/// the committed `.mkv` output exists the swap completed and the stash is stale
/// (drop it); otherwise restore the original from the stash. Either way the
/// original is never lost.
fn finish_stash(stash: &Path) {
    let Some(name) = stash.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if !name.ends_with(STASH_SUFFIX) {
        return;
    }
    let orig = stash.with_file_name(&name[..name.len() - STASH_SUFFIX.len()]);
    // The committed output carries one of the known container extensions; if any
    // sibling exists the swap completed and the stash is stale.
    let committed = ["mkv", "mp4"]
        .iter()
        .any(|ext| orig.with_extension(ext).exists());
    if committed {
        let _ = std::fs::remove_file(stash); // swap committed; stash is stale
    } else {
        let _ = std::fs::rename(stash, &orig); // swap interrupted; restore original
    }
}

/// Delete held originals older than the configured retention window. A no-op
/// unless `on_success == Holding`, a `holding_dir` is set, and retention > 0.
/// Only ever removes files *inside* the holding folder — never a live original.
pub fn purge_expired_holding(cfg: &Config) {
    if !matches!(cfg.on_success, OnSuccess::Holding) || cfg.holding_retention_days == 0 {
        return;
    }
    let Some(dir) = &cfg.holding_dir else { return };
    let max_age = std::time::Duration::from_secs(cfg.holding_retention_days as u64 * 86_400);
    let now = std::time::SystemTime::now();
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let expired = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mtime| now.duration_since(mtime).ok())
            .map(|age| age > max_age)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Recover any `.sqz_old` stashes a crash left behind under `roots`.
///
/// A stash means a Delete-mode swap was interrupted between moving the original
/// aside and placing the encoded file. Directory inputs are walked recursively;
/// a file input is checked at its exact stash path (after a crash the file itself
/// no longer exists, so it can't be walked). This just finishes the interrupted
/// transaction — the original is never lost.
pub fn recover_stashes(roots: &[PathBuf]) {
    for root in roots {
        if root.is_dir() {
            for entry in walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_file() {
                    let p = entry.path();
                    if p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(STASH_SUFFIX))
                    {
                        finish_stash(p);
                    }
                }
            }
        } else if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
            let stash = root.with_file_name(format!("{name}{STASH_SUFFIX}"));
            if stash.exists() {
                finish_stash(&stash);
            }
        }
    }
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

    // Invariant A: a failed placement must leave the original completely intact.
    // A non-existent `encoded` makes the final rename fail deterministically.
    #[test]
    fn delete_mode_restores_original_when_placement_fails() {
        let d = tmpdir();
        let src = d.join("clip.mp4");
        let missing = d.join("does_not_exist.mkv");
        write_file(&src, b"the-precious-original");

        let cfg = Config {
            on_success: OnSuccess::Delete,
            ..Config::default()
        };
        let res = replace_original(&cfg, &src, &missing);
        assert!(res.is_err());
        assert!(src.exists(), "original must survive a failed swap");
        assert_eq!(std::fs::read(&src).unwrap(), b"the-precious-original");
        // No stash left dangling.
        assert!(!d.join("clip.mp4.sqz_old").exists());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn holding_mode_restores_original_when_placement_fails() {
        let d = tmpdir();
        let holding = d.join("hold");
        let src = d.join("clip.mkv");
        let missing = d.join("nope.mkv");
        write_file(&src, b"holding-original");

        let cfg = Config {
            on_success: OnSuccess::Holding,
            holding_dir: Some(holding),
            ..Config::default()
        };
        let res = replace_original(&cfg, &src, &missing);
        assert!(res.is_err());
        assert!(
            src.exists(),
            "original must be moved back from holding on failure"
        );
        assert_eq!(std::fs::read(&src).unwrap(), b"holding-original");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refuses_to_overwrite_an_unrelated_target() {
        let d = tmpdir();
        let src = d.join("clip.mp4");
        let bystander = d.join("clip.mkv"); // unrelated existing file
        let enc = d.join("enc.mkv");
        write_file(&src, b"source");
        write_file(&bystander, b"do-not-touch-me");
        write_file(&enc, b"encoded");

        let cfg = Config {
            on_success: OnSuccess::Delete,
            ..Config::default()
        };
        let res = replace_original(&cfg, &src, &enc);
        assert!(
            res.is_err(),
            "must refuse when the .mkv target already exists"
        );
        assert!(src.exists());
        assert_eq!(std::fs::read(&bystander).unwrap(), b"do-not-touch-me");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recover_stashes_restores_when_swap_did_not_commit() {
        let d = tmpdir();
        let stash = d.join("movie.mp4.sqz_old");
        write_file(&stash, b"interrupted-original");
        // No movie.mkv present ⇒ the swap never committed ⇒ restore.
        recover_stashes(&[d.clone()]);
        assert!(d.join("movie.mp4").exists());
        assert_eq!(
            std::fs::read(d.join("movie.mp4")).unwrap(),
            b"interrupted-original"
        );
        assert!(!stash.exists());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recover_stashes_drops_stale_stash_when_swap_committed() {
        let d = tmpdir();
        let stash = d.join("movie.mp4.sqz_old");
        write_file(&stash, b"stale");
        write_file(&d.join("movie.mkv"), b"committed-output"); // swap completed
        recover_stashes(&[d.clone()]);
        assert!(!stash.exists(), "stale stash should be dropped");
        assert!(
            !d.join("movie.mp4").exists(),
            "must not resurrect the original"
        );
        assert_eq!(
            std::fs::read(d.join("movie.mkv")).unwrap(),
            b"committed-output"
        );

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

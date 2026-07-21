//! Filesystem-event watching for the `OnChange` unattended trigger.
//!
//! A single recursive [`notify`] watcher spans every root of every enabled
//! `OnChange` library. When a file is created/modified/removed under a root, the
//! owning library's id is stamped "dirty" with the current time; the supervisor
//! reads that map, waits out the library's debounce, then fires a run. Keeping the
//! watcher here (a thin wrapper over `notify`) leaves the supervisor's decision
//! logic in the pure scheduler.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// library id → unix seconds of its most recent filesystem event.
pub type DirtyMap = Arc<Mutex<HashMap<String, f64>>>;

/// A live watcher plus the sorted root set it covers, so the supervisor can tell
/// when the watched libraries changed and a rebuild is needed.
pub struct FsWatch {
    _watcher: RecommendedWatcher,
    pub signature: Vec<PathBuf>,
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The sorted, de-duplicated set of roots for a `(library id, roots)` list — the
/// watcher's signature. Pure, so the supervisor can compare it without building a
/// watcher.
pub fn signature_of(libs: &[(String, Vec<PathBuf>)]) -> Vec<PathBuf> {
    let mut sig: Vec<PathBuf> = libs.iter().flat_map(|(_, r)| r.clone()).collect();
    sig.sort();
    sig.dedup();
    sig
}

/// Called (with no arguments) whenever an event stamps at least one library
/// dirty, so the supervisor can wake immediately instead of waiting for its next
/// poll. Wrapped in an `Arc` so the watcher thread can hold it.
pub type OnChange = Arc<dyn Fn() + Send + Sync>;

/// Start a recursive watcher over every root in `libs`, stamping the owning
/// library dirty in `dirty` on each relevant event and calling `on_change` to wake
/// the supervisor. Returns `None` when there is nothing to watch or the platform
/// watcher can't be created (best-effort — a failed watch just means that trigger
/// never fires, never a crash).
pub fn start(
    libs: &[(String, Vec<PathBuf>)],
    dirty: DirtyMap,
    on_change: OnChange,
) -> Option<FsWatch> {
    let signature = signature_of(libs);
    if signature.is_empty() {
        return None;
    }
    // Every (root, owning library id) pair, for attributing an event path back to
    // the library whose root contains it (roots can be shared across libraries).
    let owners: Vec<(PathBuf, String)> = libs
        .iter()
        .flat_map(|(id, roots)| roots.iter().map(move |r| (r.clone(), id.clone())))
        .collect();

    let handler = move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else {
            return;
        };
        if !matches!(
            ev.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }
        let now = now_secs();
        let mut stamped = false;
        {
            let mut d = dirty.lock().unwrap();
            for path in &ev.paths {
                for (root, id) in &owners {
                    if path.starts_with(root) {
                        d.insert(id.clone(), now);
                        stamped = true;
                    }
                }
            }
        }
        // Wake the supervisor so it can start the debounce clock right away.
        if stamped {
            on_change();
        }
    };

    let mut watcher = notify::recommended_watcher(handler).ok()?;
    for root in &signature {
        // A missing/inaccessible root simply isn't watched.
        let _ = watcher.watch(root, RecursiveMode::Recursive);
    }
    Some(FsWatch {
        _watcher: watcher,
        signature,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_sorted_and_deduped() {
        let libs = vec![
            ("a".into(), vec![PathBuf::from("/b"), PathBuf::from("/a")]),
            ("b".into(), vec![PathBuf::from("/a")]), // shared root
        ];
        let sig = signature_of(&libs);
        assert_eq!(sig, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn empty_roots_have_empty_signature() {
        assert!(signature_of(&[]).is_empty());
        let noop: OnChange = Arc::new(|| {});
        assert!(start(&[], Arc::new(Mutex::new(HashMap::new())), noop).is_none());
    }
}

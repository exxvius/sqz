//! Run orchestration: discovery, resume bookkeeping, and a bounded worker pool.
//!
//! Workers pull the next file to process from the manifest itself, so files
//! re-queued mid-run (retry / force) are picked up live. Each in-flight file has
//! its own cancel token in a shared registry, letting the UI abort one file
//! without stopping the whole run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::core::config::Config;
use crate::core::discover::discover;
use crate::core::encoders::Encoder;
use crate::core::ffbin::FfBin;
use crate::core::manifest::Manifest;
use crate::core::paths::all_temp_dirs;
use crate::core::pipeline::{process_file, scan_into_manifest};
use crate::core::report::{Outcome, Reporter};

/// Per-file cancel tokens for files currently being processed, keyed by path.
pub type ActiveMap = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

#[derive(Debug, Default, Clone, Serialize)]
pub struct RunSummary {
    pub done: u64,
    pub normalized: u64,
    pub skipped: u64,
    pub failed: u64,
    pub would: u64,
    pub saved_bytes: i64,
    pub total_discovered: usize,
    pub pending: usize,
    pub processed: usize,
    pub interrupted: bool,
}

fn clean_orphans(temp_dirs: &[PathBuf]) {
    for d in temp_dirs {
        if let Ok(entries) = std::fs::read_dir(d) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("sqz_") && name.ends_with(".mkv") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn tally(summary: &Mutex<RunSummary>, outcome: Outcome, saved: i64) {
    let mut s = summary.lock().unwrap();
    match outcome {
        Outcome::Done => {
            s.done += 1;
            s.saved_bytes += saved;
        }
        Outcome::Normalized => {
            s.normalized += 1;
            if saved > 0 {
                s.saved_bytes += saved;
            }
        }
        Outcome::SkippedEfficient | Outcome::SkippedMarginal | Outcome::SkippedNoGain => {
            s.skipped += 1;
        }
        Outcome::Failed => s.failed += 1,
        Outcome::DryRun => s.would += 1,
        Outcome::Cancelled => {}
    }
    if !matches!(outcome, Outcome::Cancelled) {
        s.processed += 1;
    }
}

/// Discover, resume, and process pending files until the queue drains or the run
/// is cancelled. `cancel`/`paused`/`active` are shared with the caller so the UI
/// can steer a run in flight.
#[allow(clippy::too_many_arguments)]
pub fn run(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    manifest: &Manifest,
    reporter: &dyn Reporter,
    cancel: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
    active: &ActiveMap,
) -> RunSummary {
    let _ = manifest.recover_processing();
    // Finish any swap a prior crash interrupted before we scan/process.
    crate::core::replace::recover_stashes(&cfg.inputs);

    let files = discover(cfg);
    for f in &files {
        scan_into_manifest(manifest, cfg, f);
    }
    let pending = manifest.pending_paths().map(|v| v.len()).unwrap_or(0);
    reporter.on_run_start(pending);

    let summary = Mutex::new(RunSummary {
        total_discovered: files.len(),
        pending,
        ..Default::default()
    });

    if pending == 0 {
        return summary.into_inner().unwrap();
    }

    clean_orphans(&all_temp_dirs(cfg, &files));
    let in_flight = AtomicUsize::new(0);

    thread::scope(|scope| {
        for _ in 0..cfg.workers {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                while paused.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(120));
                }
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let claimed = match manifest.claim_next_pending() {
                    Ok(Some(c)) => c,
                    Ok(None) => {
                        // Nothing to claim. Exit only when no one else is working
                        // either (so mid-run re-queues can still be picked up).
                        if in_flight.load(Ordering::Relaxed) == 0 {
                            break;
                        }
                        thread::sleep(Duration::from_millis(150));
                        continue;
                    }
                    Err(_) => break,
                };

                in_flight.fetch_add(1, Ordering::Relaxed);
                let file_cancel = Arc::new(AtomicBool::new(false));
                active
                    .lock()
                    .unwrap()
                    .insert(claimed.path.clone(), Arc::clone(&file_cancel));

                let result = process_file(
                    ff,
                    cfg,
                    encoder,
                    manifest,
                    &claimed.path,
                    claimed.forced,
                    cancel,
                    &file_cancel,
                    reporter,
                );

                active.lock().unwrap().remove(&claimed.path);
                in_flight.fetch_sub(1, Ordering::Relaxed);

                tally(&summary, result.outcome, result.saved_bytes);
                reporter.on_record(&result);
            });
        }
    });

    // Reset any files left mid-flight by a cancel back to pending for resume.
    let _ = manifest.recover_processing();

    let mut out = summary.into_inner().unwrap();
    out.interrupted = cancel.load(Ordering::Relaxed);
    out
}

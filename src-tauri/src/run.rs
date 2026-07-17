//! Run orchestration: discovery, resume bookkeeping, and a bounded worker pool
//! of threads each calling `process_file`, with cooperative cancel and pause.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Rolling tally of a run, surfaced to the UI summary.
#[derive(Debug, Default, Clone, Serialize)]
pub struct RunSummary {
    pub done: u64,
    pub skipped_efficient: u64,
    pub skipped_marginal: u64,
    pub skipped_no_gain: u64,
    pub failed: u64,
    pub would: u64,
    pub saved_bytes: i64,
    pub total_discovered: usize,
    pub pending: usize,
    pub processed: usize,
    pub interrupted: bool,
}

/// Remove leftover encode temp files from a prior interrupted run.
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
        Outcome::SkippedEfficient => s.skipped_efficient += 1,
        Outcome::SkippedMarginal => s.skipped_marginal += 1,
        Outcome::SkippedNoGain => s.skipped_no_gain += 1,
        Outcome::Failed => s.failed += 1,
        Outcome::DryRun => s.would += 1,
        Outcome::Cancelled => {}
    }
    if !matches!(outcome, Outcome::Cancelled) {
        s.processed += 1;
    }
}

/// Discover, resume, and process all pending files. Blocks until the run ends
/// (all files done, or cancelled). `cancel`/`paused` are shared with the caller
/// so the UI can steer a run in flight.
#[allow(clippy::too_many_arguments)]
pub fn run(
    ff: &FfBin,
    cfg: &Config,
    encoder: &Encoder,
    manifest: &Manifest,
    reporter: &dyn Reporter,
    cancel: &Arc<AtomicBool>,
    paused: &Arc<AtomicBool>,
) -> RunSummary {
    let files = discover(cfg);
    for f in &files {
        scan_into_manifest(manifest, cfg, f);
    }
    let pending = manifest.pending_paths().unwrap_or_default();

    let mut summary = RunSummary {
        total_discovered: files.len(),
        pending: pending.len(),
        ..Default::default()
    };

    if pending.is_empty() {
        return summary;
    }

    clean_orphans(&all_temp_dirs(cfg, &files));

    let queue: Mutex<VecDeque<String>> = Mutex::new(pending.into_iter().collect());
    let summary_mtx = Mutex::new(summary.clone());

    thread::scope(|scope| {
        for _ in 0..cfg.workers {
            scope.spawn(|| loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                // Cooperative pause: hold between files while paused.
                while paused.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(120));
                }
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let next = queue.lock().unwrap().pop_front();
                let path = match next {
                    Some(p) => p,
                    None => break, // queue drained
                };

                let result =
                    process_file(ff, cfg, encoder, manifest, &path, cancel, reporter);
                tally(&summary_mtx, result.outcome, result.saved_bytes);
                reporter.on_record(&result);
            });
        }
    });

    summary = summary_mtx.into_inner().unwrap();
    summary.interrupted = cancel.load(Ordering::Relaxed);
    summary
}

//! The pure unattended scheduler.
//!
//! This is the whole "when" of 1.2.0's set-and-forget mode, kept I/O-free and
//! clock-free so the trigger math is unit-testable: given the saved libraries,
//! the persisted per-library run history, the current local time, and whether the
//! machine is idle, it answers *which libraries should start an unattended run
//! now* — and, separately, *whether an in-flight unattended run should pause*.
//!
//! The supervisor thread (in `commands`) does the I/O around this: it reads the
//! real clock and idle signal, calls in here, and launches at most one due run per
//! tick through the ordinary run path.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Local, Timelike};
use serde::{Deserialize, Serialize};

use super::library::{SavedLibrary, Trigger};

/// Per-library unattended-run history: library id → unix seconds of the last
/// auto-run we launched. Kept in its own file (`data_dir/watch_state.json`) so it
/// can churn without dirtying the human-edited `libraries.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WatchState {
    #[serde(default)]
    pub last_auto_run: HashMap<String, f64>,
}

impl WatchState {
    /// Unix seconds of the last auto-run for `id`, if any.
    pub fn last(&self, id: &str) -> Option<f64> {
        self.last_auto_run.get(id).copied()
    }

    /// Record that an auto-run for `id` launched at `at` (unix seconds).
    pub fn mark(&mut self, id: &str, at: f64) {
        self.last_auto_run.insert(id.to_string(), at);
    }
}

/// Read the watch history. Missing/corrupt file → empty (same tolerance as the
/// library and settings stores).
pub fn load_state(path: &Path) -> WatchState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the watch history, creating the parent dir if needed.
pub fn save_state(path: &Path, state: &WatchState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// A library that should start an unattended run now.
#[derive(Debug, Clone, PartialEq)]
pub struct Due {
    pub library_id: String,
    /// Unix seconds of its previous auto-run (`None` = never), for ordering.
    pub last_run: Option<f64>,
}

/// Local instant of `trigger`'s time-of-day *today*, on the same date/timezone as
/// `now`. Uses component setters (not naive→local conversion) so it inherits
/// `now`'s offset and side-steps DST ambiguity.
fn daily_trigger_today(now: DateTime<Local>, hour: u8, minute: u8) -> DateTime<Local> {
    now.with_hour(hour as u32)
        .and_then(|d| d.with_minute(minute as u32))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(now)
}

/// Is a single trigger due at `now`, given its last auto-run (unix seconds)?
fn is_due(trigger: Trigger, last_run: Option<f64>, now: DateTime<Local>) -> bool {
    let now_secs = now.timestamp() as f64;
    match trigger {
        // Interval: due immediately if never run, else after the period elapses.
        Trigger::Interval { every_mins } => match last_run {
            None => true,
            Some(prev) => now_secs - prev >= (every_mins * 60) as f64,
        },
        // Daily: due once the time-of-day has arrived today and we haven't already
        // run since today's trigger instant. Handles "app was closed over the
        // trigger" for free — the persisted last_run is all it consults.
        Trigger::Daily { hour, minute } => {
            let trigger_today = daily_trigger_today(now, hour, minute);
            if now < trigger_today {
                return false;
            }
            match last_run {
                None => true,
                Some(prev) => (prev as i64) < trigger_today.timestamp(),
            }
        }
    }
}

/// Which watched libraries should start an unattended run at `now`, oldest-due
/// first (a library that has never auto-run sorts first). A library whose
/// `idle_only` is set is suppressed while `system_idle` is false. Pure.
pub fn due_libraries(
    libs: &[SavedLibrary],
    state: &WatchState,
    now: DateTime<Local>,
    system_idle: bool,
) -> Vec<Due> {
    let mut due: Vec<Due> = libs
        .iter()
        .filter(|l| l.watch.enabled)
        .filter(|l| !l.watch.idle_only || system_idle)
        .filter(|l| is_due(l.watch.trigger, state.last(&l.id), now))
        .map(|l| Due {
            library_id: l.id.clone(),
            last_run: state.last(&l.id),
        })
        .collect();
    // Oldest-due first: never-run (None) sorts ahead of any timestamp.
    due.sort_by(|a, b| {
        let ka = a.last_run.unwrap_or(f64::NEG_INFINITY);
        let kb = b.last_run.unwrap_or(f64::NEG_INFINITY);
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    due
}

/// Should an in-flight unattended run be paused right now? True when the running
/// library is `idle_only` and the machine is no longer idle — the supervisor
/// writes this straight to the run's existing `paused` token. Pure.
pub fn should_pause(active: &SavedLibrary, system_idle: bool) -> bool {
    active.watch.idle_only && !system_idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
    use crate::core::library::WatchConfig;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn watched(id: &str, trigger: Trigger, idle_only: bool) -> SavedLibrary {
        SavedLibrary {
            id: id.into(),
            name: id.into(),
            roots: vec![PathBuf::from("/movies")],
            profile: Config::default(),
            watch: WatchConfig {
                enabled: true,
                trigger,
                idle_only,
            },
            created_at: 1.0,
            updated_at: 1.0,
        }
    }

    /// A fixed local instant to anchor tests (offset is the machine's, but every
    /// comparison stays inside Local, so results are timezone-independent).
    fn at(hour: u32, minute: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2026, 7, 21, hour, minute, 0)
            .unwrap()
    }

    #[test]
    fn interval_is_due_when_never_run() {
        let libs = vec![watched("a", Trigger::Interval { every_mins: 60 }, false)];
        let due = due_libraries(&libs, &WatchState::default(), at(12, 0), true);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].library_id, "a");
    }

    #[test]
    fn interval_waits_for_the_period_to_elapse() {
        let libs = vec![watched("a", Trigger::Interval { every_mins: 60 }, false)];
        let now = at(12, 0);
        let mut state = WatchState::default();
        // Ran 30 min ago — not yet due.
        state.mark("a", now.timestamp() as f64 - 30.0 * 60.0);
        assert!(due_libraries(&libs, &state, now, true).is_empty());
        // Ran 90 min ago — due.
        state.mark("a", now.timestamp() as f64 - 90.0 * 60.0);
        assert_eq!(due_libraries(&libs, &state, now, true).len(), 1);
    }

    #[test]
    fn daily_fires_once_past_its_time_and_not_before() {
        let libs = vec![watched("a", Trigger::Daily { hour: 3, minute: 0 }, false)];
        // 02:00 — before the 03:00 trigger.
        assert!(due_libraries(&libs, &WatchState::default(), at(2, 0), true).is_empty());
        // 04:00 — past it, never run → due.
        assert_eq!(
            due_libraries(&libs, &WatchState::default(), at(4, 0), true).len(),
            1
        );
    }

    #[test]
    fn daily_does_not_fire_twice_the_same_day() {
        let libs = vec![watched("a", Trigger::Daily { hour: 3, minute: 0 }, false)];
        let now = at(4, 0);
        let mut state = WatchState::default();
        // Already ran at 03:30 today — not due again.
        state.mark("a", at(3, 30).timestamp() as f64);
        assert!(due_libraries(&libs, &state, now, true).is_empty());
    }

    #[test]
    fn daily_catches_up_when_last_run_was_yesterday() {
        let libs = vec![watched("a", Trigger::Daily { hour: 3, minute: 0 }, false)];
        let now = at(4, 0);
        let mut state = WatchState::default();
        // Last ran a full day ago — the app was closed over today's trigger → due.
        state.mark("a", now.timestamp() as f64 - 24.0 * 3600.0);
        assert_eq!(due_libraries(&libs, &state, now, true).len(), 1);
    }

    #[test]
    fn idle_only_is_suppressed_when_not_idle() {
        let libs = vec![watched("a", Trigger::Interval { every_mins: 60 }, true)];
        // Due by schedule, but the machine is busy → held back.
        assert!(due_libraries(&libs, &WatchState::default(), at(12, 0), false).is_empty());
        // Same schedule, machine idle → due.
        assert_eq!(
            due_libraries(&libs, &WatchState::default(), at(12, 0), true).len(),
            1
        );
    }

    #[test]
    fn disabled_libraries_are_never_due() {
        let mut lib = watched("a", Trigger::Interval { every_mins: 60 }, false);
        lib.watch.enabled = false;
        assert!(due_libraries(&[lib], &WatchState::default(), at(12, 0), true).is_empty());
    }

    #[test]
    fn due_list_is_oldest_first() {
        let libs = vec![
            watched("recent", Trigger::Interval { every_mins: 60 }, false),
            watched("never", Trigger::Interval { every_mins: 60 }, false),
            watched("old", Trigger::Interval { every_mins: 60 }, false),
        ];
        let now = at(12, 0);
        let mut state = WatchState::default();
        state.mark("recent", now.timestamp() as f64 - 2.0 * 3600.0);
        state.mark("old", now.timestamp() as f64 - 10.0 * 3600.0);
        // "never" has no entry.
        let due = due_libraries(&libs, &state, now, true);
        let order: Vec<&str> = due.iter().map(|d| d.library_id.as_str()).collect();
        assert_eq!(order, vec!["never", "old", "recent"]);
    }

    #[test]
    fn should_pause_only_for_idle_only_when_busy() {
        let idle_lib = watched("a", Trigger::default(), true);
        assert!(should_pause(&idle_lib, false));
        assert!(!should_pause(&idle_lib, true));
        let anytime_lib = watched("b", Trigger::default(), false);
        assert!(!should_pause(&anytime_lib, false));
    }

    #[test]
    fn state_round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("sqz-sched-{}", uuid::Uuid::new_v4()));
        let path = dir.join("watch_state.json");
        assert!(load_state(&path).last("a").is_none());
        let mut state = WatchState::default();
        state.mark("a", 123.0);
        save_state(&path, &state).unwrap();
        assert_eq!(load_state(&path).last("a"), Some(123.0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

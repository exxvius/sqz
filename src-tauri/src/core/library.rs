//! Saved libraries: a named set of folders plus its own embedded encode profile.
//!
//! A saved library is the durable, re-runnable unit that 1.2.0's unattended mode
//! binds a watch-folder to. The _profile_ is just a [`Config`] with `inputs`
//! cleared — a saved settings blob scoped to one library, so running a library is
//! `{ ...profile, inputs: roots }` through the existing run path. There is
//! deliberately **one profile per library** (embedded, not a shared registry):
//! "different targets per folder" is expressed as separate libraries.
//!
//! Storage is a flat, ordered `Vec<SavedLibrary>` in `data_dir/libraries.json` —
//! libraries are low-frequency config (tens, not thousands), so they live beside
//! `settings.json` rather than in the per-file manifest DB.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::config::{Config, OnSuccess};

/// Smallest interval (minutes) an `Interval` trigger may fire on, so a fat-fingered
/// "every 1 minute" can't turn unattended mode into a busy loop.
pub const MIN_INTERVAL_MINS: u64 = 15;

/// When a watched library fires an unattended run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Trigger {
    /// Re-run every `every_mins` minutes (clamped to [`MIN_INTERVAL_MINS`]).
    Interval { every_mins: u64 },
    /// Re-run once per calendar day at a local wall-clock time.
    Daily { hour: u8, minute: u8 },
}

impl Default for Trigger {
    /// Overnight at 03:00 — the "process while I sleep" default.
    fn default() -> Self {
        Trigger::Daily { hour: 3, minute: 0 }
    }
}

/// Per-library unattended-run configuration. All of it round-trips through
/// `libraries.json`; the toggle that flips `enabled` is the one-click Watch button,
/// the rest live in the library editor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    /// This library participates in unattended runs.
    pub enabled: bool,
    /// When it fires.
    pub trigger: Trigger,
    /// Only start (and stay running) while the machine is input-idle.
    pub idle_only: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger: Trigger::default(),
            idle_only: true,
        }
    }
}

impl WatchConfig {
    /// Clamp/validate the schedule for persistence: an `Interval` can't dip below
    /// the floor, and a `Daily` time must be a real wall-clock instant.
    fn normalized(mut self) -> Result<Self, String> {
        match &mut self.trigger {
            Trigger::Interval { every_mins } => {
                *every_mins = (*every_mins).max(MIN_INTERVAL_MINS);
            }
            Trigger::Daily { hour, minute } => {
                if *hour > 23 || *minute > 59 {
                    return Err("Daily schedule must be a valid time of day.".into());
                }
            }
        }
        Ok(self)
    }
}

/// A named folder set with its own encode profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLibrary {
    /// Stable random slug. Assigned on first save and never reused, so renaming a
    /// library never breaks a 1.2.0 watch binding.
    pub id: String,
    /// User-facing label ("Movies", "Phone clips").
    pub name: String,
    /// The folders this library covers.
    pub roots: Vec<PathBuf>,
    /// Encode profile: a `Config` with `inputs` empty. `Config`'s
    /// `#[serde(default)]` lets old/new fields round-trip as the schema evolves.
    pub profile: Config,
    /// Unattended-run schedule. `#[serde(default)]` so every library already in
    /// `libraries.json` round-trips as "not watched".
    #[serde(default)]
    pub watch: WatchConfig,
    /// Unix seconds when first saved.
    pub created_at: f64,
    /// Unix seconds of the most recent save.
    pub updated_at: f64,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl SavedLibrary {
    /// Normalize a library for persistence: assign an id/timestamps if missing,
    /// refresh `updated_at`, strip any paths from the profile (a profile never
    /// carries inputs), and validate the encode target. Returns the cleaned copy.
    pub fn normalized(mut self) -> Result<Self, String> {
        let t = now();
        if self.id.trim().is_empty() {
            self.id = uuid::Uuid::new_v4().to_string();
        }
        if self.created_at <= 0.0 {
            self.created_at = t;
        }
        self.updated_at = t;
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return Err("A library needs a name.".into());
        }
        // A profile is codec settings only — never file paths.
        self.profile.inputs = Vec::new();
        // Clamp/validate the watch schedule at the boundary too.
        self.watch = self.watch.normalized()?;
        // Validate encode params, but the holding-dir requirement is a run-time
        // concern: `finalize_config` injects the managed holding path when a run
        // starts, so a stored profile legitimately keeps `holding_dir = None` even
        // in Holding mode. Check a clone with that path filled so the real profile
        // stays machine-independent.
        let mut check = self.profile.clone();
        if matches!(check.on_success, OnSuccess::Holding) && check.holding_dir.is_none() {
            check.holding_dir = Some(PathBuf::from("<run-time>"));
        }
        check.validate()?;
        Ok(self)
    }
}

/// Insert or replace `lib` by id, preserving order (an existing id updates in
/// place; a new id is appended). Pure — new vec out, no I/O.
pub fn upsert(mut libs: Vec<SavedLibrary>, lib: SavedLibrary) -> Vec<SavedLibrary> {
    if let Some(existing) = libs.iter_mut().find(|l| l.id == lib.id) {
        *existing = lib;
    } else {
        libs.push(lib);
    }
    libs
}

/// Drop the library with `id`, preserving order. Pure.
pub fn remove(libs: Vec<SavedLibrary>, id: &str) -> Vec<SavedLibrary> {
    libs.into_iter().filter(|l| l.id != id).collect()
}

/// Read the ordered library list. A missing or unparseable file yields an empty
/// list rather than an error — same tolerance as `get_settings`.
pub fn load_all(path: &Path) -> Vec<SavedLibrary> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the ordered library list, creating the parent dir if needed.
pub fn save_all(path: &Path, libs: &[SavedLibrary]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(libs).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(id: &str, name: &str) -> SavedLibrary {
        SavedLibrary {
            id: id.into(),
            name: name.into(),
            roots: vec![PathBuf::from("/movies")],
            profile: Config::default(),
            watch: WatchConfig::default(),
            created_at: 1.0,
            updated_at: 1.0,
        }
    }

    #[test]
    fn upsert_replaces_in_place_and_keeps_order() {
        let libs = vec![lib("a", "A"), lib("b", "B"), lib("c", "C")];
        let updated = upsert(libs, lib("b", "B-renamed"));
        assert_eq!(updated.len(), 3);
        assert_eq!(updated[1].id, "b");
        assert_eq!(updated[1].name, "B-renamed");
        // Order is preserved: a, b, c.
        assert_eq!(updated[0].id, "a");
        assert_eq!(updated[2].id, "c");
    }

    #[test]
    fn upsert_appends_a_new_id() {
        let libs = vec![lib("a", "A")];
        let updated = upsert(libs, lib("z", "Z"));
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1].id, "z");
    }

    #[test]
    fn remove_drops_only_the_target() {
        let libs = vec![lib("a", "A"), lib("b", "B")];
        let updated = remove(libs, "a");
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "b");
        // Removing an unknown id is a no-op.
        let same = remove(updated, "nope");
        assert_eq!(same.len(), 1);
    }

    #[test]
    fn normalized_assigns_id_and_timestamps_when_missing() {
        let raw = SavedLibrary {
            id: String::new(),
            name: "  Movies  ".into(),
            roots: vec![],
            profile: Config::default(),
            watch: WatchConfig::default(),
            created_at: 0.0,
            updated_at: 0.0,
        };
        let n = raw.normalized().unwrap();
        assert!(!n.id.is_empty());
        assert!(n.created_at > 0.0);
        assert!(n.updated_at > 0.0);
        // Name is trimmed.
        assert_eq!(n.name, "Movies");
    }

    #[test]
    fn normalized_preserves_existing_id_and_created_at() {
        let mut raw = lib("keep-me", "Named");
        raw.created_at = 42.0;
        let n = raw.normalized().unwrap();
        assert_eq!(n.id, "keep-me");
        assert_eq!(n.created_at, 42.0);
        // updated_at is always refreshed.
        assert!(n.updated_at > 42.0);
    }

    #[test]
    fn normalized_strips_profile_inputs() {
        let mut raw = lib("a", "A");
        raw.profile.inputs = vec![PathBuf::from("/leaked/path.mp4")];
        let n = raw.normalized().unwrap();
        assert!(n.profile.inputs.is_empty());
    }

    #[test]
    fn normalized_rejects_empty_name() {
        let raw = lib("a", "   ");
        assert!(raw.normalized().is_err());
    }

    #[test]
    fn normalized_allows_holding_mode_without_a_dir() {
        // holding_dir is injected at run time, so a stored Holding profile with a
        // null dir must still save.
        let mut raw = lib("a", "A");
        raw.profile.on_success = OnSuccess::Holding;
        raw.profile.holding_dir = None;
        let n = raw.normalized().unwrap();
        // The stored profile stays machine-independent (no placeholder leaked in).
        assert!(n.profile.holding_dir.is_none());
    }

    #[test]
    fn default_library_is_unwatched() {
        assert!(!WatchConfig::default().enabled);
        // Idle-only is the safe default: unattended work stays out of the user's way.
        assert!(WatchConfig::default().idle_only);
    }

    #[test]
    fn normalized_clamps_interval_below_the_floor() {
        let mut raw = lib("a", "A");
        raw.watch.enabled = true;
        raw.watch.trigger = Trigger::Interval { every_mins: 1 };
        let n = raw.normalized().unwrap();
        assert_eq!(
            n.watch.trigger,
            Trigger::Interval {
                every_mins: MIN_INTERVAL_MINS
            }
        );
    }

    #[test]
    fn normalized_rejects_an_impossible_daily_time() {
        let mut raw = lib("a", "A");
        raw.watch.trigger = Trigger::Daily {
            hour: 25,
            minute: 0,
        };
        assert!(raw.normalized().is_err());
    }

    #[test]
    fn watch_defaults_when_absent_from_json() {
        // A library persisted before 1.2.0 has no `watch` key; it must round-trip
        // as unwatched rather than fail to parse.
        let json = r#"{
            "id": "old",
            "name": "Legacy",
            "roots": ["/movies"],
            "profile": {},
            "created_at": 1.0,
            "updated_at": 1.0
        }"#;
        let parsed: SavedLibrary = serde_json::from_str(json).unwrap();
        assert!(!parsed.watch.enabled);
    }

    #[test]
    fn normalized_rejects_an_invalid_profile() {
        let mut raw = lib("a", "A");
        raw.profile.vmaf_target = Some(200.0); // out of [0, 100]
        assert!(raw.normalized().is_err());
    }

    #[test]
    fn load_all_of_missing_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("sqz-lib-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("libraries.json");
        assert!(load_all(&path).is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("sqz-lib-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("libraries.json");
        let libs = vec![lib("a", "A"), lib("b", "B")];
        save_all(&path, &libs).unwrap();
        let loaded = load_all(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "a");
        assert_eq!(loaded[1].name, "B");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

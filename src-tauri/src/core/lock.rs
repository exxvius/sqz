//! Locked mode: a password-gated privacy screen that masks personal info (file
//! names, paths) in the UI and makes the app read-only — no run control, no
//! settings changes, no manual database edits — until it's unlocked.
//!
//! Only the password *hash* and the locked flag are persisted here — in a
//! dedicated `lock.json`, deliberately kept out of `settings.json` so the hash
//! never leaks through Export Settings. Nothing on disk is encrypted; this guards
//! the *app*, not the files, so the escape hatch for a lost password is simply
//! deleting `lock.json` from the app data folder while the app is closed.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "lock.json";

/// Persisted lock state. `locked` survives restarts on purpose: if the machine is
/// left encoding, the app relaunches locked and demands the password.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LockState {
    /// Argon2 PHC hash string (embeds its own salt). `None` = not configured.
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

/// Owns the persisted state behind a mutex plus its backing file path.
pub struct Lock {
    path: PathBuf,
    state: Mutex<LockState>,
}

impl Lock {
    /// Load persisted state from `dir/lock.json` (defaulting when absent).
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE_NAME);
        let state = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    fn persist(&self, st: &LockState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, text).map_err(|e| e.to_string())
    }

    /// `(configured, locked)` for the UI.
    pub fn status(&self) -> (bool, bool) {
        let st = self.state.lock().unwrap();
        (st.hash.is_some(), st.locked)
    }

    /// Whether the app is currently locked (mask + block edits/run control).
    pub fn is_locked(&self) -> bool {
        self.state.lock().unwrap().locked
    }

    /// First-time setup: store the password hash. Errors if already configured.
    pub fn setup(&self, password: &str) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        if st.hash.is_some() {
            return Err("A lock password is already set.".into());
        }
        if password.is_empty() {
            return Err("Password can't be empty.".into());
        }
        st.hash = Some(hash_password(password)?);
        self.persist(&st)
    }

    /// Lock the app. No password required (locking is always safe), but a password
    /// must have been set up first.
    pub fn engage(&self) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        if st.hash.is_none() {
            return Err("Set a lock password first.".into());
        }
        st.locked = true;
        self.persist(&st)
    }

    /// Unlock the app. Always requires the correct password.
    pub fn release(&self, password: &str) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        verify(st.hash.as_deref(), password)?;
        st.locked = false;
        self.persist(&st)
    }

    /// Change the password. Only allowed while unlocked.
    pub fn change_password(&self, old: &str, new: &str) -> Result<(), String> {
        let mut st = self.state.lock().unwrap();
        if st.locked {
            return Err("Unlock the app before changing the password.".into());
        }
        verify(st.hash.as_deref(), old)?;
        if new.is_empty() {
            return Err("Password can't be empty.".into());
        }
        st.hash = Some(hash_password(new)?);
        self.persist(&st)
    }
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify(hash: Option<&str>, password: &str) -> Result<(), String> {
    let hash = hash.ok_or("No lock password is set.")?;
    let parsed = PasswordHash::new(hash).map_err(|e| e.to_string())?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| "Incorrect password.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> Lock {
        let dir = std::env::temp_dir().join(format!("sqz-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Lock::load(&dir)
    }

    #[test]
    fn setup_then_verify_roundtrip() {
        let lock = temp();
        assert_eq!(lock.status(), (false, false));
        lock.setup("hunter2").unwrap();
        assert_eq!(lock.status(), (true, false));
        lock.engage().unwrap();
        assert!(lock.is_locked());
        assert!(lock.release("wrong").is_err());
        assert!(lock.is_locked(), "wrong password must not unlock");
        lock.release("hunter2").unwrap();
        assert!(!lock.is_locked());
    }

    #[test]
    fn cannot_change_password_while_locked() {
        let lock = temp();
        lock.setup("a").unwrap();
        lock.engage().unwrap();
        assert!(lock.change_password("a", "b").is_err());
        lock.release("a").unwrap();
        lock.change_password("a", "b").unwrap();
        lock.engage().unwrap();
        lock.release("b").unwrap();
    }

    #[test]
    fn locked_state_persists_across_reload() {
        let dir = std::env::temp_dir().join(format!("sqz-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        {
            let lock = Lock::load(&dir);
            lock.setup("pw").unwrap();
            lock.engage().unwrap();
        }
        let reloaded = Lock::load(&dir);
        assert_eq!(reloaded.status(), (true, true));
    }
}

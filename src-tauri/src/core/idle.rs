//! System input-idle probe for unattended mode.
//!
//! Turns "has the user touched the keyboard/mouse lately?" into the single
//! `system_idle` boolean the scheduler consumes. Only Windows can answer today
//! (via `GetLastInputInfo`); every other platform reports *unknown*, which
//! callers treat as "idle" so unattended runs are never blocked on a system that
//! can't report activity.

/// Default input-idle threshold: no input for this many seconds reads as "away."
pub const DEFAULT_IDLE_SECS: f64 = 120.0;

/// Seconds since the last keyboard/mouse input, or `None` if the platform can't
/// tell (non-Windows for now).
pub fn idle_seconds() -> Option<f64> {
    #[cfg(windows)]
    {
        imp::idle_seconds()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Is the machine idle — no user input for at least `threshold_secs`? An unknown
/// idle time (unsupported platform) counts as idle, so it never gates a run.
pub fn is_idle(threshold_secs: f64) -> bool {
    idle_from(idle_seconds(), threshold_secs)
}

/// Pure threshold decision, split out so it's testable without the syscall.
fn idle_from(seconds: Option<f64>, threshold_secs: f64) -> bool {
    match seconds {
        Some(s) => s >= threshold_secs,
        None => true,
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    pub fn idle_seconds() -> Option<f64> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` is a fully-initialized LASTINPUTINFO with `cbSize` set as
        // the API requires; GetLastInputInfo only reads cbSize and writes dwTime.
        let ok = unsafe { GetLastInputInfo(&mut info) };
        if ok == 0 {
            return None;
        }
        // SAFETY: GetTickCount takes no arguments and has no failure mode.
        let now = unsafe { GetTickCount() };
        // Tick counts are u32 and wrap ~49.7 days after boot; wrapping_sub yields
        // the correct elapsed span across a wrap boundary.
        let idle_ms = now.wrapping_sub(info.dwTime);
        Some(idle_ms as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_idle_time_counts_as_idle() {
        assert!(idle_from(None, DEFAULT_IDLE_SECS));
    }

    #[test]
    fn idle_when_past_the_threshold() {
        assert!(idle_from(Some(200.0), 120.0));
        assert!(idle_from(Some(120.0), 120.0));
    }

    #[test]
    fn active_when_below_the_threshold() {
        assert!(!idle_from(Some(30.0), 120.0));
    }
}

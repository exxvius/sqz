//! Library health state: fold a probe outcome and an optional decode outcome into
//! one durable per-file verdict.
//!
//! The scan pass in [`commands`](crate::commands) does the I/O (probe + decode);
//! this module is the pure decision that turns those raw signals into a
//! [`HealthState`], so the rule ordering is testable in isolation.

/// A file's health as recorded in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    /// Probes (and, on a deep scan, decodes) cleanly.
    Healthy,
    /// Decoded with errors — likely truncated or corrupted.
    Corrupt,
    /// ffprobe couldn't read it at all (not a valid/known media file).
    Unreadable,
}

impl HealthState {
    /// Stable lowercase slug stored in the manifest and sent to the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Corrupt => "corrupt",
            HealthState::Unreadable => "unreadable",
        }
    }
}

/// Fold the raw scan signals into a single state. Worst-first: an unreadable file
/// can't be judged further, and a decode failure means corruption.
///
///   - `probed`  — did ffprobe read the file?
///   - `decode`  — `Some(false)` if a decode pass hit errors, `Some(true)` if it
///     passed, `None` if no decode was run (structural-only scan).
pub fn classify(probed: bool, decode: Option<bool>) -> HealthState {
    if !probed {
        return HealthState::Unreadable;
    }
    if decode == Some(false) {
        return HealthState::Corrupt;
    }
    HealthState::Healthy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unprobed_is_unreadable() {
        assert_eq!(classify(false, None), HealthState::Unreadable);
        assert_eq!(classify(false, Some(true)), HealthState::Unreadable);
    }

    #[test]
    fn decode_failure_is_corrupt() {
        assert_eq!(classify(true, Some(false)), HealthState::Corrupt);
    }

    #[test]
    fn clean_file_is_healthy() {
        assert_eq!(classify(true, Some(true)), HealthState::Healthy);
        // Structural-only scan (no decode) is healthy when it probes.
        assert_eq!(classify(true, None), HealthState::Healthy);
    }

    #[test]
    fn state_slugs_are_stable() {
        assert_eq!(HealthState::Healthy.as_str(), "healthy");
        assert_eq!(HealthState::Corrupt.as_str(), "corrupt");
        assert_eq!(HealthState::Unreadable.as_str(), "unreadable");
    }
}

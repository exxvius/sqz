//! Staged early-abort judge.
//!
//! As an encode progresses, ffmpeg tells us how many bytes it has written. From
//! that we project the final size and decide, in stages, whether finishing is
//! worth it. The later the encode, the more we've already invested, so the bar
//! for bailing gets stricter:
//!
//!   • before stage 1        — too little data, wait
//!   • stage 1 (≥5%)         — bail if wildly bloated (projected ≥ +25%)
//!   • stage 2 (≥10%)        — bail if under the savings gate AND getting worse
//!   • stage 3 (≥25%, <75%)  — bail the moment projection is under the gate
//!   • stage 4 (≥75%)        — nearly done; bail only if savings fall under a floor

use super::config::Config;

/// The projection that triggered (or would trigger) an abort.
#[derive(Debug, Clone, Copy)]
pub struct AbortProjection {
    pub frac: f64,
    pub projected: f64,
}

/// Progress fraction at which stage 3 (strict gate) begins.
const STAGE3_AT: f64 = 0.25;
/// How many recent samples define the trend window.
const TREND_WINDOW: usize = 5;

#[derive(Debug, Clone)]
pub struct AbortConfig {
    pub enabled: bool,
    pub stage1_at: f64,
    pub bloat_margin: f64,
    pub check_at: f64,
    pub late_at: f64,
    pub min_savings: f64,
    pub late_min_savings: f64,
    pub duration: f64,
    pub src_size: u64,
}

impl AbortConfig {
    pub fn from(cfg: &Config, duration: Option<f64>, src_size: u64) -> Self {
        Self {
            enabled: cfg.early_abort,
            stage1_at: cfg.abort_stage1_at,
            bloat_margin: cfg.abort_bloat_margin,
            check_at: cfg.abort_check_at,
            late_at: cfg.abort_late_at,
            min_savings: cfg.min_savings,
            late_min_savings: cfg.abort_late_min_savings,
            duration: duration.unwrap_or(0.0),
            src_size,
        }
    }
}

pub struct AbortJudge {
    cfg: AbortConfig,
    /// (progress fraction, projected final bytes) samples.
    samples: Vec<(f64, f64)>,
}

impl AbortJudge {
    pub fn new(cfg: AbortConfig) -> Self {
        Self {
            cfg,
            samples: Vec::new(),
        }
    }

    /// Feed one progress tick. Returns the projection if the encode should abort.
    pub fn observe(&mut self, sec: f64, out_bytes: Option<u64>) -> Option<AbortProjection> {
        if !self.cfg.enabled || self.cfg.duration <= 0.0 || self.cfg.src_size == 0 {
            return None;
        }
        let out_bytes = out_bytes?;
        if out_bytes == 0 {
            return None;
        }
        let frac = sec / self.cfg.duration;
        if frac <= 0.0 {
            return None;
        }
        let projected = out_bytes as f64 / frac;
        self.samples.push((frac, projected));

        let src = self.cfg.src_size as f64;
        let savings = 1.0 - projected / src;
        let hit = || Some(AbortProjection { frac, projected });

        if frac < self.cfg.stage1_at {
            None
        } else if frac < self.cfg.check_at {
            // Stage 1: catch a runaway that will clearly be much larger.
            (projected >= src * (1.0 + self.cfg.bloat_margin))
                .then(hit)
                .flatten()
        } else if frac < STAGE3_AT {
            // Stage 2: under the gate and trending worse.
            (savings < self.cfg.min_savings && self.is_worsening())
                .then(hit)
                .flatten()
        } else if frac < self.cfg.late_at {
            // Stage 3: strict — under the gate at all.
            (savings < self.cfg.min_savings).then(hit).flatten()
        } else {
            // Stage 4: almost done; only bail if it's really not worth it.
            (savings < self.cfg.late_min_savings).then(hit).flatten()
        }
    }

    /// True if the projected size is trending upward (savings shrinking) over the
    /// recent window. Needs a few samples so a single noisy tick can't trigger it.
    fn is_worsening(&self) -> bool {
        let n = self.samples.len();
        if n < 3 {
            return false;
        }
        let window = &self.samples[n.saturating_sub(TREND_WINDOW)..];
        let first = window.first().unwrap().1;
        let last = window.last().unwrap().1;
        last > first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(src: u64, dur: f64) -> AbortConfig {
        AbortConfig {
            enabled: true,
            stage1_at: 0.05,
            bloat_margin: 0.25,
            check_at: 0.10,
            late_at: 0.75,
            min_savings: 0.10,
            late_min_savings: 0.03,
            duration: dur,
            src_size: src,
        }
    }

    // projected = out_bytes / frac, and frac = sec/dur. To hit a target projected
    // size P at fraction f, feed out_bytes = P * f.
    fn tick(j: &mut AbortJudge, dur: f64, frac: f64, projected: f64) -> Option<AbortProjection> {
        let sec = frac * dur;
        let out_bytes = (projected * frac) as u64;
        j.observe(sec, Some(out_bytes))
    }

    #[test]
    fn stage1_aborts_on_bloat() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // At 6%, projected 1.4M vs 1.0M src → +40% → abort.
        assert!(tick(&mut j, 100.0, 0.06, 1_400_000.0).is_some());
    }

    #[test]
    fn stage1_waits_when_not_bloated() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // At 6%, projected 1.1M (+10%) — bad but not runaway; wait.
        assert!(tick(&mut j, 100.0, 0.06, 1_100_000.0).is_none());
    }

    #[test]
    fn stage2_aborts_when_under_gate_and_worsening() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // Climbing projections in the 10–25% window, all under the 10% gate.
        assert!(tick(&mut j, 100.0, 0.11, 0.93e6).is_none()); // first sample
        tick(&mut j, 100.0, 0.13, 0.95e6);
        let r = tick(&mut j, 100.0, 0.15, 0.98e6); // worsening + under gate
        assert!(r.is_some());
    }

    #[test]
    fn stage2_holds_when_improving() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // Projections falling (savings improving) — do not abort even if under gate.
        tick(&mut j, 100.0, 0.11, 0.98e6);
        tick(&mut j, 100.0, 0.13, 0.95e6);
        assert!(tick(&mut j, 100.0, 0.15, 0.93e6).is_none());
    }

    #[test]
    fn stage3_aborts_immediately_under_gate() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // At 40%, projected 0.95M → only 5% savings, under the 10% gate → abort.
        assert!(tick(&mut j, 100.0, 0.40, 0.95e6).is_some());
    }

    #[test]
    fn stage4_keeps_going_above_floor() {
        let mut j = AbortJudge::new(cfg(1_000_000, 100.0));
        // At 80%, projected 0.94M → 6% savings, above the 3% floor → keep.
        assert!(tick(&mut j, 100.0, 0.80, 0.94e6).is_none());
        // But 0.98M → 2% savings, under the floor → abort.
        assert!(tick(&mut j, 100.0, 0.85, 0.98e6).is_some());
    }

    #[test]
    fn disabled_never_aborts() {
        let mut c = cfg(1_000_000, 100.0);
        c.enabled = false;
        let mut j = AbortJudge::new(c);
        assert!(tick(&mut j, 100.0, 0.5, 2_000_000.0).is_none());
    }
}

//! Time abstraction. `SystemTime`/`Instant` are clippy-banned outside this
//! module: every latency budget (docs/GOALS.md) and every DST scenario must
//! be reproducible under a virtual clock.

/// Source of ordered time for engines and pipelines.
pub trait Clock: Send + Sync {
    /// Current logical time in milliseconds since an arbitrary epoch.
    /// Monotonic non-decreasing within one clock instance.
    fn now_ms(&self) -> u64;
}

/// Real monotonic clock. The only site in the workspace allowed to touch
/// [`std::time::Instant`].
#[derive(Debug)]
pub struct MonotonicClock {
    origin: std::sync::OnceLock<std::time::Instant>,
}

impl MonotonicClock {
    /// New wall clock anchored at first call (not construction), so a clock
    /// created at process start reads ~0 on first use.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            origin: std::sync::OnceLock::new(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_ms(&self) -> u64 {
        // INVARIANT: sole sanctioned Instant use in the workspace; the DST
        // harness (peregrine-dst) substitutes a virtual clock everywhere else.
        #[allow(clippy::disallowed_methods)]
        let now = std::time::Instant::now();
        let origin = *self.origin.get_or_init(|| now);
        u64::try_from((now - origin).as_millis()).unwrap_or(u64::MAX)
    }
}

/// Deterministic clock for tests and simulation; advances only when told.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    t_ms: u64,
}

impl FixedClock {
    /// New virtual clock at `t_ms`.
    #[must_use]
    pub const fn at(t_ms: u64) -> Self {
        Self { t_ms }
    }

    /// Advance the virtual clock; time only moves when the scenario moves it.
    pub const fn advance(&mut self, ms: u64) {
        self.t_ms += ms;
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.t_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_advancement_ordered() {
        let mut c = FixedClock::at(0);
        assert_eq!(c.now_ms(), 0);
        c.advance(250);
        assert_eq!(c.now_ms(), 250);
        c.advance(1);
        assert_eq!(c.now_ms(), 251);
    }

    #[test]
    fn monotonic_clock_never_goes_backwards() {
        let c = MonotonicClock::new();
        let a = c.now_ms();
        let b = c.now_ms();
        assert!(b >= a);
    }
}

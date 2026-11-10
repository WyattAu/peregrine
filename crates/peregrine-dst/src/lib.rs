//! Peregrine DST (catalog-3 track O): the deterministic simulation
//! harness. Time, randomness and (later) network and disk are fully
//! controlled; scenarios inject partitions, key delays and out-of-order
//! syncs, and two runs of the same scenario must be byte-identical.
//! TigerBeetle/FoundationDB discipline from week one.

use peregrine_core::clock::{Clock, FixedClock};

/// `SplitMix64`: tiny, seedable, deterministic. Good enough for scenario
/// scheduling; real crypto randomness never comes from here.
#[derive(Debug, Clone, Copy)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// New generator from a scenario seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next deterministic 64-bit value.
    #[must_use]
    pub const fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Deterministic uniform draw in `[0, bound)`; a zero bound is treated
    /// as 1.
    #[must_use]
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

/// Faults a scenario may inject into the simulated environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Drop every event in a window (network partition).
    Partition,
    /// Delay Megolm key arrival by `ms` (out-of-order decrypt race).
    KeyDelay {
        /// Virtual delay applied to key delivery.
        ms: u64,
    },
    /// Reorder delivery of `n` in-flight events.
    Reorder {
        /// Number of in-flight events to permute.
        n: u32,
    },
    /// Fail the next `count` writes at the store boundary (disk fault).
    DiskFault {
        /// Number of writes to fail.
        count: u32,
    },
}

/// One reproducible scenario run: virtual clock + seeded schedule.
pub struct Scenario {
    clock: FixedClock,
    rng: SplitMix64,
    faults: Vec<Fault>,
}

impl Scenario {
    /// New scenario; identical seeds produce identical runs, forever.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            clock: FixedClock::at(0),
            rng: SplitMix64::new(seed),
            faults: Vec::new(),
        }
    }

    /// Schedule a fault at the current virtual time.
    pub fn inject(&mut self, at_ms: u64, fault: Fault) -> &mut Self {
        self.clock.advance(at_ms);
        self.faults.push(fault);
        self
    }

    /// Current virtual time.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// Deterministic jitter draw for the scenario scheduler.
    #[must_use]
    pub fn jitter_ms(&mut self, bound: u64) -> u64 {
        self.rng.below(bound)
    }

    /// Faults injected so far, in schedule order.
    #[must_use]
    pub fn faults(&self) -> &[Fault] {
        &self.faults
    }
}

impl Clock for Scenario {
    fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)] // tests: panics are the assertion mechanism
mod tests {
    use super::*;

    fn run(seed: u64) -> (u64, Vec<u64>) {
        let mut s = Scenario::new(seed);
        s.inject(100, Fault::Partition);
        let jitters = (0..4).map(|_| s.jitter_ms(1000)).collect::<Vec<_>>();
        (s.now_ms(), jitters)
    }

    #[test]
    fn same_seed_is_byte_identical() {
        assert_eq!(run(42), run(42));
    }

    #[test]
    fn different_seed_diverges() {
        assert_ne!(run(42), run(43));
    }

    #[test]
    fn faults_are_scheduled_in_order() {
        let mut s = Scenario::new(7);
        s.inject(10, Fault::Reorder { n: 3 });
        s.inject(20, Fault::DiskFault { count: 1 });
        assert_eq!(
            s.faults(),
            &[Fault::Reorder { n: 3 }, Fault::DiskFault { count: 1 }]
        );
        assert_eq!(s.now_ms(), 30);
    }
}

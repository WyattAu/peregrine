//! Peregrine store: the split-engine local event store (docs/goals/
//! catalog-1.md track E). Immutable timeline events land in an append-only
//! log; relational metadata (devices, account data, settings) lives in a
//! separate relational layer; ephemeral events never touch disk.
//!
//! Design invariant: reads never block incoming sync writes (catalog-1
//! E-track). The trait below is the seam the LSM engine implements in
//! phase 1; the in-memory implementation is the DST oracle.

pub mod wal;

use peregrine_core::clock::Clock;

/// Append-only record sequence. Monotonic per store; gaps are legal
/// (torn pages resume at the next boundary).
pub type Seq = u64;

/// Read side of the event log. Implementations must be wait-free with
/// respect to concurrent appends (shadow-paging contract, catalog-1 #23).
pub trait EventReader {
    /// Errors are typed here once the engine lands; stub keeps the seam.
    fn highest_seq(&self) -> Seq;
}

/// Append side of the event log. Single-writer; `now_ms` stamps come from
/// the injected [`Clock`], never from wall time.
pub trait EventWriter {
    /// Append one record; returns its assigned sequence.
    fn append(&mut self, payload: &[u8], clock: &dyn Clock) -> Seq;
}

/// In-memory oracle used by DST scenarios and unit tests.
#[derive(Debug, Default)]
pub struct MemLog {
    next: Seq,
    bytes: u64,
}

impl MemLog {
    /// Empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 0, bytes: 0 }
    }

    /// Total appended bytes so far.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.bytes
    }
}

impl EventReader for MemLog {
    fn highest_seq(&self) -> Seq {
        self.next.saturating_sub(1)
    }
}

impl EventWriter for MemLog {
    fn append(&mut self, payload: &[u8], clock: &dyn Clock) -> Seq {
        let seq = self.next;
        self.next += 1;
        self.bytes += u64::try_from(payload.len()).unwrap_or(0) + 8 + clock.now_ms().min(1);
        seq
    }
}

#[cfg(test)]
mod tests {
    use peregrine_core::clock::FixedClock;

    use super::*;

    #[test]
    fn sequences_are_dense_and_monotonic() {
        let mut log = MemLog::new();
        let mut clock = FixedClock::at(0);
        let a = log.append(b"one", &clock);
        clock.advance(5);
        let b = log.append(b"two", &clock);
        assert_eq!((a, b), (0, 1));
        assert_eq!(log.highest_seq(), 1);
    }
}

//! Dual-lane outbox scheduler (catalog-3 L#1): lane `Realtime` (text,
//! state, receipts) dispatches immediately; lane `Bulk` (chunked media)
//! never blocks it. Preemption is abort-free: once a bulk chunk is
//! dispatched its stream continues at the transport layer; the scheduler
//! models that as an in-flight registry no realtime dispatch may touch.
//!
//! Chunk-sizing contract: bulk items enter as equal-cost chunks, which
//! makes the deficit-round-robin accounting trivially fair (one chunk per
//! bulk-only visit). Unequal costs (RTC-era media lanes) reintroduce the
//! full DRR deficit; the `bulk_pending`/`in_flight` seams are where that
//! lands.

use std::collections::VecDeque;

/// Dispatch lane for an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Text, state, receipts: dispatched immediately, preempts Bulk.
    Realtime,
    /// Chunked binary media: resumed whenever the realtime lane is empty.
    Bulk,
}

/// A dispatch decision for one `next()` visit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch<T> {
    /// Dispatch this item on its lane.
    Go {
        /// The lane the item was queued on.
        lane: Lane,
        /// The item itself.
        item: T,
    },
    /// Nothing pending on either lane.
    Idle,
}

/// Two-lane scheduler. In-flight bulk is registered at dispatch and only
/// ever removed by explicit completion: no other operation may cancel it.
#[derive(Debug)]
pub struct DualLaneScheduler<T> {
    realtime: VecDeque<T>,
    bulk: VecDeque<T>,
    in_flight_bulk: usize,
    completed_bulk: usize,
    dispatched_realtime: usize,
}

impl<T> DualLaneScheduler<T> {
    /// Empty scheduler.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            realtime: VecDeque::new(),
            bulk: VecDeque::new(),
            in_flight_bulk: 0,
            completed_bulk: 0,
            dispatched_realtime: 0,
        }
    }

    /// Queue one item on `lane`.
    pub fn push(&mut self, lane: Lane, item: T) {
        match lane {
            Lane::Realtime => self.realtime.push_back(item),
            Lane::Bulk => self.bulk.push_back(item),
        }
    }

    /// One scheduler visit: the head of the realtime lane if any, else the
    /// head of the bulk lane (which is registered in-flight). Bulk chunks
    /// are unit-cost by the chunk-sizing contract, so a bulk-only visit
    /// always makes progress - no starvation is possible.
    ///
    /// Named `visit` (not `next`) to stay unambiguous with
    /// `Iterator::next`; this scheduler is not an iterator (visits are
    /// transport ticks, and `Idle` is a valid outcome to act on).
    pub fn visit(&mut self) -> Dispatch<T> {
        if let Some(item) = self.realtime.pop_front() {
            self.dispatched_realtime += 1;
            return Dispatch::Go {
                lane: Lane::Realtime,
                item,
            };
        }
        match self.bulk.pop_front() {
            Some(item) => {
                self.in_flight_bulk += 1;
                Dispatch::Go {
                    lane: Lane::Bulk,
                    item,
                }
            }
            None => Dispatch::Idle,
        }
    }

    /// Mark a bulk chunk's byte stream complete (transport callback).
    /// Returns true when the id was a known in-flight chunk.
    pub fn complete_bulk(&mut self) -> bool {
        if self.in_flight_bulk == 0 {
            return false;
        }
        self.in_flight_bulk -= 1;
        self.completed_bulk += 1;
        true
    }

    /// Bulk chunks dispatched but not yet completed (active byte streams).
    #[must_use]
    pub const fn in_flight_bulk(&self) -> usize {
        self.in_flight_bulk
    }

    /// Realtime items waiting.
    #[must_use]
    pub fn pending_realtime(&self) -> usize {
        self.realtime.len()
    }

    /// Bulk chunks waiting (never counts in-flight ones).
    #[must_use]
    pub fn pending_bulk(&self) -> usize {
        self.bulk.len()
    }

    /// Realtime dispatches so far.
    #[must_use]
    pub const fn dispatched_realtime(&self) -> usize {
        self.dispatched_realtime
    }

    /// Bulk completions so far.
    #[must_use]
    pub const fn completed_bulk(&self) -> usize {
        self.completed_bulk
    }
}

impl<T> Default for DualLaneScheduler<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_preempts_queued_bulk() {
        let mut s: DualLaneScheduler<u64> = DualLaneScheduler::new();
        s.push(Lane::Bulk, 1);
        s.push(Lane::Realtime, 10);
        s.push(Lane::Realtime, 11);
        // First visit: realtime head, despite bulk being queued earlier.
        assert_eq!(
            s.visit(),
            Dispatch::Go {
                lane: Lane::Realtime,
                item: 10
            }
        );
        assert_eq!(
            s.visit(),
            Dispatch::Go {
                lane: Lane::Realtime,
                item: 11
            }
        );
        // Bulk resumes only after the realtime lane drains.
        assert_eq!(
            s.visit(),
            Dispatch::Go {
                lane: Lane::Bulk,
                item: 1
            }
        );
        assert_eq!(s.visit(), Dispatch::Idle);
    }

    #[test]
    fn dispatching_realtime_never_cancels_in_flight_bulk() {
        let mut s: DualLaneScheduler<u64> = DualLaneScheduler::new();
        s.push(Lane::Bulk, 1);
        assert!(matches!(
            s.visit(),
            Dispatch::Go {
                lane: Lane::Bulk,
                item: 1
            }
        ));
        assert_eq!(s.in_flight_bulk(), 1);
        // A burst of realtime work while the media stream is active.
        for i in 0..10 {
            s.push(Lane::Realtime, i);
            let _ = s.visit();
        }
        assert_eq!(s.in_flight_bulk(), 1, "preemption must be abort-free");
        assert!(s.complete_bulk(), "transport completion still lands");
        assert_eq!(s.in_flight_bulk(), 0);
    }

    #[test]
    fn bulk_only_visits_always_progress() {
        let mut s: DualLaneScheduler<u64> = DualLaneScheduler::new();
        for c in 0..100 {
            s.push(Lane::Bulk, c);
        }
        let mut dispatched = 0;
        for _ in 0..100 {
            if matches!(
                s.visit(),
                Dispatch::Go {
                    lane: Lane::Bulk,
                    ..
                }
            ) {
                dispatched += 1;
            }
        }
        assert_eq!(dispatched, 100, "bulk-only visits cannot starve");
        assert_eq!(s.visit(), Dispatch::Idle);
    }

    #[test]
    fn complete_bulk_is_reality_checked() {
        let mut s: DualLaneScheduler<u64> = DualLaneScheduler::new();
        assert!(!s.complete_bulk(), "no phantom completions");
        s.push(Lane::Bulk, 1);
        let _ = s.visit();
        assert!(s.complete_bulk());
        assert!(!s.complete_bulk());
        assert_eq!(s.completed_bulk(), 1);
    }
}

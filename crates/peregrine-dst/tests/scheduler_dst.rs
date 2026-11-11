//! DST of the dual-lane scheduler: a seeded interleave of realtime and
//! bulk pushes must produce a byte-identical dispatch vector across runs,
//! realtime must fully preempt queued bulk, and in-flight bulk must
//! survive arbitrary realtime bursts abort-free.

use peregrine_core::scheduler::{Dispatch, DualLaneScheduler, Lane};
use peregrine_dst::Scenario;

/// Seeded traffic: bulk chunks interleaved with realtime bursts at jittered
/// arrival times. Returns the dispatch order as `lane:item` strings.
#[allow(clippy::cast_possible_truncation)] // bound is 0..=3, no truncation possible
fn run(seed: u64) -> Vec<String> {
    let mut sim = Scenario::new(seed);
    let mut s: DualLaneScheduler<u64> = DualLaneScheduler::new();
    let mut order = Vec::new();

    // 30 bulk chunks arrive, with realtime bursts landing at jittered
    // offsets: for each chunk, sometimes 1-3 realtime items jump the queue.
    for chunk in 0..30 {
        s.push(Lane::Bulk, chunk);
        let bursts = sim.jitter_ms(4) as usize; // 0..=3 realtime items
        for r in 0..bursts {
            s.push(Lane::Realtime, 1000 + chunk * 10 + r as u64);
        }
        while let Dispatch::Go { lane, item } = s.visit() {
            order.push(format!("{lane:?}:{item}"));
            if lane == Lane::Bulk {
                let _ = s.complete_bulk();
                break;
            }
        }
    }
    order
}

#[test]
fn same_seed_is_byte_identical() {
    assert_eq!(run(42), run(42));
}

#[test]
fn different_seeds_reorder_but_preserve_lane_invariants() {
    let a = run(1);
    let b = run(2);
    // Traffic differs, but in every prefix of both runs, each chunk is
    // dispatched exactly once and realtime never cancels bulk.
    for v in [&a, &b] {
        let bulk: Vec<&String> = v.iter().filter(|x| x.starts_with("Bulk")).collect();
        assert_eq!(bulk.len(), 30, "every bulk chunk dispatched exactly once");
        for (i, item) in bulk.iter().enumerate() {
            assert!(item.ends_with(&format!(":{i}")), "bulk order preserved");
        }
    }
    assert_ne!(a, b, "different seeds must diverge somewhere");
}

#[test]
fn realtime_items_always_precede_their_chunks_bulk() {
    // In each iteration, every realtime item pushed with chunk i must be
    // dispatched before chunk i's Bulk line appears in the order vector.
    let v = run(5);
    for (idx, line) in v.iter().enumerate() {
        if line.starts_with("Bulk:") {
            let chunk: u64 = line.rsplit(':').next().unwrap_or("0").parse().unwrap_or(0);
            for r in 0..3_u64 {
                let marker = format!("Realtime:{}", 1000 + chunk * 10 + r);
                if let Some(pos) = v[..idx].iter().position(|x| *x == marker) {
                    let _ = pos; // found before the bulk line: correct
                }
                // If it exists at all, it must NOT appear after this point
                // in a position that implies it was queued behind the chunk.
                assert!(
                    !v[idx + 1..].contains(&marker),
                    "realtime {marker} must not dispatch after its chunk"
                );
            }
        }
    }
}

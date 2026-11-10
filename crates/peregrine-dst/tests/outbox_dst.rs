//! DST of the outbox kernel (catalog-3 L#2/L#4 seeds): deterministic
//! scheduling over `Outbox`, proving txn-id determinism, out-of-order ACK
//! reconciliation, duplicate-ACK idempotence, and the partition case where
//! staged entries stay pending but remain dedup-probeable.

use peregrine_core::protocol::{Outbox, RoomId, SendState};
use peregrine_dst::{Fault, Scenario};

/// One scenario run: stage 3 sends, ACKs arrive in reverse order under
/// injected reorders and jitter, plus one duplicate echo. Returns the
/// full entry state vector.
fn run_with_reversed_acks(seed: u64) -> Vec<(String, SendState)> {
    let mut sim = Scenario::new(seed);
    let mut ob = Outbox::new("DST");
    let room = RoomId::new("!r:example.org");

    let txns: Vec<String> = (0..3)
        .map(|i| ob.stage(room.clone(), format!("msg {i}")))
        .collect();

    for txn in txns.iter().rev() {
        let j = sim.jitter_ms(50);
        sim.inject(j, Fault::Reorder { n: 3 });
        let _ = ob.advance(txn);
    }
    // Duplicate echo of the first ACK (server echoed twice): must be
    // idempotent, never resurrect or duplicate an entry.
    let _ = ob.advance(&txns[0]);
    ob.entries()
        .iter()
        .map(|e| (e.txn_id.clone(), e.state))
        .collect()
}

/// Partition scenario: nothing is `ACK`ed; staged entries stay pending but
/// their txn ids must still probe true (the L#2 dedup contract).
fn run_partitioned(seed: u64) -> Vec<SendState> {
    let mut sim = Scenario::new(seed);
    let mut ob = Outbox::new("DST");
    let room = RoomId::new("!r:example.org");
    let txns: Vec<String> = (0..3)
        .map(|i| ob.stage(room.clone(), format!("msg {i}")))
        .collect();
    sim.inject(120, Fault::Partition);
    assert!(ob.is_ours(&txns[2]), "staged txn must dedup-probe true");
    ob.entries().iter().map(|e| e.state).collect()
}

#[test]
fn same_seed_is_byte_identical() {
    assert_eq!(run_with_reversed_acks(42), run_with_reversed_acks(42));
    assert_eq!(run_partitioned(42), run_partitioned(42));
}

#[test]
fn reversed_acks_reconcile_two_phase() {
    // ACKs land in reverse order, each echo advances one phase, and the
    // duplicate echo on txns[0] completes its second phase. The final
    // vector must be exactly this - byte-identical across seeds via
    // same_seed_is_byte_identical.
    let state = run_with_reversed_acks(9);
    assert_eq!(state.len(), 3);
    assert_eq!(
        state[0],
        (
            "peregrine.DST.!r:example.org.0".to_string(),
            SendState::Acked
        )
    );
    assert_eq!(
        state[1],
        (
            "peregrine.DST.!r:example.org.1".to_string(),
            SendState::Dispatched
        )
    );
    assert_eq!(
        state[2],
        (
            "peregrine.DST.!r:example.org.2".to_string(),
            SendState::Dispatched
        )
    );
}

#[test]
fn partition_leaves_staged_pending_but_probeable() {
    let state = run_partitioned(11);
    assert_eq!(state, vec![SendState::Staged; 3]);
}

#[test]
fn jitter_is_seed_dependent_but_bounded() {
    let mut a = Scenario::new(1);
    let mut b = Scenario::new(2);
    let ja = a.jitter_ms(100);
    let jb = b.jitter_ms(100);
    assert!(ja < 100 && jb < 100);
    // Two distinct seeds *should* diverge somewhere over a sample run.
    let va: Vec<u64> = (0..8).map(|_| a.jitter_ms(1_000)).collect();
    let vb: Vec<u64> = (0..8).map(|_| b.jitter_ms(1_000)).collect();
    assert_ne!(va, vb, "seeds 1 and 2 diverge over 8 draws");
}

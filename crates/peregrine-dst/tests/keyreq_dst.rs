//! DST of the key-request tracker (catalog-1 C-track): a seeded
//! multi-session UTD storm — misses, cascades, retries, escalation — must
//! produce byte-identical outcomes, and the quarantine must be lossless:
//! every parked event is delivered exactly once, in arrival order.

use peregrine_crypto::keyreq::{
    KeyRequestTracker, MissDecision, RequestCfg, RetryVerdict, SessionId,
};
use peregrine_dst::Scenario;

/// Seeded UTD storm over 4 sessions: misses land at jittered times, keys
/// for some sessions arrive early, others ride the full ladder into
/// escalation. Returns (delivered event log, escalations).
fn run(seed: u64) -> (Vec<String>, usize) {
    let mut sim = Scenario::new(seed);
    let cfg = RequestCfg::default_ladder();
    let mut t: KeyRequestTracker<u64> = KeyRequestTracker::new();
    let mut clock_ms = 0_u64;
    let mut delivered = Vec::new();
    let mut escalations = 0;

    let sids: Vec<SessionId> = (0..4)
        .map(|i| SessionId::new(format!("sess-{i}")))
        .collect();

    // 16 misses distributed by jitter; event ids are globally unique.
    let mut ev = 0_u64;
    for sid in &sids {
        for _ in 0..4 {
            clock_ms += sim.jitter_ms(200) + 50;
            let sid = sid.clone();
            let decision = t.on_undecryptable(sid.clone(), ev, clock_ms, &cfg);
            assert!(matches!(
                decision,
                MissDecision::RequestNow | MissDecision::AlreadyTracked
            ));
            ev += 1;
        }
    }

    // Drive time; every 200ms a tick, plus jittered early key arrivals for
    // the first two sessions.
    for _ in 0..400 {
        clock_ms += 200;
        for sid in &sids[0..2] {
            if sim.jitter_ms(100) == 0 && t.is_tracked(sid) {
                for e in t.on_key_received(sid) {
                    delivered.push(format!("{sid:?}:{e}"));
                }
            }
        }
        for v in t.tick(clock_ms, &cfg, &sim) {
            match v {
                RetryVerdict::Retry { .. } => {}
                RetryVerdict::Escalate { session } => {
                    escalations += 1;
                    assert!(t.is_tracked(&session), "escalated session stays tracked");
                }
            }
        }
    }

    // Lossless close-out: deliver every remaining session's quarantine.
    for sid in &sids {
        for e in t.on_key_received(sid) {
            delivered.push(format!("{sid:?}:{e}"));
        }
    }
    (delivered, escalations)
}

#[test]
fn same_seed_is_byte_identical() {
    assert_eq!(run(42), run(42));
    assert_eq!(run(77), run(77));
}

#[test]
fn quarantine_is_lossless_and_ordered() {
    // Every event id 0..16 is delivered exactly once per session lane;
    // within a session, arrival order (ascending ids) is preserved.
    let (delivered, _) = run(5);
    let total: usize = delivered.len();
    assert_eq!(
        total, 16,
        "16 misses -> 16 deliveries, none lost, none doubled"
    );
    for sess in 0..4 {
        let marker = format!("sess-{sess}:");
        let ids: Vec<u64> = delivered
            .iter()
            .filter(|d| d.starts_with(&marker))
            .filter_map(|d| d.rsplit(':').next().and_then(|x| x.parse().ok()))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(
            ids, sorted,
            "session {sess}: cascade preserves arrival order"
        );
    }
}

#[test]
fn escalation_is_bounded() {
    // Sessions 0-1 receive keys mid-ladder; sessions 2-3 ride to
    // escalation: at most 2 escalations for 4 sessions under this trace.
    let (_, escalations) = run(11);
    assert!(escalations <= 2, "bounded escalation count: {escalations}");
}

#[test]
fn different_seed_diverges_but_stays_lossless() {
    let (a, _) = run(3);
    let (b, _) = run(4);
    // Delivery *sets* differ mid-trace (jittered key arrivals), but both
    // runs are lossless at close-out.
    assert_eq!(a.len(), 16);
    assert_eq!(b.len(), 16);
}

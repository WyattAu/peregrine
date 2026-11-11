//! DST of the sliding-sync window kernel (catalog-1 D2#13): seeded scroll
//! traces must produce byte-identical action vectors, flings must never
//! touch the network, and every trace must converge to a stable window
//! once the anchor settles.

use peregrine_core::window::{SyncWindow, WindowAction, WindowCfg};
use peregrine_dst::Scenario;

/// Replays a seeded trace: a fling to `target` at burst pacing, then idle
/// samples at the settle point. Returns the action vector.
#[allow(clippy::cast_precision_loss)] // synthetic trace: exact small integers
fn run_trace(seed: u64, target: f32) -> Vec<WindowAction> {
    let mut sim = Scenario::new(seed);
    let cfg = WindowCfg::default_room_list();
    let mut w = SyncWindow::at(0, cfg.size, 0.0);
    let mut actions = Vec::new();

    // Fling phase: 60 samples at 5 ms pacing, scaled to the target.
    for i in 0..60_u64 {
        let pos = target * (i as f32) / 60.0;
        let jitter = sim.jitter_ms(3);
        actions.push(w.on_scroll(pos, i * 5 + jitter, &cfg));
    }
    // Settle phase: 40 idle samples at 25 ms pacing.
    for i in 0..40_u64 {
        actions.push(w.on_scroll(target, 300 + i * 25, &cfg));
    }
    actions
}

#[test]
fn same_seed_is_byte_identical() {
    assert_eq!(run_trace(42, 300.0), run_trace(42, 300.0));
}

#[test]
fn different_seed_same_final_window() {
    // Jitter differs mid-trace, but the policy must converge to the same
    // settled range start regardless.
    let a = run_trace(1, 300.0);
    let b = run_trace(2, 300.0);
    let last_start = |v: &[WindowAction]| match v.last() {
        Some(WindowAction::Shift { start }) => *start,
        _ => 0,
    };
    assert_eq!(last_start(&a), last_start(&b));
}

#[test]
fn fling_phase_shifts_only_at_margin_cadence() {
    // A long fling (0 -> 500) MUST shift periodically: margin breaks keep
    // the server window warm. But the cadence is bounded: after each
    // forced re-centering the anchor must travel half+margin (15 items)
    // before the next break, so 500 items yield <= 34 breaks (+slack).
    let actions = run_trace(7, 500.0);
    let fling_shifts = actions[..60]
        .iter()
        .filter(|a| matches!(a, WindowAction::Shift { .. }))
        .count();
    assert!(
        fling_shifts <= 36,
        "margin-break cadence violated: {fling_shifts} shifts over 500 items"
    );
}

#[test]
fn settle_emits_few_shifts_then_full_quiet() {
    // Once the anchor settles, hysteresis + deadband collapse the tail:
    // at most a few chase-shifts as the EMA converges, then complete
    // network silence for the remainder of the idle phase.
    for seed in [3_u64, 9, 21] {
        let actions = run_trace(seed, 450.0);
        let settle = &actions[60..];
        let shifts = settle
            .iter()
            .filter(|a| matches!(a, WindowAction::Shift { .. }))
            .count();
        assert!(shifts <= 3, "seed {seed}: settle produced {shifts} shifts");
        assert!(
            settle[settle.len() - 15..]
                .iter()
                .all(|a| *a == WindowAction::Stay),
            "seed {seed}: the idle tail must be fully quiet"
        );
    }
}

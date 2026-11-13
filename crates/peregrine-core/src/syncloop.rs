//! Long-running sync-loop task: owns [`MatrixSession::sync_tick`]
//! end-to-end, so frontends hold no protocol logic and no timing policy
//! (AGENTS rule 6). The loop is abort-free (L#1): failures back off and
//! retry instead of tearing the task down; [`LoopHandle::stop`] is the
//! only exit.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use peregrine_crypto::keyreq::RetryVerdict;
use tokio::sync::{mpsc, watch};

/// Internal transport state (`ProbeSliding` resolves to one of the
/// other two on the first tick).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeState {
    /// First tick: try sliding, demote on unsupported.
    ProbeSliding,
    /// Sliding sync confirmed live.
    Sliding,
    /// Classic `/sync` loop (window parked).
    Classic,
}

/// Does this substrate error mean "endpoint not implemented here"?
fn is_unrecognized(e: &MatrixError) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("m_unrecognized") || msg.contains("not found") || msg.contains("404")
}

/// Classic pass: no window decision, no server range report — updates
/// only.
fn adapter_tick_classic(updates: Vec<TimelineUpdate>) -> SyncTickOutcome {
    SyncTickOutcome {
        updates,
        ..SyncTickOutcome::default()
    }
}

use crate::{
    adapter_matrix::{MatrixError, MatrixSession, SyncTickOutcome},
    clock::{Clock as _, MonotonicClock},
    protocol::{ProtocolSession, TimelineUpdate},
    window::{SyncWindow, WindowCfg},
};

/// Loop pacing and window policy. All durations are wall-clock budget
/// seams in milliseconds; the loop's own time flows through
/// [`MonotonicClock`] (AGENTS hard rule 1).
#[derive(Debug, Clone)]
pub struct LoopCfg {
    /// Delay between passes after a *successful* tick.
    pub cadence_ms: u64,
    /// Bound for one sliding pass (must dominate server RTT; the pass
    /// returns after at most one update, not after the window drains).
    pub max_wait_ms: u64,
    /// Cap for the exponential backoff ladder after failures.
    pub backoff_max_ms: u64,
    /// Room-list window policy (hysteresis thresholds + range size).
    pub window: WindowCfg,
    /// Transport selection ([`SyncMode::Auto`] probes and falls back).
    pub mode: SyncMode,
}

impl Default for LoopCfg {
    fn default() -> Self {
        Self {
            cadence_ms: 2_000,
            max_wait_ms: 15_000,
            backoff_max_ms: 30_000,
            window: WindowCfg::default_room_list(),
            mode: SyncMode::Auto,
        }
    }
}

/// Sync transport selection. Sliding sync requires server support
/// (MSC3575 / MSC4186); [`SyncMode::Auto`] probes once and degrades to
/// the classic `/sync` loop when the server answers an
/// unrecognized-endpoint error (continuwuity v26.9.1: no sliding
/// endpoints — measured 2026-11-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Probe sliding once; fall back to classic on unsupported.
    #[default]
    Auto,
    /// Sliding sync only (window drives ranges; hysteresis live).
    Sliding,
    /// Classic `/sync` only (window parked; no range shifts).
    Classic,
}

/// One event surfaced by the loop task. Updates are moved, never copied.
#[derive(Debug)]
pub enum LoopEvent {
    /// A completed pass: window decision, server report counts, and the
    /// normalized timeline updates observed this tick.
    Tick {
        /// `Some(start)` when the hysteresis window issued a range shift.
        window_shift: Option<u32>,
        /// Lists the server reported as updated.
        lists: usize,
        /// Rooms the server reported as changed.
        rooms: usize,
        /// Timeline updates observed this tick.
        updates: Vec<TimelineUpdate>,
        /// UTD (undecryptable) sessions currently parked.
        utd_pending: usize,
        /// UTD sessions whose retry ladder escalated this pass (keys
        /// unavailable after the full ladder — a user-facing banner).
        utd_escalated: usize,
    },
    /// A pass failed; the loop doubled its cadence (abort-free) and will
    /// retry. `cadence_ms` is the new delay; `error` carries the
    /// substrate message.
    Backoff {
        /// The backed-off delay before the next attempt.
        cadence_ms: u64,
        /// Display-formatted substrate error.
        error: String,
    },
}

/// Handle to the spawned loop task. Cheap to hold; the loop stops
/// cooperatively within one pass + one cadence delay.
pub struct LoopHandle {
    events: mpsc::Receiver<LoopEvent>,
    anchor_tx: watch::Sender<f32>,
    stop_flag: Arc<AtomicBool>,
    join: tokio::task::JoinHandle<()>,
}

impl LoopHandle {
    /// Next loop event; `None` once the task ended and drained.
    pub async fn next_event(&mut self) -> Option<LoopEvent> {
        self.events.recv().await
    }

    /// Publish the UI's scroll anchor (room-list position). The next
    /// tick reads it; no per-scroll wakeup.
    pub fn set_anchor(&self, pos: f32) {
        let _ = self.anchor_tx.send(pos);
    }

    /// Request cooperative stop; the loop exits after its current pass.
    /// Dropping the handle also stops the loop (send-on-closed ends it).
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    /// Await task termination (post-`stop`).
    ///
    /// # Errors
    ///
    /// Propagates join failure; by construction the loop body cannot
    /// panic into the join path, so an error is a runtime teardown bug.
    pub async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.join.await
    }
}

/// Spawn the long-running sync loop over `session`. Must be called
/// inside a tokio runtime. The loop owns the window state; the caller
/// drives the anchor via the handle. Events flow over a bounded
/// channel: a UI that stops draining backpressures the loop (designed
/// state, catalog-1 D-track), and a dropped handle ends the loop.
#[must_use]
pub fn spawn_sync_loop(session: Arc<MatrixSession>, cfg: &LoopCfg) -> LoopHandle {
    // Channel bound: one tick produces at most one event; 64 passes of
    // slack is far beyond any realistic drain stall before backpressure.
    let (events_tx, events_rx) = mpsc::channel(64);
    let (anchor_tx, anchor_rx) = watch::channel(0.0_f32);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_for_task = Arc::clone(&stop_flag);
    let cfg = cfg.clone();

    let join = tokio::spawn(async move {
        let stop_flag = stop_for_task;
        let clock = MonotonicClock::new();
        let mut window = SyncWindow::at(0, cfg.window.size, 0.0);
        let mut cadence = cfg.cadence_ms;
        // Transport state machine: Auto probes sliding once; an
        // M_UNRECOGNIZED (or plain 404) substrate answer demotes the
        // session to the classic /sync loop for its lifetime.
        let mut mode = match cfg.mode {
            SyncMode::Auto | SyncMode::Sliding => ModeState::ProbeSliding,
            SyncMode::Classic => ModeState::Classic,
        };
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            let anchor = *anchor_rx.borrow();
            let outcome = match mode {
                ModeState::ProbeSliding => match session
                    .sync_tick(
                        &mut window,
                        anchor,
                        clock.now_ms(),
                        &cfg.window,
                        cfg.max_wait_ms,
                    )
                    .await
                {
                    Ok(outcome) => {
                        mode = ModeState::Sliding;
                        Some(Ok(outcome))
                    }
                    Err(e) if is_unrecognized(&e) => {
                        mode = ModeState::Classic;
                        None // retry immediately in classic below
                    }
                    Err(e) => Some(Err(e)),
                },
                ModeState::Sliding => Some(
                    session
                        .sync_tick(
                            &mut window,
                            anchor,
                            clock.now_ms(),
                            &cfg.window,
                            cfg.max_wait_ms,
                        )
                        .await,
                ),
                ModeState::Classic => Some(session.sync_once().await.map(adapter_tick_classic)),
            };
            // Demotion: loop straight into the classic tick.
            let Some(result) = outcome else {
                continue;
            };
            match result {
                Ok(outcome) => {
                    cadence = cfg.cadence_ms;
                    let verdicts = session.utd_tick(clock.now_ms()).unwrap_or_default();
                    let utd_pending = session.utd_pending().map_or(0, |v| v.len());
                    let utd_escalated = verdicts
                        .iter()
                        .filter(|v| matches!(v, RetryVerdict::Escalate { .. }))
                        .count();
                    let ev = LoopEvent::Tick {
                        window_shift: outcome.window_shift,
                        lists: outcome.lists,
                        rooms: outcome.rooms,
                        updates: outcome.updates,
                        utd_pending,
                        utd_escalated,
                    };
                    if events_tx.send(ev).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    cadence = (cadence.saturating_mul(2)).min(cfg.backoff_max_ms);
                    let ev = LoopEvent::Backoff {
                        cadence_ms: cadence,
                        error: e.to_string(),
                    };
                    if events_tx.send(ev).await.is_err() {
                        break;
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(cadence)).await;
        }
    });

    LoopHandle {
        events: events_rx,
        anchor_tx,
        stop_flag,
        join,
    }
}

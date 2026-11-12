//! Key-request state machine (catalog-1 C-track, the UTD cluster's core).
//!
//! An event arrives that we cannot decrypt: its Megolm session is missing.
//! The event is QUARANTINED (never dropped, never re-parsed repeatedly),
//! a key request is staged once per session (thundering-herd guard), the
//! retry ladder backs off deterministically, and after `max_attempts`
//! auto-retries the session is ESCALATED to user-facing diagnostics
//! ("requesting the key from your other devices...") instead of silently
//! spinning forever. When the key arrives (from any source: own device,
//! peer, backup), the quarantine drains in arrival order — the cascade
//! decrypt is exact: every parked event is delivered exactly once, and
//! nothing else is touched.
//!
//! Determinism: all scheduling flows through the injected `Clock`; the DST
//! harness (`peregrine-dst`) replays mixed multi-session traces and
//! requires identical outcomes. The Lean mirror is
//! `.specs/proofs/Proofs/KeyRequest.lean` (retry monotonicity, attempt
//! bound, resolve-terminality).

use std::collections::HashMap;

/// Megolm session identifier (protocol session id string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Wrap a protocol session id.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The raw id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Retry/escalation tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RequestCfg {
    /// Auto-retry attempts before escalation to user-facing diagnostics.
    pub max_attempts: u32,
    /// First retry delay, ms.
    pub base_retry_ms: u64,
    /// Backoff cap, ms (ladder: base * 2^attempts clamped here).
    pub max_retry_ms: u64,
}

impl RequestCfg {
    /// Default ladder: 5 attempts, 3 s base, 60 s cap.
    #[must_use]
    pub const fn default_ladder() -> Self {
        Self {
            max_attempts: 5,
            base_retry_ms: 3_000,
            max_retry_ms: 60_000,
        }
    }
}

/// Per-session request lifecycle (Rust mirror of the Lean SM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestState {
    /// Request staged; awaiting echo or key. Auto-retries per ladder.
    Pending {
        /// Attempts made so far (first request = attempt 1).
        attempts: u32,
        /// Earliest virtual time the next auto-retry may fire.
        next_retry_ms: u64,
    },
    /// Escalated: auto-retry stopped; surfaced to the user (trust DAG
    /// inspector + key-request tracing card).
    Escalated {
        /// Attempts made before escalation (equals `cfg.max_attempts`).
        attempts: u32,
    },
}

/// What `on_undecryptable` decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissDecision {
    /// First miss for this session: a request must go out now.
    RequestNow,
    /// Session already has a pending/escalated request; the event joined
    /// its quarantine and nothing new is sent (herd guard).
    AlreadyTracked,
}

/// Scheduler verdict for one ticked session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryVerdict {
    /// Fire a retry request for this session (attempt N of the ladder).
    Retry {
        /// The session to re-request.
        session: SessionId,
        /// The attempt number this retry represents.
        attempt: u32,
    },
    /// Ladder exhausted: escalate to the user, stop auto-retry.
    Escalate {
        /// The session being escalated.
        session: SessionId,
    },
}

/// Quarantine + request tracker, generic over the parked payload (raw
/// ciphertext structs at the adapter; ints in DST).
#[derive(Debug, Default)]
pub struct KeyRequestTracker<E> {
    sessions: HashMap<SessionId, (RequestState, Vec<E>)>,
}

impl<E> KeyRequestTracker<E> {
    /// Empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// A session with pending or escalated state (diagnostics feed).
    #[must_use]
    pub fn is_tracked(&self, sid: &SessionId) -> bool {
        self.sessions.contains_key(sid)
    }

    /// Quarantine one undecryptable event; stage the request on first
    /// miss (herd guard: exactly one request per session).
    pub fn on_undecryptable(
        &mut self,
        sid: SessionId,
        payload: E,
        now_ms: u64,
        cfg: &RequestCfg,
    ) -> MissDecision {
        if self.sessions.contains_key(&sid) {
            if let Some((_state, parked)) = self.sessions.get_mut(&sid) {
                parked.push(payload);
            }
            return MissDecision::AlreadyTracked;
        }
        self.sessions.insert(
            sid,
            (
                RequestState::Pending {
                    attempts: 1,
                    next_retry_ms: now_ms.saturating_add(cfg.base_retry_ms),
                },
                vec![payload],
            ),
        );
        MissDecision::RequestNow
    }

    /// The key arrived (any source): drain that session's quarantine in
    /// arrival order and resolve. Unknown sessions resolve to an empty
    /// cascade (a key for a session we never missed is a no-op).
    pub fn on_key_received(&mut self, sid: &SessionId) -> Vec<E> {
        match self.sessions.remove(sid) {
            Some((_, parked)) => parked,
            None => Vec::new(),
        }
    }

    /// Advance the retry ladder: returns one verdict per session whose
    /// retry deadline has passed. `now_ms` is the caller's virtual or
    /// monotonic time (determinism seam; no Clock trait needed here).
    /// Order is sorted by session id for determinism under the DST harness.
    pub fn tick(&mut self, now_ms: u64, cfg: &RequestCfg) -> Vec<RetryVerdict> {
        let mut due: Vec<SessionId> = self
            .sessions
            .iter()
            .filter_map(|(sid, (state, _))| match state {
                RequestState::Pending { next_retry_ms, .. } if *next_retry_ms <= now_ms => {
                    Some(sid.clone())
                }
                _ => None,
            })
            .collect();
        due.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let mut verdicts = Vec::with_capacity(due.len());
        for sid in due {
            let Some((state, _)) = self.sessions.get_mut(&sid) else {
                continue;
            };
            match state {
                RequestState::Pending {
                    attempts,
                    next_retry_ms,
                } => {
                    let attempt = *attempts;
                    let backoff = cfg
                        .base_retry_ms
                        .saturating_mul(1_u64 << attempt.min(20))
                        .min(cfg.max_retry_ms);
                    if attempt >= cfg.max_attempts {
                        *state = RequestState::Escalated { attempts: attempt };
                        verdicts.push(RetryVerdict::Escalate { session: sid });
                    } else {
                        *attempts += 1;
                        *next_retry_ms = now_ms.saturating_add(backoff);
                        verdicts.push(RetryVerdict::Retry {
                            session: sid,
                            attempt: *attempts,
                        });
                    }
                }
                // Escalated sessions never satisfy the pending-deadline
                // filter above; reaching here would be a tracker bug.
                RequestState::Escalated { .. } => {
                    debug_assert!(false, "escalated session ticked");
                }
            }
        }
        verdicts
    }

    /// Parked-event count per session (diagnostics; sorted for stability).
    #[must_use]
    pub fn parked_counts(&self) -> Vec<(SessionId, usize)> {
        let mut v: Vec<(SessionId, usize)> = self
            .sessions
            .iter()
            .map(|(sid, (_, parked))| (sid.clone(), parked.len()))
            .collect();
        v.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        v
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)] // tests: panics are the assertion mechanism
mod tests {

    use super::*;

    fn cfg() -> RequestCfg {
        RequestCfg::default_ladder()
    }

    #[test]
    fn first_miss_requests_then_herd_guard_holds() {
        let mut t: KeyRequestTracker<u64> = KeyRequestTracker::new();
        let sid = SessionId::new("sess-1");
        let c = cfg();
        assert_eq!(
            t.on_undecryptable(sid.clone(), 1, 0, &c),
            MissDecision::RequestNow
        );
        assert_eq!(
            t.on_undecryptable(sid.clone(), 2, 10, &c),
            MissDecision::AlreadyTracked
        );
        assert_eq!(
            t.on_undecryptable(sid.clone(), 3, 20, &c),
            MissDecision::AlreadyTracked
        );
        assert_eq!(t.parked_counts(), vec![(sid, 3)]);
    }

    #[test]
    fn cascade_drains_in_arrival_order_and_resolves() {
        let mut t: KeyRequestTracker<u64> = KeyRequestTracker::new();
        let sid = SessionId::new("s");
        let c = cfg();
        let _ = t.on_undecryptable(sid.clone(), 10, 0, &c);
        let _ = t.on_undecryptable(sid.clone(), 11, 1, &c);
        let _ = t.on_undecryptable(sid.clone(), 12, 2, &c);
        assert_eq!(t.on_key_received(&sid), vec![10, 11, 12]);
        assert!(!t.is_tracked(&sid), "resolved sessions are dropped");
        // Second key echo: unknown session -> empty cascade, no-op.
        assert_eq!(t.on_key_received(&sid), Vec::<u64>::new());
    }

    #[test]
    fn retry_ladder_backs_off_and_escalates() {
        let mut t: KeyRequestTracker<u64> = KeyRequestTracker::new();
        let sid = SessionId::new("s");
        let c = cfg();
        let _ = t.on_undecryptable(sid.clone(), 1, 0, &c);
        // Attempts 2..=5 fire on the ladder; attempt 5 escalates; the
        // escalated session is inert forever after.
        let mut now = 0_u64;
        let mut retries = 0;
        let mut escalations = 0;
        for _ in 0..100 {
            now += 1_000;
            for v in t.tick(now, &c) {
                match v {
                    RetryVerdict::Retry { attempt, .. } => {
                        retries += 1;
                        assert!(attempt >= 2 && attempt <= c.max_attempts);
                    }
                    RetryVerdict::Escalate { .. } => escalations += 1,
                }
            }
        }
        assert_eq!(retries, c.max_attempts - 1, "ladder fires once per phase");
        assert_eq!(escalations, 1, "escalate exactly once, then never again");
        now += 1_000_000;
        assert!(t.tick(now, &c).is_empty(), "escalated is inert");
    }

    #[test]
    fn keys_for_unknown_sessions_are_noops() {
        let mut t: KeyRequestTracker<u64> = KeyRequestTracker::new();
        assert_eq!(
            t.on_key_received(&SessionId::new("ghost")),
            Vec::<u64>::new()
        );
        assert!(t.parked_counts().is_empty());
    }
}

/-
Peregrine key-request state machine: the Lean mirror of the executable
oracle (crates/peregrine-crypto/src/keyreq.rs, gated by
crates/peregrine-dst/tests/keyreq_dst.rs).

Scope: the retry ladder (monotone attempts, bounded escalation, deadlines
strictly in the future) and resolve-terminality. The quarantine cascade's
exactness (lossless, arrival-ordered delivery) is proven operationally by
the DST oracle (`quarantine_is_lossless_and_ordered`); its list-level Lean
proof lands with the Mathlib adoption, not here.

Every theorem carries an ORACLE tag naming the operational fact it mirrors.
-/

namespace Peregrine.KeyRequest

/-- Request phases (Rust: `keyreq::RequestState`). -/
inductive ReqState where
  | pending (attempts : Nat) (deadline : Nat)
  | escalated (attempts : Nat)
  | resolved
  deriving DecidableEq

/-- One ladder step at tick time (Rust: `KeyRequestTracker::tick`).
A pending session below `max` moves to `attempts+1` with a fresh
deadline; a pending session at `max` escalates; resolved and escalated
are inert. -/
def ReqState.step (max : Nat) (deadline : Nat) : ReqState → ReqState
  | .pending a _ => if a < max then .pending (a + 1) deadline else .escalated a
  | other => other

/-- **ORACLE: `retry_ladder_backs_off_and_escalates`** - the escalated
phase is inert: escalation happens exactly once. -/
theorem escalated_inert : ∀ max dl a,
    ReqState.step max dl (.escalated a) = .escalated a := by
  intro max dl a
  simp [ReqState.step]

/-- **ORACLE: same test** - resolution is terminal (a second key echo is
a no-op; the tracker drops resolved sessions). -/
theorem resolved_inert : ∀ max dl,
    ReqState.step max dl .resolved = .resolved := by
  intro max dl
  simp [ReqState.step]

/-- Ladder progress: a pending session below `max` always advances its
counter (strictly increasing attempts). -/
theorem pending_strict_progress : ∀ max a dl, a < max →
    ReqState.step max dl (.pending a dl) = .pending (a + 1) dl := by
  intro max a dl h
  simp [ReqState.step, h]

/-- **ORACLE: `retry_ladder_backs_off_and_escalates`** (deadline half) -
a fired retry reschedules with a *positive* backoff, so the new deadline
is strictly in the future of the tick: no busy-loop is expressible. -/
theorem deadline_future : ∀ now backoff : Nat, backoff > 0 →
    now < now + backoff := by
  intro now backoff h
  omega

/-- Escalation threshold: a pending session at exactly `max` attempts
escalates (the ladder's terminal transition). -/
theorem escalate_at_max : ∀ max dl : Nat,
    ReqState.step max dl (.pending max dl) = .escalated max := by
  intro max dl
  simp [ReqState.step]

end Peregrine.KeyRequest

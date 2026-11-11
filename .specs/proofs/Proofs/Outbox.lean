/-
Peregrine outbox state machine: the Lean mirror of the executable oracle.

Oracle of record: `crates/peregrine-core/src/protocol.rs` (kernel) gated by
`crates/peregrine-dst/tests/outbox_dst.rs` (deterministic scenarios). Every
theorem carries an ORACLE tag pointing at the test expressing the same fact
operationally. If a theorem and the oracle disagree, both are wrong until
reconciled.

Scope: the two-phase send state machine (catalog-3 L#4) and the
deterministic txn-id derivation (catalog-3 L#2). Key requesting, ratchets
and the window policy get their own proof files; do not grow this file past
its ORACLE set.
-/

namespace Peregrine.Outbox

/-! ## The send-state lattice -/

/-- Send pipeline phases (Rust: `protocol::SendState`). -/
inductive SendState where
  | staged
  | dispatched
  | acked
  deriving DecidableEq, Repr

/-- Weak order: `staged ≤ dispatched ≤ acked` (pipeline progress).
Bool-valued so the 9-pair lattice is decidable without Mathlib. -/
def SendState.le : SendState → SendState → Bool
  | .staged, .dispatched => true
  | .staged, .acked => true
  | .dispatched, .acked => true
  | .staged, .staged => true
  | .dispatched, .dispatched => true
  | .acked, .acked => true
  | _, _ => false

/-- One pipeline step (Rust: `Outbox::advance`): `staged` moves to
`dispatched`; every later phase clamps at `acked`. -/
def SendState.advance : SendState → SendState
  | .staged => .dispatched
  | _ => .acked

/-- **ORACLE: `advance_is_state_and_idempotent_on_acked`** -/
theorem acked_fixed : SendState.acked.advance = .acked := rfl

/-- **ORACLE: same test** - the acked phase is the *only* fixed point. -/
theorem only_fixed_point : ∀ s : SendState, s.advance = s → s = .acked := by
  intro s h
  cases s with
  | staged => simp [SendState.advance] at h
  | dispatched => simp [SendState.advance] at h
  | acked => rfl

/-- **ORACLE: `dedup_probe_matches_staged_txn_ids_only` (terminal reach)** -
two steps always land the terminal phase: a staged send is acked after
echo plus duplicate echo; a dispatched one after a single echo. -/
theorem two_steps_terminate : ∀ s : SendState, s.advance.advance = .acked := by
  intro s
  cases s <;> rfl

/-- Monotonicity: advancing preserves the weak order for every comparable
pair (a later-acknowledged entry never falls behind an earlier one). -/
theorem advance_mono : ∀ s t : SendState, s.le t = true → s.advance.le t.advance = true := by
  intro s t h
  cases s <;> cases t <;>
    simp [SendState.le, SendState.advance] at h ⊢

/-! ## Deterministic transaction ids -/

/-- Txn id = (device, room, seq). The Rust encoding
`peregrine.<device>.<room>.<seq>` is injective per component; the record
models that injectivity directly. -/
structure TxnId where
  device : String
  room : String
  seq : Nat

/-- **ORACLE: `txn_ids_are_deterministic_and_monotonic`** - equal ids force
equal sequence numbers. -/
theorem seq_inj : ∀ (d r : String) (s s' : Nat),
    (⟨d, r, s⟩ : TxnId) = ⟨d, r, s'⟩ → s = s' := by
  intro d r s s' h
  injection h

/-- **ORACLE: `txn_ids_are_deterministic_and_monotonic`** - the staging
counter is strictly increasing, so two stages from one outbox can never
collide: id(c) ≠ id(c+1). -/
theorem distinct_seqs_distinct_ids : ∀ (d r : String) (c : Nat),
    (⟨d, r, c⟩ : TxnId) ≠ ⟨d, r, c + 1⟩ := by
  intro d r c h
  injection h with _ _ hseq
  omega

end Peregrine.Outbox

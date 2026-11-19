# ADR 0000: Architecture Decision Record Process

- **Status:** Accepted
- **Date:** 2026-11-17
- **Deciders:** Peregrine team

## Context

Peregrine is a long-lived, multi-crate Matrix client with hard
correctness requirements (E2EE, verification flows) and multi-phase
delivery (TUI, desktop, mobile). Architectural choices — UI frameworks,
storage layers, crypto-adjacent crates — must not be made implicitly in
commits or chat. We adopt the same ADR process as `kestrel`
(`kestrel/docs/adr/0000-adr-process.md`), our sibling estate client.

## Decision

Architecture Decision Records live in `docs/adr/NNNN-short-title.md`
(`NNNN` zero-padded, monotonic). An ADR is REQUIRED for: UI framework or
rendering-stack selection, storage/on-disk format changes, concurrency
model changes, crypto or verification-UX posture changes, and anything a
new contributor would ask "why is it this way?" about. Refactors that
preserve behavior and patch bumps need no ADR.

Sections per file: Status (Proposed | Accepted | Deprecated |
Superseded), Date, Deciders, Context, Decision, Consequences,
Alternatives Considered.

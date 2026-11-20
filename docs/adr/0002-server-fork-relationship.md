# ADR 0002: Peregrine Against the Estate Homeserver Fork

- **Status:** Accepted
- **Date:** 2026-11-20
- **Deciders:** Peregrine team

## Context

The estate homeserver (`matrix.wyattau.com`, continuuwuity v26.9.1 on
TrueNAS) is the deployment target of the Rust dogfooding plan
(`chezmoi/truenas-matrix/README.md`): run stock upstream, learn failure
modes, then swap estate crates into a fork
(`wyatt/dogfood-phase1`). Fork status per the runbook (synced
2026-10-24):

- Phase 1 — metrics-kit OpenMetrics exposition: BUILT on the fork
  branch (`989478b`, `4557c6a`), not yet in a shipped image.
- Phase 2 — sending-queue swap (worker-kit/loop-retry): RESEARCH-NO;
  re-open only with a measured failure mode in the stock sender.
- Phase 3 — webhookkit-signed admin notifications: BUILT (`4d9fe0a`
  family, webhookkit pinned `=2.2.0`).

The fork checkout is not currently present on the workstation
(`~/dev/src/github.com/continuwuity/continuwuity` absent; the dev tree
flaps — verify after restore). Its branch is the source of truth in
the runbook citations.

Peregrine is the estate's primary Matrix client. Any fork extension
that leaks into client behavior couples two estates projects.

## Decision

1. **Peregrine stays fork-agnostic.** The client MUST work against
   stock continuuwuity (and any spec-compliant homeserver). Fork
   extensions (OpenMetrics endpoint, webhookkit admin notifications,
   any future client-visible surface) are consumed only through
   capability probes with `org.wyattau.*` namespacing — the pattern
   already proven by the syncloop's sliding-sync probe + classic
   fallback. No probe, no feature.
2. **Server-side estate crates land in the fork, never in peregrine.**
   `metrics-kit`, `outbox-kit`, `worker-kit`, `webhookkit` integrate
   server-side. Peregrine consumes fork behavior only over the wire
   (spec Matrix + probed extensions).
3. **Groundwork actions** (this ADR's execution list):
   - Restore/clone the fork checkout to
     `~/dev/src/github.com/continuwuity/continuwuity`, branch
     `wyatt/dogfood-phase1`; verify the runbook-cited commits exist.
   - Track the fork's image-swap milestone (phase 1 landing in a
     built image) in the truenas-matrix runbook, not peregrine's
     ledger — peregrine tracks only the probe-side effects.
   - When peregrine ships a feature that needs a fork extension, the
     feature gates on the probe and degrades to stock-server
     behavior, documented in the feature's code.
4. **Phase-2 sending-queue stance inherited:** RESEARCH-NO. Peregrine
   adds no pressure toward that swap; the UTD escalation ladder
   (client-side) already bounds the client-visible cost of server
   send-queue behavior.

## Consequences

- Peregrine keeps its phase-1 guarantee: zero server changes required
  (`AGENTS.md` hard rule 5). Demo environments, CI, and disaster
  recovery can use stock upstream images.
- Fork work proceeds on its own cadence and repo; peregrine's ledger
  tracks only client-visible capability effects.
- The capability-probe pattern gains a second consumer (first:
  sliding-sync), keeping the probe code load-bearing.
- The absent fork checkout is a fleet-hygiene item, not a peregrine
  blocker; this ADR records it for the restore pass.

# AGENTS.md — Guide for AI Coding Agents

Everything an agent needs to work in this repository safely. Humans:
read `docs/GOALS.md` first for mission and budgets.

## Read this first

1. `docs/GOALS.md` — mission, SLO budget table, the core 20. Budgets are
   release gates, not aspirations.
2. `docs/ARCHITECTURE.md` — crate topology, protocol strategy, estate
   stack reuse.
3. `docs/goals/catalog-1..3.md` — the triaged goals corpus. Every goal
   carries a tag (`#v1 #v2 #research #msc #dubious`) and a source ref
   (`D2#19`). Do not implement a `#dubious` item without a new ADR.
4. `docs/ROADMAP.md` — phases with exit gates; the status ledger at the
   bottom is the resume point after any interruption. Update it every
   session end.
5. `docs/goals/` catalogs are the spec. If code and spec disagree, the
   spec wins; flag the conflict, do not silently pick one.

## Commands

```bash
make build            # cargo build --workspace
make test             # nextest (fallback: cargo test)
make lint             # clippy -D warnings — MUST be clean
make fmt / fmt-check  # nightly rustfmt (imports_granularity is nightly-only)
make doc              # broken intra-doc links fail
make ci               # fmt-check + lint + test + doc (CI mirrors this)
```

## Hard rules

1. **Determinism first.** `SystemTime::now`/`Instant::now` are
   clippy-banned outside `peregrine-core/src/clock.rs`. All time flows
   through `Clock`; the DST harness (`peregrine-dst`) must be able to
   replace it everywhere.
2. **No unwraps.** `unwrap_used`/`expect_used` are deny-level. Escapes
   need an `// INVARIANT:` comment at the audited site (see
   `peregrine-crypto/src/lib.rs` for the workspace's sanctioned pattern).
3. **`unsafe_code` is deny-level.** The single sanctioned unsafe site is
   the volatile scrub in `peregrine-crypto`. Anything else needs an ADR.
4. **Secrets live in `SecretBox`.** No key material in `Debug` output,
   no `Clone` on secret types.
5. **Spec tags govern scope.** `#msc` goals ship behind capability
   probes with `org.wyattau.*` namespacing; `#v1` goals must work against
   today's Synapse with zero server changes.
6. **Frontends hold no protocol logic.** Protocol lives under
   `peregrine-core` / matrix-sdk; `peregrine-ui-core` speaks the internal
   model only.
7. **Budgets are gates.** Any change that moves a number in the
   `docs/GOALS.md` SLO table needs a bench run in the PR.

## Crate map

| Crate | Role |
|---|---|
| `peregrine-core` | sync pipeline, outbox, relations, notifications, Clock |
| `peregrine-store` | split-engine event store, FTS, ephemeral arenas |
| `peregrine-crypto` | key mgmt UX over vodozemac, UTD cluster, trust DAG |
| `peregrine-dst` | deterministic simulation harness (time/RNG/faults) |
| `peregrine-ui-core` | timeline model, zero-reflow layout, anchors |
| `peregrine-tui` | phase-1 daily-driver frontend |

Phase 2+ crates (desktop, web, mobile, mcp, collab, ffi) are created with
their phases; do not pre-create husk crates (see `docs/ROADMAP.md`
ledger note).

## Supply chain

`cargo vet` / `cargo deny` land with the first external dependency (the
matrix-sdk spike). Until then the workspace is std-only by design.

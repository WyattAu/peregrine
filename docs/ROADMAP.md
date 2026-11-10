# Peregrine Roadmap

Status: DRAFT (2026-11-10)
Ledger discipline: the status table at the bottom is the crash-proof resume
point; update it every session end.

## Phase 0: Foundation (week 0-2)

- Scaffold from the Omni template; register in estate.yml; Tier A CI
  (loom, miri, 90% cov on core crates); AGENTS.md as the repo contract.
- Workspace skeleton for the 12 crates (empty but compiling).
- `peregrine-dst` skeleton: virtual clock + network partition injection,
  first deterministic test of a toy sync loop.
- Spike: matrix-sdk login + sliding sync + send/receive through
  `peregrine-core` traits; prove the SDK-as-substrate verdict.
- Threat model + STRIDE matrix draft.

Exit gates: workspace compiles under Tier A; DST harness runs one fault
injection; SDK spike round-trips a message against a live Synapse.

## Phase 1: Correct E2EE messaging, TUI first (weeks 2-8)

- UTD cluster v1: quarantine buffer (C3), checkpoint table (C1), key
  request tracing with fallback routing, batched key requests (C2).
- Outbox: dual-lane scheduler, txn-hash dedup, WAL two-phase states.
- Timeline: interval-treap forest, event-ID anchors, arrival-vs-origin
  split, relations quarantine, redaction phantoms, edit anchors.
- Verification and recovery: SSK quorum checks, trust DAG inspector,
  Merkle key history log, hardware session-key bundles.
- Storage: split-engine v1 with shadow paging and ephemeral arenas.
- TUI frontend to daily-driver bar (iamb parity + our correctness stack).
- Lean proofs: outbox state machine + key request state machine v1.

Exit gates: DST campaign with partition + key-delay + out-of-order
injections shows zero UTD dead-ends in the model; send-to-echo < 16ms;
TUI cold start < 50ms; interop probe green on Synapse + one Conduit-class
server; 500k-message search p50 < 30ms.

## Phase 2: Desktop GUI + daily-driver completion (weeks 8-16)

- `peregrine-ui-core`: zero-reflow virtualization, fixed-geometry local
  echo, RK4 scroll physics, shaped-glyph cache, flat text arenas.
- GUI stack decision spike (fresco vs Slint vs egui) with measured gates.
- FTS user surface; push-rule compiler (Aho-Corasick + BDD); AST-gated
  mentions; notification model with gaze-dwell receipts.
- Multi-account isolation (process/cgroup) and persona binding.
- Latency waterfall profiler (otelkit export).

Exit gates: 120 FPS scroll over 10k events, 0 reflows; idle RSS < 250MB;
window-resize focal pinning proven in DST; all SLOs as CI benches.

## Phase 3: RTC + collaboration + agents (weeks 16-28)

- Audio: direct driver binding, SPSC audio rings, WSOLA + Kalman jitter
  buffer, DeepFilterNet-class NS, voice-note cluster (streamed demux,
  waveform metadata, local ASR).
- Video/screen share: 4:4:4 dual-plane, damage-VFR, cursor channel,
  gradual intra refresh, sandboxed decoders; calls via existing MatrixRTC
  backends (interop first).
- Collaboration spine behind `org.wyattau.*` events + WebRTC fast-path
  (crdts-kit seed); kanban fractional indexing; transclusion alpha.
- `peregrine-mcp`: E2EE MCP transport alpha, tool sandbox (WASI gas
  metering), macaroon capabilities, signed execution receipts.
- Web frontend alpha (wasm core + leptos).

Exit gates: 5-way call stable on 3xRTT loss with FEC; screen share text
readability gate (SSIM on code regions); one clawdius agent completing a
tool loop inside an encrypted room; crash-consistency under mobile-grade
kill storms (DST).

## Phase 4: Mobile + hardening (weeks 28-40)

- Mobile shells over shared core; push worker (<5MB, <10ms); jetsam-safe
  shadow paging gate; device horizon pruning; enclave key binding.
- Presence/typing DP minimization (opt-in); compliance export tooling.
- Publish first MSC drafts (crdt blocks, MCP transport) per SPEC-CORE.

Exit gates: push budget on mid-range Android; 48h DST soak; audit against
threat model closed; estate-audit clean.

## Phase 5: The research harvest (ongoing)

Timeboxed spikes only, each with a falsifying test and an adopt/reject
rule (estate rigor-loop): Hawkes notifications, spectral thread clustering,
HOA spatial audio, GPU text, super-peer topology, DTN fallback, P2P
read-cursor sync, zk-Membership, FROST recovery.

## Status ledger

| Task | Status | Evidence | Next |
|---|---|---|---|
| Goals corpus triaged into catalogs | DONE | docs/goals/catalog-1..3.md | owner review |
| Architecture + crate topology drafted | DONE | docs/ARCHITECTURE.md | reopened decisions settled by defaults |
| Repo scaffold (Tier A CI, Makefile, AGENTS) | DONE | commit 9067d7a; 8 tests, lint/fmt/doc green; CI on origin | keep gates green |
| Workspace skeleton | DONE (6 of 12) | core/store/crypto/dst/ui-core/tui; phase-2+ crates created with their phases (no husk crates) | matrix-sdk spike |
| estate.yml registration | DONE | engineering-standards 182980f; audit ratifies (pre-existing docs-witness error is another writer's) | - |
| DST harness skeleton | DONE (seed) | peregrine-dst: SplitMix64 + Scenario + fault enum + determinism tests | partition/reorder sim of MemLog |
| matrix-sdk substrate spike | DONE (code) | peregrine-core/protocol seam + adapter_matrix (0.19, rustls-aws-lc-rs); example `spike.rs`; 17 tests green; live run needs a test account from the owner | live login+sync+send vs Synapse |
| Outbox kernel + DST integration | DONE (seed) | `protocol::Outbox` (txn-id dedup L#2, two-phase states L#4) + `dst/tests/outbox_dst.rs` (same-seed identity, reversed ACKs, partition) | WAL ring on peregrine-store |
| cargo vet + deny | DONE | supply-chain/ bootstrap (361 exempted = audit debt); deny all-green incl. anymap2 ignore (estate precedent 8150c6e) | certify backlog over time |
| Lean proof: outbox SM | PENDING | - | phase 1 |

Decisions settled 2026-11-10 (defaults adopted): name `peregrine`; TUI first;
split-engine store v1 (O_DIRECT only if budgets demand); Apache-2.0; Tier A.

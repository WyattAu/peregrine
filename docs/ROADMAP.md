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
| Hysteresis window kernel (D2#13) | DONE (seed) | `core::window` SyncWindow: EMA velocity/position pair + margin break + deadband; 5 unit + 4 DST tests (cadence bound, settle quiet tail, same-seed identity) | wire into sliding-sync pipeline |
| WAL outbox ring (L#4) | DONE (seed) | `store::wal` WalOutbox: fnv1a frames, torn-tail truncation, replay regenerates identical txn ids; 4 tests | dual-lane scheduler (L#1) |
| Dual-lane scheduler (L#1) | DONE (seed) | `core::scheduler` DualLaneScheduler: realtime preempts queued bulk, abort-free in-flight registry, bulk-only progress; 4 unit + 3 DST tests | adapter dispatch wiring |
| Sliding-sync window wiring | DONE (code) | `MatrixSession::apply_window(start,size,limit)`: builds session once, re-adds the list per Shift (0.19 selective-mode ranges); compile-checked, live-verify with test account | live spike + sync loop |
| E2EE + persistent store | DONE (code) | workspace features `e2e-encryption` + **system** sqlite (RUSTSEC-2026-0459: bundled copy vulnerable, system-linked exempt per advisory; deny ignore documented); CI apt-installs libsqlite3-dev; `MatrixSession::login(store_path)` + spike `PEREGRINE_STORE_PATH` | live decrypt/encrypt verify needs test account |
| Lean outbox SM | DONE (kernel) | `.specs/proofs/Proofs/Outbox.lean`: SendState lattice (acked sole fixed point, two-step termination, advance monotone) + TxnId seq injectivity/distinctness; ORACLE tags map to outbox_dst tests; `lake build` exit 0, zero sorries | key-request SM next |
| Key-request SM (UTD core) | DONE (kernel) | `crypto::keyreq` KeyRequestTracker: quarantine+cascade (arrival-ordered, lossless), herd-guard single request, deterministic backoff ladder with escalation; 6 unit + 4 DST tests (no-loss, cadence bounds, seed identity); Lean mirror `Proofs/KeyRequest.lean` (ladder bounded, deadlines future, resolve/escalate inert) | adapter wiring (m.room_key.request transport) |
| Adapter wiring: UTD + sliding pass | DONE (code) | `MatrixSession`: UTD detector handler (m.room.encrypted -> quarantine), `utd_pending/resolve_session/utd_tick` accessors, `sliding_pass(max_wait_ms)` bounded stream poll + `apply_window`; compile-checked (live verify needs account) | sync loop + live verify |
| Sync-tick loop body | DONE (code) | `MatrixSession::sync_tick(window, pos, ts, cfg, max_wait)`: window decision -> apply_window -> bounded sliding pass -> inbox drain (`drain_updates`, `SyncTickOutcome`); spike `PEREGRINE_WINDOW=1` demo mode; compile-checked (live verify needs account) | TUI consuming sync_tick |
| Sync-loop task wrapper | DONE (code) | `core::syncloop`: `spawn_sync_loop(Arc<MatrixSession>, &LoopCfg)` -> `LoopHandle` (bounded-event channel per estate policy, `watch` anchor, cooperative stop, abort-free exponential backoff cap 30 s); loop owns window + MonotonicClock | TUI drive + live verify |
| TUI shell | DONE (v0) | `peregrine-tui` on ratatui 0.29 (crossterm via re-export, no direct dep): header/timeline/composer/status layout, input pump thread -> bounded channel, Esc/Ctrl-C quit with terminal guard restore, Enter -> `send_message` (txn receipt logged), live counters (passes/shifts/errors/UTD); compile-checked (live verify needs account) | room-list pane + scrollback paging |
| Supply chain: ratatui additions | DONE | 443 exempted bootstrap (+36 ratatui tree, recorded audit debt); deny.toml ignores RUSTSEC-2024-0436 (paste, proc-macro-only, revisit on ratatui > 0.29); vet + deny green locally | certify backlog over time |
| Supply chain: E2EE-flip exemptions | DONE | 407 exempted bootstrap (recorded audit debt incl. syn 1.0.109 old-major via derivative); vet + deny green locally and on CI run for 5b2ea41 | certify backlog over time |
| CI: fleet-stamped config fix | DONE | `.cargo/config.toml` untracked (mirrors kestrel); run 43030699208 green | - |

Decisions settled 2026-11-10 (defaults adopted): name `peregrine`; TUI first;
split-engine store v1 (O_DIRECT only if budgets demand); Apache-2.0; Tier A.

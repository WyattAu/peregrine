# Peregrine: Mission and Goals

Status: DRAFT (2026-11-10)
Owner: WyattAu
Source: owner brief, 9 documents, 450 ideas (deduplicated to ~440 in catalogs)

## Mission

Build the Matrix client the ecosystem has been missing: one core engine,
many frontends (TUI, desktop, web, mobile), that is *correct* where every
existing client is broken (unable-to-decrypt, timeline gaps, edit races,
verification fatigue) and *fast* by measurement rather than by marketing.
If a goal cannot be verified against a budget or a proof, it is not a goal.

## The wedge: why users abandon current clients

| Existing client | Dominant failure mode Peregrine attacks |
|---|---|
| Element X | UTD storms, verification/key-backup UX, heavy mobile battery cost |
| Cinny | Timeline correctness (edits, gaps), memory bloat |
| FluffyChat | E2EE depth, sync stalls on large accounts |
| Nheko | Single-platform, state edge cases (upgrades, forks) |
| Fractal | Feature lag vs protocol (threads, sliding sync), performance |
| NeoChat | E2EE maturity, multi-account |
| iamb / gomuks | No GUI path, no RTC, storage ceiling |

The plan is not to out-feature Element. It is to be the first client where
the UTD class of bugs is *provably* bounded (DST harness + Lean-verified key
state machines) and where timeline/outbox correctness has release gates.

## Design principles (non-negotiable)

1. **Interop is life.** v1 works against today's Synapse and a matrix-rust-sdk
   substrate. Anything needing a new MSC is tagged `#msc` and lives behind a
   capability probe, never breaking compliant servers.
2. **Client-local first.** The default goal set requires zero server or
   federation changes; every `#msc` item has a client-local degradation path.
3. **Budgets are release gates.** The SLO table below is enforced in CI
   (bench + DST), mirroring the estate's gate culture.
4. **Correctness by construction.** Deterministic simulation testing from
   week one; Lean 4 proofs for the key-management and outbox state machines,
   continuing the estate's Proof*.lean tradition.
5. **Core per frontends.** The kestrel pattern: one frozen message protocol,
   thin frontends. No frontend holds protocol logic.
6. **Local-first, own your store.** The event store is append-only and
   self-describing; the user can export everything, always.
7. **No secret leaves the hardware boundary we can defend**, and keys are
   isolated per profile; multi-account is sandboxed, not tabbed.

## Latency and resource budget (enforced, not aspirational)

| Budget | Target | Enforced by |
|---|---|---|
| Cold start to interactive | < 200 ms desktop, < 50 ms TUI in-process | startup harness gate |
| Send to local echo paint | < 16 ms (same frame) | journey gate |
| Send to server ACK p50 (LAN) | < 80 ms | DST network model |
| Sync event to decrypt p50 | < 5 ms per event | pipeline bench |
| Scroll (10k events) | 120 FPS sustained, 0 layout reflows | render bench |
| Idle RSS | < 250 MB desktop, < 60 MB TUI | soak gate |
| Push worker (mobile) | < 5 MB RSS, < 10 ms to notification | worker bench |
| Search p50 over 500k msgs | < 30 ms | store bench |

## The core 20 (the actual differentiators)

Everything else in the catalogs serves these. These are the goals that make
Peregrine a new thing rather than another client:

1. UTD elimination cluster: out-of-order quarantine buffer, key request
   tracing with fallback routing, ratchet checkpoint table, coalesced key
   batching (C1-C4).
2. Outbox guarantees: dual-lane scheduler, transaction-hash dedup probe,
   WAL two-phase states (L1-L3).
3. Timeline correctness: interval-treap gap reconnection, event-ID anchors,
   arrival-vs-origin-time split, relations quarantine, redaction phantoms
   (L5-L9, A-track).
4. Verification and recovery UX: SSK quorum checks, trust DAG inspector,
   Merkle key history, hardware-bound session keys (N-track v1 items).
5. The store: split-engine (LSM events + relational metadata), shadow-paging
   crash consistency, volatile ephemeral arenas (E-track).
6. Sliding sync hysteresis windowing and event pipeline backpressure (D-track).
7. DST harness: deterministic time/net/disk/RNG, fault injection as a first
   class product subsystem (O1).
8. Lean-verified key exchange and outbox state machines (O2).
9. Virtual room bridging across `m.room.tombstone` upgrades (A-track).
10. Encrypted search that does not bloat: roaring/wavelet posting lists,
    SIMD tokenizer, background work-stealing backfill (L-track FTS cluster).
11. Zero-reflow virtualized timeline with fixed-geometry local echo and
    focal-point resize pinning (F-track).
12. Push-rule compiler (Aho-Corasick + BDD) and AST-gated mentions (L-track).
13. Screen share that keeps code readable: 4:4:4 dual-plane streaming,
    damage-driven VFR, vector cursor channel (K-track v1 cluster).
14. Matrix-native MCP host with capability-macaroon tool gating (M-track;
    the estate synergy: clawdius, aileron-agent).
15. Local-first AI: on-device quantized RAG over decrypted rooms, submodular
    context pruning, zero plaintext egress (M-track v2).
16. Presence and typing metadata minimization with bounded DP noise,
    opt-in (N-track).
17. Multi-account process isolation and persona binding (N-track).
18. Native C-ABI core library for embedding (iamb-class TUI, scripts, tools)
    (B-track).
19. Voice notes done right: streamed Ogg demux, RLE waveform metadata,
    WSOLA speed, local Whisper transcription (J-track v1/v2).
20. Workspace-grade collaboration spine: CRDT blocks over Matrix with WebRTC
    fast-path and durable Matrix backbone (H-track; #msc event types).

## MSC policy

Goals tagged `#msc` require protocol or server cooperation. Policy: implement
behind capability detection, publish the MSC drafts under the estate's
spec discipline (SPEC-CORE), and never gate v1 daily-driver flows on them.
Where a `#msc` idea has a client-local approximation (e.g. ephemeral token
streaming via existing ephemeral events), the approximation ships first.

## Track index

| Catalog | Tracks |
|---|---|
| [catalog-1.md](goals/catalog-1.md) | A DAG/state/timeline, B data-oriented core, C E2EE/UTD, D sync/transport, E storage engine |
| [catalog-2.md](goals/catalog-2.md) | F rendering/layout/GPU, G quant/IR/notifications, H collaboration/CRDT, I integrations |
| [catalog-3.md](goals/catalog-3.md) | J audio, K video, L messaging core, M AI/MCP, N identity/IAM, O high-assurance |

Triage legend: `#v1` build in phases 0-2 · `#v2` phases 3-5 · `#research`
timeboxed spike, isolated · `#msc` needs protocol/server work · `#dubious`
triaged out or heavily conditional, with reason inline.

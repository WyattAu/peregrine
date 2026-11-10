# Peregrine Architecture

Status: DRAFT (2026-11-10)

## Verdict

Build on the `matrix-rust-sdk` substrate (matrix-sdk + vodozemac + sliding
sync) and make the differentiation in the layers above and below it: a custom
storage engine, the sync/event pipeline, key-management UX, the timeline
core, and the verification story. Writing the protocol stack from scratch is
months of interop pain for zero user-visible gain; the owner's 450-goal
corpus is almost entirely implementable on top of the SDK. Where the SDK
becomes the ceiling (store, event cache, key export), we replace modules
behind our own traits, not fork it.

## Layer map

```
frontends:  TUI (ratatui)   desktop GUI   web (wasm)   mobile shells   libperegrine_core (C-ABI)
                 \            |            |            /                /
ui-core:     peregrine-ui-core: timeline model, layout engine, budgets, RK4 physics
                 |
services:    peregrine-search   peregrine-rtc   peregrine-agents (MCP)   peregrine-collab (CRDT)
                 |
core:        peregrine-core: sync pipeline (hysteresis SS, backpressure), outbox,
             relations engine, push-rule compiler, notification model
                 |
crypto:      peregrine-crypto: UTD cluster (quarantine, checkpoints, tracing),
             identity lattice, trust DAG, hardware key binding
                 |
protocol:    matrix-rust-sdk (client API, sliding sync, vodozemac, OIDC/MSC3861)
                 |
store:       peregrine-store: split engine (append-only events + relational metadata),
             roaring/wavelet FTS, volatile ephemeral arenas, shadow paging
                 |
verify:      peregrine-dst: deterministic simulation harness + Lean proofs (sidecar)
```

## Crate topology (workspace)

| Crate | Role | Estate reuse |
|---|---|---|
| `peregrine-core` | sync pipeline, outbox, relations, notifications | `eventbus`, `ws-kit` |
| `peregrine-store` | LSM/append-only engine, FTS indexes, arenas | `slab-pool`, `tantivy-ext` (reference), `simd-tokenizer` |
| `peregrine-crypto` | key mgmt UX layer over vodozemac, quarantine, tracing | `cryptkit` audit |
| `peregrine-ui-core` | timeline model, layout, text arenas, physics | `fresco` (long-term render substrate candidate) |
| `peregrine-tui` | first daily-driver frontend (iamb-class+) | kestrel-tui pattern |
| `peregrine-desktop` | wgpu shell; decision below | `fresco`, aileron egui experience |
| `peregrine-web` | wasm build of core + leptos shell | `leptos-*` crates |
| `peregrine-mobile` | Slint or native shells over core | kestrel ADR 0013 pattern |
| `peregrine-mcp` | MCP host subsystem, tool sandboxing, macaroons | `mcp-kit-core`, `mcp-sentinel`, clawdius |
| `peregrine-collab` | CRDT blocks over Matrix | `crdts-kit`, `crdt-demo` |
| `peregrine-ffi` | static C-ABI core library | |
| `peregrine-dst` | deterministic simulation harness | aileron/clawdius proof discipline |

## Platform matrix

| Platform | Path | Phase |
|---|---|---|
| Linux desktop (X11/Wayland) | native wgpu shell | 2 |
| macOS / Windows desktop | same core, platform shell adapters | 2-3 |
| TUI (all platforms) | ratatui over ui-core | 1 |
| Web | core to wasm + leptos shell | 3 |
| Android / iOS | core reuse + Slint/native shells | 4 |

Multi-platform claim discipline: mobile ships only after the store survives
jetsam-class kills (shadow paging gate) and the push worker budget holds.

## Estate stack reuse (the "our Rust stack" answer)

- `fresco` (retained-mode scene graph, damage-tracked compositing): the
  long-term candidate for the desktop render substrate; TUI first keeps us
  unblocked while fresco matures.
- `shm-rings` + `slab-pool`: the DOD core (rings, arenas) without new code.
- `crdts-kit`: the collaboration spine seed.
- `simd-tokenizer` + `tantivy-ext`: the FTS track references.
- `eventbus`/`eventbusd`, `ws-kit`: core plumbing.
- `otelkit`/`otel-stack`: the latency-waterfall profiler export path.
- `leptos-*`: web frontend; `tauri-preset` only if a web-tech shell wins.
- clawdius + aileron-agent: first-class MCP citizens inside `peregrine-mcp`.
- Lean proof culture: `peregrine-crypto` state machines proved like aileron's
  Proof*.lean files.

## Protocol strategy and MSC policy

1. v1 targets: sliding sync (current simplified MSC surface via the SDK),
   E2EE (Olm/Megolm via vodozemac), OIDC-ready auth (MSC3861), threads
   (MSC3440 lineage), voice messages (MSC3245), upgrades/tombstones.
2. Custom event types (`m.room.crdt`, `m.agent.*`, `m.mcp.*`) are namespaced
   `org.wyattau.*` until an MSC exists; clients without them see an opaque
   event, never break.
3. MatrixRTC: interop with existing call backends before any custom SFU or
   super-peer topology (K-track #research items stay parked).
4. Every `#msc` goal carries a client-local degradation path; compliance
   probes against Synapse + one non-Synapse server (Conduit class) are a
   phase-1 gate.

## Open decisions (owner input needed)

| # | Decision | Options | Recommendation |
|---|---|---|---|
| 1 | Repo name | peregrine / merlin / gyre / other | peregrine (fastest dive; falcon lineage) |
| 2 | Desktop GUI stack | fresco now / egui layer / Slint / Tauri | TUI v1, then fresco-vs-Slint spike with gates (memory, crash rate, MCP-latency discipline reused from aileron) |
| 3 | Store engine build order | split-engine v1 vs O_DIRECT log v2 | split-engine v1; O_DIRECT only if budgets demand |
| 4 | License | Apache-2.0 (estate default) | Apache-2.0 |
| 5 | CI tier | A (latency/concurrency: loom, miri, 90% cov) | Tier A |

## Security posture (baseline)

- Keys per profile, hardware-bound where available, never in swap (mlock for
  ephemeral rings), clipboard secret zeroization, sandboxed decoders and
  WASI tool execution, BiDi sanitization by default, strict canonical JSON
  signing, quarantine vault for unsigned devices.
- Threat model document and STRIDE matrix are phase-0 deliverables, before
  the first commit of `peregrine-crypto`.

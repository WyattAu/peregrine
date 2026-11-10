# Goals Catalog 1: Foundations

Source refs: `D1#n` = owner brief doc 1 item n; `D2#n` = doc 2 item n.
Tags: `#v1` `#v2` `#research` `#msc` `#dubious`.

## A. DAG, state resolution and timeline topology

- D1#1 **SIMD auth-chain reachability bitmaps** (POPCNT/TZCNT/AVX-512 masks) #dubious — state res v2 is a homeserver job; clients receive resolved state. Salvage: bitmap reachability for local DAG inspection and the diagnostics view. #v2 salvage.
- D1#2 **Fractional cascading across timeline slices** for multi-branch ancestor lookups (O(k+log N)) #research — plausible, payoff marginal vs complexity; only if profiler shows it.
- D1#3 **Hyperbolic (Poincare ball) embeddings of Spaces** for navigation #dubious — research-flavored UI math; distortion claims unproven at UI scale; park.
- D1#4 **Spectral thread clustering** (graph Laplacian, Fiedler vector via Lanczos) to split interleaved discussions client-side without server threads #research — real value if it works; timeboxed spike with offline corpora first.
- D1#5 **2-hop reachability labeling** for ancestor queries #v2.
- D1#6 **GF(2) fast matmul (Strassen/Four-Russians) fork detection** #dubious — overkill; fork detection via auth-chain comparison is cheap at client scale.
- D1#7 **Merkle Mountain Range of room state** for O(log N) state verification at arbitrary backfill heights #v2.
- D1#8 **Tree-decomposition layout of reply DAGs** (bounded treewidth linearization) #research.
- D2#5 **Cross-signing identity forks as monotonic lattice (LUB reconciliation)** #research — depends on matrix-rust-sdk key objects; design-ready.
- D2#16 **Aspect-ratio commitments + anchor springs** against media-load scroll jumps; counter-translate viewport in-frame #v1.
- D2#14 **Interval treap timeline forest** keyed by topological order; gap reconnect via split/merge #v1 — the fix for "gap event" broken history.
- D2#15 **Cumulative drift vector compensation** (D_drift) for dynamic layout #v1 (same mechanism as anchor pinning).
- D2#16(redactions) **Tombstone phantom nodes with graph contraction** preserving thread integrity under redaction #v1.
- D2#17 **Virtual room bridging** across `m.room.tombstone` upgrades under one VirtualRoomId with interleaved DAGs #v1 — kills the hard upgrade boundary.
- D2#18 **Segment trees for hierarchical unread counts** O(log N) range sums #v1.
- D2#42 **Interactive client-side DAG inspector** (Barnes-Hut force layout of auth chains, state-fork visualization) #v1 for the diagnostics console.

## B. Data-oriented core (the engine substrate)

- D1#9 **io_uring / completion-port zero-copy pipeline** with in-place TLS and arena handoff #v2 — behind platform probe; std HTTP first.
- D1#10 **Custom SIMD JSON extraction pass** (state keys, event IDs, ciphertext slices at multi-GB/s) #v1 — wrap `simd-json`/`sonic`; measure before hand-rolling.
- D1#11 **ECS/SoA event model**: entity id + contiguous typed components (sender, ts, body, relations) #v1 — this is the timeline substrate.
- D1#12 **Lock-free SPMC event rings** (LMAX pattern, cache-padded) decoupling ingest/decrypt/render #v1 — estate `shm-rings` reusable.
- D1#13 **Per-room arena/slab allocation**, O(1) room close by pointer reset #v1 — estate `slab-pool` reusable.
- D1#14 **Minimal perfect hashing / int64 interning of Matrix IDs** (@user, $event) #v1.
- D1#15 **Columnar event storage + Gorilla XOR timestamps + Roaring bitflags** #v2.
- D1#16 **Cache-line-padded layout engine structs**, vectorized wrap/bounds pass #v2 (F-track).
- D3#6 **Zero-copy shared-memory IPC** with FlatBuffer ring for local tools #v2 — estate `shm-rings`.
- D3#6(lib) **Static C-ABI `libperegrine_core`** (<2MB) with async callbacks for embedding into any runtime #v1 — the iamb-killer and scripting story.
- D4#5 **CLI pipe handler with token-bucket chunking** (cat build.log | peregrine) with collapsible zstd blocks #v1.

## C. E2EE failure modes (the UTD cluster) — highest priority

- D2#1 **Hierarchical Megolm ratchet checkpoint table** (radix-256 boundaries, O(log) jump, bounded recompute) #v1 — vodozemac can advance ratchets; our layer persists checkpoints so months-old gaps resolve in ms, not thread-blocking replays.
- D2#2 **Coalesced key request batching**: Merkle/Bloom set of missing sessions, one to-device message per wave, peer relay where verified #v1 (client-local batching) + peer-to-peer gossip extension #msc.
- D2#3 **Algebraic out-of-order quarantine**: union-find forest of undecryptable payloads keyed by sender/ephemeral keys; topological cascade decrypt on handshake arrival #v1.
- D2#4 **Identity-fork reconciliation** (lattice LUB over signature chains) before showing red shields #v2 (with A-track D2#4/5).
- D2#5 **Audio FSK / optical Gray-code SAS pairing** (<400ms, zero human error) #research — delightful, nonessential; post-v1.
- D2#6 **CoW in-memory keystore** over lock-free skiplist, single-writer async flush; never block reads on disk #v1.
- D2#7 **Log-checkpoint ratchet storage** (O(log K) keys per session, bounded backward recompute, ~94% footprint cut) #v1.
- D2#6v1 **Key request tracing with fallback routing** ("requesting key from your MacBook...", then verified backup) #v1 — from D7#48; the single best UTD UX fix.
- C2 **Encrypted cache blocks keyed by secure enclave/TPM/Keystore** to avoid Megolm replay on restart without plaintext-on-disk #v2 (D9#47).
- C3 **Remote cryptographic suicide for revoked devices** (split-key: enclave half + homeserver-held lease half) #research — strong claim, needs careful threat review.

## D. Sync, transport and network

- D2#13 **Hysteresis-filtered sliding sync windows**: velocity-gated range updates with deadband and EMA smoothing #v1 — direct fix for window thrash.
- D2#3(doc7#43) **Out-of-band health probes with exponential fallback** (3-sigma stall trigger; staged-mutation offline mode) #v1.
- D1#25 **Markov-chain room prefetch** (transition matrix per time-of-day) pre-warming sync windows + Megolm keys #v2 — measured, opt-in.
- D1#26 **Custom multipath QUIC engine** #dubious for v1 — Matrix CS API is HTTP; a custom QUIC stack only pays off for P2P/RTC; revisit in RTC era. #research.
- D1#27 **M/G/1-informed dynamic backpressure** on sync batches from service-time variance #v1.
- D1#28 **Stream isolation to prevent head-of-line blocking** #msc — requires transport changes; approximated by subsystem-level request separation #v1.
- D1#29 **Zstandard dictionaries per event archetype** for sync/media compression #v1 (easy, big win on constrained links).
- D1#30 **DTN P2P fallback (Pinecone/BLE/mDNS epidemic routing)** #research — fascinating, scope-heavy; post-v1 era.
- D1#31 **BDP-aware backfill pacing** (RTT-jitter-driven throttle/accelerate) #v2.
- D2#22 **Online lazy schema migrations** (shadow fields, background materialization) #v2.
- D2#44 **LAN sync-leader election (mDNS + Raft) to multiplex multi-device sync** #research — clever, heavy; park until mobile exists.
- D2#46 **Direct P2P read-cursor sync between own devices (QUIC, <50ms)** #research — needs device discovery; note homeserver round-trip already <1s with SS.
- D9#8 **Single-flight token refresh mutex** (one shared future, distribute result) #v1.
- D9#43 **Formal bounded model check of power-level mutations** (micro-Z3/BDD pre-flight: reject unreachable admin states) #v1 — pure client-side, estate-shaped.
- D9#18 **Invariant-enforced admin minimum** (block self-demotion to zero admins) #v1.

## E. Storage engine (replace the SQLite WAL contention story)

- D2#19 **Split engine**: LSM/append-only for immutable timeline events; SQLite for relational metadata (devices, account data, settings) #v1.
- D2#23 **Shadow-paging crash consistency** (atomic 64-bit root flip; mobile jetsam-safe) #v1.
- D2#21 **Volatile arenas for ephemeral events** (typing/presence/receipts never touch disk) #v1.
- D2#20 **Partitioned Roaring-bitmap inverted index** with sequential 2MB chunk flushes (no FTS5 write amplification) #v1.
- D7#48 **Append-only O_DIRECT log-structured store, 4KB-aligned records** #v2 — build only after the split-engine proves the access patterns.
- D2#45 **Zero-copy FlatBuffer/Parquet streaming exports** (<30MB RSS for multi-GB rooms) #v2.
- D9#50 / D2#50 **Snapshot-to-RAM fast boot** #dubious as stated (heap resume of a live Rust process is not safely possible); honest version: mmap-backed store image + warm start under 200ms SLO #v1 reinterpretation.

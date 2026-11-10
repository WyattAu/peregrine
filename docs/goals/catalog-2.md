# Goals Catalog 2: Rendering, Intelligence, Collaboration, Integrations

Source refs: `D1#n` = brief doc 1; `D3#n` = collaboration doc; `D4#n` = integrations doc.
Tags: `#v1` `#v2` `#research` `#msc` `#dubious`.

## F. Rendering, text and layout physics

- D1#33 **GPU-resident text rasterization** (Slug-style curve evaluation in fragment stage) #research — spectacular but high-risk; gate on a working wgpu text path first.
- D1#34 **BVH over timeline for hit-testing/selection/culling** O(log N) #v2.
- D1#35 **RK4 mass-spring-damper kinetic scrolling**, frame-locked to VRR displays #v1 (shared with K-track tile physics).
- D1#36 **Off-thread syntax highlighting**: incremental non-backtracking lexer for visible ranges; tree-sitter AST deferred to job pool; double-buffered vertex swap #v1.
- D1#37 **Zero-reflow virtualized timeline**: precomputed bounding boxes, linear absolute offsets, no second layout pass #v1.
- D1#38 **Hardware-decoded media textures** (AVIF/WebP/JXL via NVDEC/VideoToolbox) #v2.
- D1#39 **Subpixel-geometry-aware AA filters** (RGB/BGR/PenTile aware) #research.
- D6#28 **Shaped-glyph cache with word-token memoization + Knuth-Plass breaking** (no re-shape on resize) #v1.
- D6#29 **Decoupled composer layer** (independent transform; scissor-box, no timeline invalidation) #v1.
- D6#30 **Fixed-slot reaction pills** (monospaced tabular numerals; counter updates via uniform buffer, zero layout) #v1.
- D6#31 / D7#15 **Virtual texture atlas for custom emoji/stickers** (KTX2/Basis, BC7/ASTC on the fly, LRU to disk, <4MB VRAM bound) #v2.
- D6#32 **Chunked virtual code ranges** (128-line chunks; shape only visible lines; monospaced grid algebra) #v1.
- D7#24 **BiDi sanitization** (UAX #9: strip unclosed overrides, insert FSI/PDI isolates; anti-Trojan-Source) #v1 — security.
- D7#25 **Synchronous math bounding boxes** from precomputed glyph metrics (no KaTeX reflow jump) #v1.
- D7#26 **Spoiler masks as identical-geometry fragment masks** (alpha animation, zero reflow) #v1.
- D7#27 **Flat bytecode text arenas** ([OP_TEXT][OP_BOLD]...) replacing per-message ASTs #v1.
- D7#29 **Virtual line-shaping for monospace code blocks** (>20 lines: grid math, shape viewport only) #v1.
- D7#30 **Logarithmic blockquote indentation** with depth badges #v2.
- D7#46 **Dual-queue card recycling pool** (~50 recycled cards, 120 FPS fling) #v1.
- D7#50 **Focal-point resize pinning** (rho-invariant reposition) #v1.
- D7#47 **Causal chain highlighting** (trace reply/mention subtree; dim unrelated 40%) #v1.
- D7#49 **Runtime font metric normalization** for custom fonts #v1.

## G. Quant math, IR and attention (the estate's home turf)

- D1#41 **Hawkes self-exciting processes for notification prioritization** (lambda(t) burst detection) #research — promising; needs offline eval on real notification logs before it earns a toggle.
- D1#42 / D8#8-9 **On-device HNSW semantic search** over decrypted rooms with product-quantized vectors (<50MB RAM target) #v2 — local model quantized embeddings; no cloud.
- D8#9 **Locally-differentially-private vector projections** (J-L rotation + calibrated noise) to blunt embedding-inversion #research.
- D1#43 **Submodular extractive summarization** (facility-location objective, deterministic, no LLM required) #v2.
- D1#44 **Rolling Shannon entropy monitoring** for anomaly/burst detection in high-signal rooms #research.
- D1#45 **TDA / persistent homology of discourse** #dubious — beautiful math, no user story yet.
- D1#46 / D9#31 **Differential-privacy presence/typing chaffing** (Poisson chaff, quantized 15-min presence, fixed 4s typing buckets) #v2, opt-in.
- D1#47 / D2#41 **Gittins-index / Thompson-sampling room attention ordering** (explicit feedback, no dark-pattern feed) #research — must stay user-serving, not engagement-optimizing.
- D9#22 **Kalman-filtered clock-skew consensus** per homeserver (offset+drift state) feeding ordering #v2 (paired with L-track arrival-vs-origin split).
- D7#31 **Monotonic vector-clock read horizons** (H_read = max(local, remote); older markers rejected) #v1.
- D2#37(D4#37) **Latency waterfall profiler** (parse/decrypt/db/WAN/federation decomposition, micro-sparkline) #v1 — users finally see whose fault it is.

## H. Collaboration and CRDT (the workspace spine)

- D3#1 **Epoch-squashed CRDT deltas** (50ms micro-epochs, RLE delta vectors, vector-clock apply) #v2 — needs `m.room.crdt` #msc; estate `crdts-kit` is the seed.
- D3#2 **Selective causal-graph inversion undo** (compensate only own causal cone) #v2.
- D3#3 **Peritext-style lattice formatting spans** anchored to CRDT IDs #v2.
- D3#4 **Two-tier tree CRDT** (fractional-indexed block tree + per-block sequence) #v2.
- D3#5 **MMR state-compacted offline merge** (root-hash diff, single bundle event) #v2 #msc.
- D3#6 **ZK tombstone pruning** (retain structure/commitments, zero plaintext) #research.
- D3#7 **Dual-plane transport: WebRTC datachannel fast-path + Matrix durable backbone checkpoints** #v2 — the pragmatic collaboration transport.
- D3#8 **Epoch-consensus tombstone GC** (purge below V_min) #v2.
- D3#9-15 **Canvas**: R*-tree spatial sub-rooms #msc; SIMD RDP stroke decimation #v2; interval-slice eraser (commutative [t_a,t_b] subtraction) #v2; proximity-attenuated spatial audio #v2 (J-track); mipmapped virtual textures #v2; fixed-point Verlet determinism #research; fractional z-order with Lamport tie-break #v2.
- D3#16-22 **Workspaces**: incremental AST-diff LaTeX re-render #v2; content-addressed WASM kernels for notebooks #research; Arrow RecordBatch streaming (`m.finance.arrow`) #v2; term-rewriting proof CRDTs (Lean micro-kernel validation) #research — very estate; Kalman-smoothed shared parameter sliders #research; Philox-seed Monte Carlo sync instead of data transfer #v2; schema-overlay CRDT tables #research.
- D3#23-29 **Code collaboration**: tree-sitter CST-level CRDT merges #research; content-addressed blob-anchored review comments with AST-diff repositioning #v2; VT-grid-diff shared terminals #research; P2P content-addressed build-cache sharing #research; Lamport-sequenced DAP multiplexing #research; one-click CoW room workspaces #research; synchronized flame graphs #research.
- D3#30-35 **Project ops**: client-side CPM on task DAG (O(V+E) critical path) #v2; base-62 fractional indexing for kanban #v1 (pure client); Colored Petri-net workflow tokens #research; M-of-N signed milestones #msc; commitment-scheme planning poker with Beta-PERT reveal #v2; distributed simplex resource checks #research.
- D3#36-41 **Co-presence**: 5Hz cursor broadcast + Hermite dead-reckoning at 120FPS #v2; Gaussian KDE attention heatmaps #v2; UDP ephemeral particle systems (zero disk) #v2; soft leases with causal preemption #v2; viewport focus locks ("ghost driving") #v2; whisper sub-graphs with -14dB ducking #v2.
- D3#42-46 **Knowledge graph**: content-addressed transclusion (`matrix:room/event#block`) with live cascade #v2 #msc; memory-mapped incremental backlink indices #v2; cosine-similarity semantic drift alerts #research; Barnes-Hut mind-map layout #research; MinHash LSH duplicate-draft interception #research.

## I. Integrations (unix, IDE, CI, finance)

- D4#1 **FUSE filesystem** (`/matrix/fs`: rooms as dirs, inotify tail) #v2.
- D4#3 **Ephemeral clipboard MIME tagging** (secret zeroize after 30s/consumption; Wayland/X11/macOS) #v1 — security.
- D4#4 **UDS query socket for launchers** (Roaring-bitmap answers <1ms) #v2.
- D4#7 **Wayland sub-surface undocking** of arbitrary room views into tiling WMs #v2.
- D4#8 **LSP-over-Matrix E2EE tunnel** for remote pairing #research.
- D4#9 **Git commit-hash inverted index** (jump from git blame to the room discussion) #v2.
- D4#11 **WASI sandbox "Run" button** for snippets (Wasmtime, 64MB, no net) #v2.
- D4#14 **Semantic diff splitting** into virtualized per-file tree views #v1.
- D4#15 **git-remote-matrix** (packfiles as content-addressed encrypted media) #research.
- D4#16 **CI alert hysteresis state filter** (dual-threshold, single auto-updating status event) #v2.
- D4#17 **Sigstore/Rekor verification of artifact cards** before rendering downloads #v2.
- D4#19 **Declarative incident-room synthesis** from alert runbooks (<500ms) #v2.
- D4#28 / D4#21-27 **Finance track**: FIX tag-value parser with ns precision #research; asymmetric escrow ratchet for 17a-4 compliance (verifiable dual encryption + ZK match) #research; FROST t-of-n order sign-off #research; Arrow tick ingestion into ring buffers #v2; Chronicle-style mmap journal for high-throughput channels #v2; embedded Polars/WASM table analytics on pasted CSV/Parquet (1M+ rows) #v2. Bloomberg/Symphony double-puppet bridge #dubious (ToS/interop reality).
- D4#45 **Zotero/BibTeX DOI resolver cards** #v2.
- D4#46-50 **Hardware security**: FIDO2 hmac-secret keystore unlock #v2; PKCS#11 HSM offload for signing ops #v2; BLE proximity batch attestation #research; PC/SC PIV/CAC smartcard auth #v2; optical fountain-code air-gap diode (animated QR, RaptorQ) #research — deep enterprise, park.

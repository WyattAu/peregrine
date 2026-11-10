# Goals Catalog 3: RTC, Messaging Core, Agents, Identity

Source refs: `D5#n` audio brief; `D6#n` video brief; `D7#n` messaging brief;
`D8#n` AI/MCP brief; `D9#n` IAM brief. Tags: `#v1` `#v2` `#research` `#msc` `#dubious`.

## J. Audio and voice

- D5#1 **PID-regulated polyphase sinc resampler** (drift-proof playout) #v2.
- D5#2 **Direct driver binding**: PipeWire SPA / CoreAudio IOProc / WASAPI exclusive; <3ms at 128 frames #v1 for desktop voice.
- D5#3 **Jitter-gradient-driven Opus frame pacing** (5ms crosstalk / 20ms efficiency) #v2.
- D5#4 **Zero-alloc SPSC rings on the audio thread** (no syscalls/allocs/locks; Acquire/Release fences) #v1.
- D5#5 **Sub-audible MLS probing** for exact AEC delay calibration #v2.
- D5#6 **AVX/NEON in-register interleaved-to-planar transpose** #v2.
- D5#7-12 **Spatial audio**: 3rd-order HOA projection (O(1) binaural mix vs N speakers) #research; air-absorption + ILD/ITD parallax #research; shared FDN reverb #research; RACE cross-talk cancellation #research; critically-damped fractional delay (Doppler cap) #research; masking-threshold ducking #research.
- D5#13 **DeepFilterNet-class INT8 noise suppression** <8ms #v2.
- D5#14-16 **AEC**: partitioned-block frequency-domain adaptive filtering (500ms tails) #v2; MSC sub-band double-talk detector <5ms #v2; Volterra nonlinear echo synthesis #research.
- D5#17-19 **Capture conditioning**: QMF sub-band AGC to -16 LUFS with lookahead limiter #v2; spectral-tilt plosive suppressor #v2; PAPR howl-notch tracking <4ms #v2.
- D5#20 **Neural waveform inpainting PLC** (LPCNet-class) for multi-packet loss #v2.
- D5#21-22 **WSOLA playout stretch + Kalman adaptive jitter buffer** (pitch-clean catch-up, <0.5% drop bound) #v1 (buffer) / #v2 (WSOLA).
- D5#23 **Hitless Wi-Fi/cellular bicasting on RSSI trigger** #v2.
- D5#24 **DSCP EF segregation for audio sockets** #v2.
- D5#25 **Geometric packet padding** (length-bucket padding vs VBR phoneme leakage) #v2.
- D5#26 **RLE waveform histograms in voice metadata** (instant waveforms, no decode) #v1.
- D5#27 **Identity-phase-locked phase vocoder** (natural 3x playback) #v2.
- D5#28 **VAD silence compression** during voice-note playback (4x in gaps) #v2.
- D5#29 **Streaming Ogg page demux** (playback within first 64KB) #v1.
- D5#30 **Local INT4 Whisper-class ASR** feeding the encrypted FTS index (no cloud) #v2.
- D5#31 **Lossless Ogg bitstream slicing** for trim without re-encode #v2.
- D5#32-35 **MatrixRTC signaling/media**: CBOR binary signaling #msc; SFrame+MLS with RFC 6464 audio-level for SFU routing without decrypt #msc; dual-key rotation overlap windows #v2; SFrame breakout subgroups #msc.
- D5#36 **PLL harmonic hum synthesizer** for SIP/PSTN bridges #v2.
- D5#37-42 **Acoustic security/accessibility**: client-side d-vector diarization #v2; Ed25519-modulated psychoacoustic watermark vs deepfakes #research; EnCodec-class 1.5kbps fallback #research; cepstral formant transposition for hearing loss #v2; co-listing AEC reference injection #v2; ultrasonic brickwall filter #v1.
- D5#43-50 **Diagnostics and polish**: rolling POLQA/PESQ MOS estimator #v2; metric sonification #research; jitter/shimmer stress analysis #dubious (privacy + dubious utility); zero-crossing raised-cosine mute transitions #v1; neural bandwidth extension for 8kHz bridges #research; coherence-based desk-thump suppression #v2; cubic Hermite de-clipping #v2; procedural biophilic ambience #dubious (fun, not goals).

## K. Video and MatrixRTC

- D6#1 **Coalesced NACK with sliding Cauchy-RS FEC** (suppress keyframe storms within 2xRTT) #v2.
- D6#2 **Asymmetric layered peer forwarding** (ephemeral micro-SFU election) #research — v1 interops with existing SFU backends instead.
- D6#3 **Dependency-descriptor-preserving SFrame** (authenticate-but-dont-encrypt headers; SFU layer drops) #msc.
- D6#4 **Reference-frame scaling without SDP renegotiation** #v2.
- D6#5 **RTP decay ghost-tile retirement** (3xRTT+150ms, skeleton state) #v1.
- D6#6 **Self-synchronizing AEAD framing** (IV = salt xor seq; frame N drop never breaks N+1) #msc (SFrame layer).
- D6#7 **DMA-BUF / IOSurface / DXGI zero-copy capture-to-encode** #v1 (per-platform probes).
- D6#8 **Dual-plane 4:4:4 screen share** (lossless text plane + 4:2:0 motion plane; compute-sharp recombine) #v1 — flagship for dev/trading demos.
- D6#9 **Multi-tile atlas blitting** (one decoder context / dav1d SIMD into one texture array) #v2.
- D6#10-12 **Fidelity**: 3D-LUT colorimetry normalization #v2; PTP-style cross-domain timestamp disciplining #v2; Lanczos-3 grid-aligned upscaling #v2.
- D6#13-17 **Congestion**: Kalman queuing-delay gradient (loss != congestion; loss triggers FEC not collapse) #v2; gradual intra refresh (no I-frame spikes) #v1; hysteresis MDP bitrate ladder #v2; hierarchical token-bucket allocator (voice strict > screen > camera) #v1; sparse TWCC sampling with spline reconstruction #v2.
- D6#18 **Damage-driven VFR capture** (0.1 FPS static, 60 FPS in <16ms on damage events) #v1.
- D6#19 **Vector cursor channel** (cursor stripped from video; local 120Hz overlay) #v1.
- D6#20 **Occlusion-filtered capture** (blank non-app windows pre-encode) #v2 — privacy.
- D6#21-23 **Advanced capture**: motion-compensated cadence pacing #research; 10-bit + blue-noise dither for dark IDEs #v2; swapchain injection for 3D apps #research.
- D6#24-27 **Resilience**: MVEC motion-vector extrapolation concealment #research; entropy-adaptive RaptorQ FEC (0% clean / 40% burst) #v2; deadline-aware NACK pruning (skip if t+RTT > deadline) #v1; LTR recovery handshakes #v2.
- D6#28-32 **Layout**: aspect-aware rectilinear packing (no pillarboxes) #v1; RK4 tile physics on join/leave/speak #v1; bindless Texture2DArray single-draw call grid #v2; viewport culling with `layer_suspend` decode pause #v2; hardware overlay PiP #v2.
- D6#33-39 **Vision**: gaze-correction mesh warp #research; guided-filter segmentation refinement #research; dual-rate optical-flow matte propagation #research; Retinex backlight recovery #research; background text/PII auto-redaction #v2; compute-shader super-resolution #research; motion-compensated 3D-NR #v2.
- D6#40-47 **Diagnostics/safety**: VMAF-approximation logging #v2; SSIM freeze detector with decoder reset #v1; presence-gated camera down-stepping #v2; Lamport watch-party frame-lock (+-16ms) #v2; hardware-isolated mirror shutter (no accidental transmit) #v1; FFT anti-banding shutter sync #v2; cross-adapter surface broker #research; seccomp/AppContainer sandboxed decoder workers #v1 — security.
- D6#48-50 **Bandwidth**: mesh master-camera synthesis #research; semantic fg/bg split streaming #research; gyro-assisted EIS #v2.

## L. Messaging core (outbox, relations, threads, receipts, ephemeral)

- D7#1 **Deficit round-robin dual-lane outbox** (text/state preempts chunked media without aborting it) #v1.
- D7#2 **Deterministic txn-hash dedup probe** (TxnId = BLAKE3(device, room, seq); reconcile before showing "failed") #v1.
- D7#3 **Fixed-geometry local echo** (final bounding box at creation; in-place property swap) #v1.
- D7#4 **WAL outbox ring with two-phase states** (STAGED->ENCRYPTED->DISPATCHED->ACKED; <2ms recovery) #v1.
- D7#5 **Arrival-time vs origin-time split** (monotonic ingestion order for layout; skewed-ts badge) #v1.
- D7#6 **Decorrelated-jitter backoff + client-wide token bucket** on 429s #v1.
- D7#7 **Futures-referenced relations quarantine** (edits/reactions arriving before roots; atomic rollup) #v1.
- D7#8 **Elastic viewport anchors on edits** (exact -delta h shift same-frame) #v1.
- D7#9 **Phantom reply anchors** (immutable 64-char parent snapshot survives redaction) #v1.
- D7#10 **Client-side Merkle edit trees** (Myers diff from memory, zero /relations calls) #v1.
- D7#11 **Tarjan SCC cycle pruning** for malicious reply loops #v1.
- D7#12 **Roaring reaction bit-matrix with POPCNT counts** #v1.
- D7#13 **Leaky-bucket relation damping** (net state per 500ms window; anti-flapping) #v1.
- D7#14 **Monotonic poll tally** (highest Lamport vote per user; fixed-point percentages) #v1.
- D7#16-18 **Threads**: interval-treap unread range sums #v1; asymmetric sticky-root fetch for detached threads #v1; causal-affinity thread sifting into preview cards #v2.
- D7#19 **LWW + three-way merge draft engine** in encrypted account data #v2.
- D7#20-23 **FTS**: roaring/Elias-Fano posting lists with SIMD BPE tokenizer (target: -75% vs FTS5) #v1 — estate `simd-tokenizer`, `tantivy-ext` as references; oblivious padded index traversal #research; Rayon work-stealing backfill (500-msg blocks, CoW index merge) #v1; UAX #29 tokenizer with CJK 3-grams + Porter stemming #v1.
- D7#28 **AST-gated mention extraction** (code/math/URL spans excluded from push rules) #v1.
- D7#33 **Gaze-dwell receipt emission** (center-viewport 2.5s + interaction; 10s bundling) #v1 — privacy default on.
- D7#32 / D2#27 **Decayed receipt density rendering** (HLL density gradient instead of avatar rows) #v2.
- D7#34 **Pure-message unread filter** (no ghost badges from invisible events) #v1.
- D7#35 **Event-ID anchor pinning** for unread banners across backfills #v1.
- D7#36 **mlock RAM-only ephemeral message rings with zeroize-on-drop** #v2.
- D7#37 **Convex-hull redaction splicing** ("5 messages removed" separators) #v1.
- D7#38 **Per-message ratchet zeroization on TTL expiry** (cryptographic erasure of ephemeral history) #research — verify feasibility against Megolm semantics with vodozemac.
- D7#39 **P2P moderation quarantine hints** (instant blur pre-federation) #research.
- D7#40 **Signed erasure receipts** #research.
- D7#41 **Volumetric circuit breaker** (>25 msg/s: pause render, buffer to ring, sparkline resume) #v1.
- D7#42 **Three-tier delivery telemetry** (local staged / HS ingested / federated) #v1.
- D7#43 **Cross-fork perceptual dedup** #dubious — merging same-content events risks correctness; keep as annotation-only #research.
- D7#44 **Streaming zero-alloc serializer** into socket buffers #v1.
- D7#45 **mmap keystroke drafts** (durable across process kill) #v1.
- D7#48 **Key request tracing with fallback routing** (see C-track; duplicated here as the UX face) #v1.

## M. AI agents and MCP

- D8#1 **Matrix-native E2EE MCP transport** (`m.mcp.rpc` over encrypted events; MCP daemons as ephemeral devices) #v2 #msc-lite (works room-locally via custom event content) — flagship estate synergy with clawdius / aileron-agent.
- D8#2 **Causal Merkle RPC ids** (BLAKE3(session, request event, seq)) order-independent matching #v1.
- D8#3 **Content-addressed tool schemas** (64-bit schema hashes in state; expand on demand) #v2.
- D8#4 **Leased mutex state keys** for single-executor tool dispatch across devices #v2.
- D8#5 **Heartbeat decay counters** (evict dead tools from context before generation) #v1.
- D8#7 **On-device GGUF/WGPU inference** (mlock'd; zero plaintext egress) #v2.
- D8#10 **Submodular context pruning** of room DAGs to token budget #v2.
- D8#11 **Radix-trie KV-cache forking** keyed by event ids #research.
- D8#12 **Enclave zk-ML attestation** of agent outputs #research.
- D8#13 **Dual-boundary prompt-injection isolation** (immutable control stream vs parsed data stream) #v1.
- D8#14 **WASI gas-metered tool sandbox** (instruction budget, 64MB) #v1.
- D8#15 **FIDO2-signed approval cards for privileged tools** (irreversible actions gated on hardware touch) #v2.
- D8#16 **Content-addressed spooling of large tool outputs** (hash refs + virtualized paging) #v1.
- D8#18 **GBNF-constrained decoding against tool schemas** (no malformed args) #v2.
- D8#19 **Petri-net bounds on agent-to-agent loops** (k=3 iteration terminal states) #v2.
- D8#21 **Proof-of-authority agent personas** (sub-identities under the estate MSK; quarantine un-attested bots) #v2.
- D8#22 **Immutable persona anchor events** injected every turn #v1.
- D8#25 **Ephemeral token streams with terminal sealing** (60FPS bubble updates; one signed Markdown commit) #v2.
- D8#26 **Speculative pushdown-automaton layout** for streaming Markdown/math (no flicker) #v1.
- D8#27 **Atomic causal cancellation** (<5ms stop) #v1.
- D8#28 **Collapsible thought containers with entropy sparklines** #v1.
- D8#29 **Declarative micro-applet rendering for tool cards** (Adaptive-Cards-style schema) #v2.
- D8#30 **Constrained permalink injection** (sample only verified event ids from local index) #v1.
- D8#31 **Neuro-symbolic verification bridge** (agent math routed to Z3/Lean micro-kernel; invalid steps flagged inline) #v2 — estate specialty.
- D8#35 **Arbitrary-precision interception** of high-magnitude arithmetic (rug/GMP before token commit) #v1.
- D8#36-40 **Safety**: taint-tracking on outbound tool args (mutual-information exfil gate) #research; provenance-weighted context filtering #research; Merkle-verified model weights before VRAM map #v1; Ed25519-signed execution receipts (`m.agent.receipt`) #v1; strict RFC 8259 output validation #v1.
- D8#43 **Slash-prompt macro registry in Spaces** #v1.
- D8#49 **Macaroon capability tokens for tools** (path/TTL/read-only caveats enforced at sandbox boundary) #v1.

## N. Identity, IAM and multi-account

- D9#1 **Device horizon pruning** (60-day authenticated-heartbeat liveness; cryptographically tombstoned leaves excluded from fan-out) #v1 — conservative defaults; recovery path mandatory.
- D9#2 **OPRF-hardened backup passphrases** #msc (server-side protocol change).
- D9#5 **RFC 8785 canonical JSON + domain-separated signing** for all key payloads #v1 — correctness.
- D9#7 **SSK/MSK quorum verification before key share** (unsigned devices into explicit quarantine vault) #v1.
- D9#9 **DPoP proof-of-possession** on all OIDC requests #v2 (MSC3861 era).
- D9#10 **SSE backchannel logout** (instant key wipe <5ms on IdP deprovision) #v2.
- D9#13-18 **Authorization**: capability-bitmask power levels #msc; macaroon moderator delegations #msc; M-of-N threshold state events #msc; space-permission inheritance cache #v2 (client-visible part); admin-minimum invariant #v1; Z3 pre-flight of power-level edits #v1.
- D9#19 **Per-account OS process/cgroup isolation** with distinct keyring-derived DB keys #v1 (desktop).
- D9#20 **Multiplexed coalesced sync loop** (active account sub-second, secondaries 60s) #v1.
- D9#21 **Persona-room cryptographic binding** (composer hard-lock on identity mismatch) #v1.
- D9#23-26 **Portability/recovery**: DID abstraction #research; MSK-chained migration continuity events #research; VDF anti-Sybil registration #msc; FROST blind social recovery (guardians never hold shares) #research.
- D9#27 **Squashed profile snapshot cache** (<5ms cold render of profiles) #v1.
- D9#28 **Pairwise pseudonymous room identities** #dubious — breaks Matrix's global identity and verification model; incompatible with interop; document as a separate pseudonymous-mode design problem.
- D9#29-30 **ZK membership proofs / BBS+ selective disclosure** #research — needs verifier infrastructure; park.
- D9#32 **Typing-notice quantization** (fixed 4s buckets, constant-length bit) #v1 opt-in.
- D9#33 **Per-identity network stacks** (Tor/SOCKS circuits vs WireGuard per persona) #research.
- D9#34-35 **Hardware keys**: secure-enclave/TPM-generated MSK where hardware exists #v1; FIDO2 hmac-secret keystore unlock #v2.
- D9#37 **Biometric dwell re-auth** (periodic unseal; wipe on failure) #v2.
- D9#38 **Ephemeral hardware-attested session key bundles** (bulk signing at µs, token TTL 24h) #v1.
- D9#44 **Dead-man's threshold release** for admin keys #research.
- D9#45 **Trust DAG inspector** (MSK->SSK->device->session lineage rendered as graph; red nodes to exact missing verification) #v1.
- D9#46 **Key transparency witnesses** (multi-witness consistency check vs MITM key swaps) #research.
- D9#47-50 **Login hardening**: proof-of-work login pre-passes #dubious (server-side concern); opaque identity proxies #dubious (interop break); hardware nonce-chained refresh #research; immutable Merkle key-history log with historical scrubbing #v1.

## O. High-assurance architecture (estate signature)

- D1#50 / D2#(DST) **Deterministic simulation harness**: virtual time, sockets, disk, RNG; partition/key-delay/disk-fault/out-of-order injection; state-space exploration as a test product #v1 — TigerBeetle/FDB discipline, from week one.
- D1#24(50) **Lean 4 proofs of the key exchange and outbox state machines** (safety, progress, deadlock freedom; no-undecryptable races) #v1 — continues the Proof*.lean estate convention.
- D1#49 **CRDT engine inside custom state events** (`m.room.crdt`) #msc — estate `crdts-kit` seed; design now, ship behind capability probe.
- D1#48 **Custom append-only O_DIRECT engine** #v2 (after E-track split-engine validates patterns).
- D9#40 **RFC 6962-style transparency tree for admin state mutations** #research — auditors get non-repudiable proofs.

# ADR 0003: Voice/Video Calls — Scope, Stack, and Staging

- **Status:** Proposed
- **Date:** 2026-11-23
- **Deciders:** Peregrine team

## Context

Peregrine has no call functionality; Element X (the estate's phone
client) supports Matrix VoIP 1:1 calls. Matrix calls are signaled over
the room timeline (`m.call.invite` / `m.call.answer` / `m.call.hangup`
/ ICE candidates as `m.call.candidates`), with media over WebRTC
(SDP offer/answer inside the invite/answer events). This is
spec-standard, so a compliant client interoperates with Element.

Building this requires a WebRTC media stack — a large, security-
sensitive dependency crossing the crate boundary (estate rule: ADR
required).

## Decision

Stage the work; land 1:1 voice first:

1. **Stage 1 — signaling + audio (1:1, peregrine ↔ peregrine).**
   Implement `m.call.*` event handling in `peregrine-core` (timeline
   events + to-device fallback per the VoIP spec). Media via the
   `webrtc-rs` pure-Rust stack; audio capture/playback via `cpal`.
   UI: call bar in the gui (mute/hangup), incoming-call toast.
2. **Stage 2 — Element X interop.** Interop-test against Element X
   phone calls (spec VoIP; no LiveKit dependency for 1:1).
3. **Stage 3 — TURN.** Run coturn on the NAS (relay for NAT-ed
   networks); configure `turn.username/credential/uris` in the
   homeserver's `.well-known/matrix/client`. Without TURN, calls work
   only on same-LAN or permissive NATs.
4. **Stage 4 — video.** Video capture + render surface in the gui
   (Slint image buffer from decoded frames).
5. **Out of scope (v1):** group calls (requires an SFU — LiveKit or
   Matrix RTC); screen sharing; E2EE-encrypted media (MatrixVoIP
   E2EE-encrypted calls are an MSC; track upstream).

## Alternatives Considered

- **libwebrtc (C++ bindings)** — complete but heavy, unsafe-binding
  burden, conflicts with the estate's unsafe-deny posture.
- **LiveKit SFU from day one** — solves group calls + NAT traversal,
  but adds a server dependency and breaks Matrix-spec 1:1 interop
  (Element X would need the LiveKit path too; Matrix RTC MSC is
  still in flux).
- **Defer calls indefinitely** — the user's stated need; rejected.

## Consequences

- New dependency: `webrtc-rs` + `cpal` — vet grind grows; both are
  active, audited-adjacent (webrtc-rs is used by LiveKit's Rust stack).
- Audio permission handling (PipeWire on KDE) is platform glue.
- Calls only work when both ends support them; Element X 1:1 works
  per spec (stage 2 proves it).
- The estate homeserver needs no change for 1:1 (timeline signaling);
  stage 3 adds coturn as a NAS service (compose + config in the
  truenas-matrix runbook).

## Verification gates per stage

- Stage 1: peregrine↔peregrine 1:1 voice call on the estate LAN,
  audio heard both directions, hangup cleanly tears down the PeerConnection.
- Stage 2: peregrine↔Element X call answered, audio both ways.
- Stage 3: call over a NATed path (phone on LTE) succeeds via TURN.

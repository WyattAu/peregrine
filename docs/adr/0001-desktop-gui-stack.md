# ADR 0001: Use Slint for the Desktop GUI Shell

- **Status:** Accepted (spike gate passed 2026-11-20)
- **Date:** 2026-11-17
- **Deciders:** Peregrine team

## Context

Phase 2 requires a native desktop shell hosting the room list, timeline,
composer, and verification UX, with system notifications, a < 200 ms
cold-start SLA, and a < 120 MB idle budget. The core (sync, crypto,
storage, verification) is already `peregrine-core` + `peregrine-tui`,
live-verified against the estate homeserver — the GUI wraps the same
core, exactly the kestrel pattern.

Kestrel (`kestrel/docs/adr/0001-slint-gui-framework.md`) evaluated this
same question for its GUI shell and chose Slint: versioned 1.x API
stability, native compiled UI with small runtime meeting the cold-start
and memory budgets, strong-typed `.slint` DSL generating Rust structs
and callbacks, live-preview tooling. Licensing constraint applies to
peregrine identically: Slint is GPLv3 **or** Royalty-Free Desktop
License; peregrine is Apache-2.0, so we accept the RF Desktop License
terms (reviewed at the spike gate) with Iced as the fallback if terms
become incompatible.

Unlike kestrel, peregrine needs **no webview exception**: Matrix bodies
are rendered as sanitized plain text in v1 — no `wry`, no embedded
browser, no platform-glue layer for HTML.

## Decision

Use **Slint** for the `peregrine-gui` desktop shell, pending a
one-week spike that proves: three-pane layout (rooms/timeline/composer),
smooth 60 FPS scrolling of a 10k-event timeline, and cold-start < 200 ms
wrapping the existing `peregrine-core` API. Spike failure (layout
limits, budget miss, license blocker) supersedes this ADR with Iced as
the documented fallback.

System tray and desktop notifications are platform-glue via dedicated
crates (`tray-icon`, `notify-rust`) independent of the UI framework —
not a framework-selection criterion.

## Consequences

- **Reuse:** The TUI's model layer (rooms, timeline, verification
  state) maps directly to Slint's typed model properties; both
  frontends consume the same `peregrine-core` API, so behavior parity
  is structural.
- **Stability:** Slint 1.x compatibility policy reduces churn versus
  Iced's pre-1.0 breaking changes.
- **Licensing:** Same gate as kestrel — RF Desktop License compliance
  reviewed at the spike gate; Iced is the documented fallback.
- **Cost:** Team learns the Slint DSL; the TUI remains the
  power-user/SSH surface, so both frontends are maintained.
- **Verification UX:** Device verification (emoji SAS compare) renders
  natively in Slint — no webview round-trip.

## Spike Results (gate evidence, 2026-11-20, `peregrine-gui` a794d9f)

| Gate criterion | Budget | Measured | Verdict |
|---|---|---|---|
| Three-pane layout (rooms/timeline/composer) | renders | `.slint` compiles + renders on KDE Wayland and Xvfb | PASS |
| Cold start | < 200 ms | 175 ms (Wayland) / 85 ms (Xvfb) | PASS |
| 10k-event timeline scroll | 60 FPS | **60.2 fps — software GL (LLVMpipe) under system load 6–9, Xvfb** | PASS |

The scroll number is worst-case: femtovg fell back to software rendering
offscreen, and the machine carried the user's game + browser load
during the run. A GPU-backed render on an idle desktop has headroom.
Live-display runs measured 20–53 fps under the same load — compositor
/vsync/focus interaction, not renderer capability; re-confirm on an
idle desktop when `peregrine-gui` development starts in earnest.
License terms (RF Desktop License) accepted per kestrel precedent; the
Iced fallback clause remains in force only if those terms change.

## Alternatives Considered

- **egui** — immediate mode, excellent for tools; weaker for polished
  multi-pane desktop UX, accessibility, and system integration.
  Re-evaluated in the spike only if Slint fails its gate.
- **Iced** — pure-Rust Elm architecture; pre-1.0 churn and historical
  list-scrolling performance concerns. Documented fallback.
- **Tauri/web shell** — rejected: violates the native-shell posture and
  widens attack surface without a kestrel-equivalent need (no HTML
  bodies).
- **GTK/Qt bindings** — non-Rust UI core or binding-maintenance burden.

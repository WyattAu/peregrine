//! Peregrine TUI frontend. Phase 1 daily-driver target (kestrel-tui
//! pattern: cold start < 50 ms in-process). Rendering stack (ratatui)
//! lands in phase 1; this binary is the scaffold smoke test.

fn main() {
    // INVARIANT: binaries may print; print_stdout targets library error
    // hygiene, and CLI output is this crate's product.
    #[allow(clippy::print_stdout)]
    {
        println!(
            "peregrine {} (design scaffold; no protocol wiring yet)",
            peregrine_core::CORE_VERSION
        );
    }
}

//! Headless sync-loop probe: login, spawn the loop (Auto mode), print
//! every event for N seconds. Isolates transport behavior from the UI.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example loop_probe -- [seconds]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and error surface. Library crates keep the hygiene lints.

use std::{sync::Arc, time::Duration};

use peregrine_core::{
    adapter_matrix::MatrixSession,
    syncloop::{LoopCfg, LoopEvent, spawn_sync_loop},
};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("loop_probe: missing env {name}");
        std::process::exit(2);
    })
}

fn main() {
    // INVARIANT: example binaries may expect on runtime construction;
    // the process exits with the error message on failure.
    #[allow(clippy::expect_used)]
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let hs = require("PEREGRINE_HS_URL");
    let user = require("PEREGRINE_USER");
    let pass = require("PEREGRINE_PASS");
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let session = match MatrixSession::login(&hs, &user, &pass, "peregrine-loop", None).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("loop_probe: login failed: {e}");
            std::process::exit(1);
        }
    };
    println!("login ok; spawning loop for {secs}s (Auto mode)");

    let mut handle = spawn_sync_loop(Arc::clone(&session), &LoopCfg::default());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let ev = tokio::select! {
            ev = handle.next_event() => ev,
            () = tokio::time::sleep_until(deadline) => break,
        };
        match ev {
            Some(LoopEvent::Tick {
                window_shift,
                lists,
                rooms,
                updates,
                utd_pending,
                utd_escalated,
            }) => {
                println!(
                    "tick: shift={window_shift:?} lists={lists} rooms={rooms} updates={} utd={utd_pending} escalated={utd_escalated}",
                    updates.len()
                );
                for u in updates.iter().take(3) {
                    println!(
                        "  [{}] {} in {}: {}",
                        u.ingested_seq, u.sender, u.room, u.body
                    );
                }
            }
            Some(LoopEvent::Backoff { cadence_ms, error }) => {
                println!("backoff {cadence_ms} ms: {error}");
            }
            Some(LoopEvent::TransportDemoted { reason }) => {
                println!("transport demoted: {reason}");
            }
            None => {
                println!("loop ended");
                break;
            }
        }
    }
    handle.stop();
}

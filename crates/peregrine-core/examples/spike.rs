//! Spike: login + one sync pass + (opt-in) send against a live homeserver.
//!
//! Read-only by default. To transmit you must set BOTH `PEREGRINE_ROOM`
//! (a room id the account has joined) and `PEREGRINE_SEND` (the body).
//!
//! ```text
//! PEREGRINE_HS_URL=https://matrix.example.org \
//! PEREGRINE_USER=alice PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example spike
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product and
// the error surface. Library crates keep the hygiene lints.

use peregrine_core::{
    adapter_matrix::MatrixSession,
    clock::{Clock as _, MonotonicClock},
    protocol::{ProtocolSession, RoomId},
    window::{SyncWindow, WindowCfg},
};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("spike: missing env {name}");
        std::process::exit(2);
    })
}

fn step<T, E: std::fmt::Display>(what: &str, r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => {
            eprintln!("spike: {what} failed: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    // INVARIANT: example binaries may expect on runtime construction; the
    // process exits with the error message on failure via step().
    #[allow(clippy::expect_used)]
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let clock = MonotonicClock::new();

    let hs = require("PEREGRINE_HS_URL");
    let user = require("PEREGRINE_USER");
    let pass = require("PEREGRINE_PASS");

    let t0 = clock.now_ms();
    let session = step(
        "login",
        MatrixSession::login(
            &hs,
            &user,
            &pass,
            "peregrine-spike",
            env("PEREGRINE_STORE_PATH").as_deref(),
        )
        .await,
    );
    let login_ms = clock.now_ms() - t0;
    println!("login   ok  {login_ms} ms  (device {})", {
        // device id is embedded in txn ids; print via a probe send only.
        "see txn ids on send"
    });

    let t1 = clock.now_ms();
    let updates = step("sync", session.sync_once().await);
    let sync_ms = clock.now_ms() - t1;
    println!(
        "sync    ok  {sync_ms} ms  ({} timeline updates)",
        updates.len()
    );
    for u in updates.iter().take(5) {
        println!(
            "  [#{}] {} in {}: {}",
            u.ingested_seq, u.sender, u.room, u.body
        );
    }

    match (env("PEREGRINE_ROOM"), env("PEREGRINE_SEND")) {
        (Some(room), Some(body)) => {
            let t2 = clock.now_ms();
            let receipt = step(
                "send",
                session.send_message(&RoomId::new(room), &body).await,
            );
            let send_ms = clock.now_ms() - t2;
            println!("send    ok  {send_ms} ms  txn {}", receipt.txn_id);
        }
        _ => {
            println!("read-only pass (set PEREGRINE_ROOM + PEREGRINE_SEND to transmit)");
        }
    }

    // Hysteresis-window demo (PEREGRINE_WINDOW=1): synthetic scroll ramp
    // drives the sliding-sync range through sync_tick - the phase-1 loop
    // body, exercised end-to-end against the live server when set.
    if env("PEREGRINE_WINDOW").as_deref() == Some("1") {
        let cfg = WindowCfg::default_room_list();
        let mut window = SyncWindow::at(0, cfg.size, 0.0);
        let mut shifts = 0;
        for i in 0..40_u64 {
            // INVARIANT: example trace positions are exact small integers.
            #[allow(clippy::cast_precision_loss)]
            let pos = (i as f32) * 6.0; // gentle scroll: 240 items total
            let outcome = match session
                .sync_tick(&mut window, pos, i * 200, &cfg, 15_000)
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    let msg = e.to_string().to_ascii_lowercase();
                    if msg.contains("m_unrecognized")
                        || msg.contains("not found")
                        || msg.contains("404")
                    {
                        println!(
                            "window  demo: sliding sync unavailable on this server \
                             (classic fallback in the loop task); skipping"
                        );
                        break;
                    }
                    eprintln!("spike: sync_tick failed: {e}");
                    std::process::exit(1);
                }
            };
            if let Some(start) = outcome.window_shift {
                shifts += 1;
                println!("window  shift -> {start}");
            }
        }
        println!(
            "window  demo: {shifts} shifts over 40 ticks (settled at {})",
            window.start()
        );
    }

    println!("waterfall: login {login_ms} ms | sync {sync_ms} ms");
}

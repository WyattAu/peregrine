//! Headless send probe: restore/login -> send one encrypted message to
//! `PEREGRINE_ROOM` -> verify by transaction receipt. Exercises the
//! exact adapter `send_message` path the TUI and GUI composers use,
//! without any UI or pty in the loop.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! PEREGRINE_ROOM='!room:server' \
//! cargo run -p peregrine-core --example send_probe -- [message]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: example bins are CLI products; stdout/stderr are the surface.

use peregrine_core::{
    adapter_matrix::MatrixSession,
    protocol::{ProtocolSession, RoomId},
};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let hs = env("PEREGRINE_HS_URL").unwrap_or_else(|| {
        eprintln!("send_probe: missing PEREGRINE_HS_URL");
        std::process::exit(2);
    });
    let user = env("PEREGRINE_USER").unwrap_or_else(|| {
        eprintln!("send_probe: missing PEREGRINE_USER");
        std::process::exit(2);
    });
    let pass = env("PEREGRINE_PASS").unwrap_or_default();
    let room = env("PEREGRINE_ROOM").unwrap_or_else(|| {
        eprintln!("send_probe: missing PEREGRINE_ROOM");
        std::process::exit(2);
    });
    let body = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "peregrine send_probe message".to_owned());

    let home = env("HOME").unwrap_or_else(|| '.'.to_string());
    let safe: String = user
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let path = format!("{home}/.local/share/peregrine/store-{safe}-send-probe");
    let sidecar = std::path::Path::new(&path).with_extension("session.json");

    let session = if sidecar.exists() {
        match MatrixSession::restore(&hs, &path).await {
            Ok(s) => {
                println!("restore ok");
                s
            }
            Err(_) => fresh_login(&hs, &user, &pass, &path).await,
        }
    } else {
        fresh_login(&hs, &user, &pass, &path).await
    };

    // Bounded sync so the room is in client state.
    session
        .initial_sync()
        .await
        .map_err(|e| {
            eprintln!("send_probe: initial_sync failed: {e}");
            std::process::exit(3);
        })
        .ok();
    println!("sync    ok");

    let rid = RoomId::new(room.clone());
    match session.send_message(&rid, &body).await {
        Ok(receipt) => {
            println!("send    ok  txn={}", receipt.txn_id);
        }
        Err(e) => {
            eprintln!("send_probe: send failed: {e}");
            std::process::exit(4);
        }
    }
}

async fn fresh_login(hs: &str, user: &str, pass: &str, path: &str) -> MatrixSession {
    match MatrixSession::login(hs, user, pass, "peregrine-send-probe", Some(path)).await {
        Ok(s) => {
            println!("login   ok  (fresh device)");
            s
        }
        Err(e) => {
            eprintln!("send_probe: login failed: {e}");
            std::process::exit(2);
        }
    }
}

//! Live DM listener: waits for the phone test. Auto-accepts invites,
//! surfaces every decrypted timeline message, and reports the UTD
//! quarantine (the cross-device E2EE proof: an Element X DM over
//! federation must arrive decrypted, or park as UTD and resolve).
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example dm_live -- [seconds]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

use std::time::Duration;

use peregrine_core::{adapter_matrix::MatrixSession, protocol::ProtocolSession};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("dm_live: missing env {name}");
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
    let secs: u64 = env("PEREGRINE_SECS")
        .or_else(|| std::env::args().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);

    let session = MatrixSession::login(&hs, &user, &pass, "peregrine-dm", None)
        .await
        .unwrap_or_else(|e| {
            eprintln!("dm_live: login failed: {e}");
            std::process::exit(1);
        });
    let client = session.client().clone();
    println!("dm_live listening {secs}s as {user} (invites auto-accepted)");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        for room in client.invited_rooms() {
            let id = room.room_id().to_owned();
            println!(
                "invite   <- {id} ({}); joining",
                room.name().unwrap_or_default()
            );
            match client.join_room_by_id(&id).await {
                Ok(_) => println!("join     ok  {id}"),
                Err(e) => println!("join     FAILED {id}: {e}"),
            }
        }
        match session.sync_once().await {
            Ok(updates) => {
                for u in updates {
                    println!(
                        "msg      [{}] {} in {}: {}",
                        u.ingested_seq, u.sender, u.room, u.body
                    );
                }
            }
            Err(e) => println!("sync     error: {e}"),
        }
        let utd = session.utd_pending().unwrap_or_default();
        if !utd.is_empty() {
            println!("utd      parked sessions: {utd:?}");
        }
        tokio::time::sleep(Duration::from_millis(1_500)).await;
    }

    let pending = session.utd_pending().unwrap_or_default();
    println!(
        "done: utd sessions parked={} (resolutions logged above)",
        pending.len()
    );
}

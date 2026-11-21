//! Force-creates a NEW direct room with `@wyattau:matrix.org`, enables
//! E2EE, sends the first message, and prints the room id for follow-up
//! tests. Workaround for the stuck-extremities incident on the old DM
//! (see truenas-matrix runbook, INCIDENT 2026-11-20).
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example new_dm_probe
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: example bins are CLI products; stdout/stderr are the surface.

use matrix_sdk::ruma::{
    OwnedUserId,
    api::client::room::create_room::v3::{Request as CreateRoomRequest, RoomPreset},
};
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
        eprintln!("new_dm_probe: missing PEREGRINE_HS_URL");
        std::process::exit(2);
    });
    let user = env("PEREGRINE_USER").unwrap_or_else(|| {
        eprintln!("new_dm_probe: missing PEREGRINE_USER");
        std::process::exit(2);
    });
    let pass = env("PEREGRINE_PASS").unwrap_or_default();
    let peer = env("PEREGRINE_PEER").unwrap_or_else(|| "@wyattau:matrix.org".to_owned());
    let peer_id = OwnedUserId::try_from(peer.clone()).unwrap_or_else(|e| {
        eprintln!("new_dm_probe: bad PEREGRINE_PEER {peer}: {e}");
        std::process::exit(2);
    });

    let home = env("HOME").unwrap_or_else(|| '.'.to_string());
    let safe: String = user
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let path = format!("{home}/.local/share/peregrine/store-{safe}-send-probe");

    // Reuse the send_probe store (known-good device, no login churn).
    let session = match MatrixSession::restore(&hs, &path).await {
        Ok(s) => {
            println!("restore ok");
            s
        }
        Err(_) => {
            match MatrixSession::login(&hs, &user, &pass, "peregrine-send-probe", Some(&path)).await
            {
                Ok(s) => {
                    println!("login   ok  (fresh device)");
                    s
                }
                Err(e) => {
                    eprintln!("new_dm_probe: login failed: {e}");
                    std::process::exit(2);
                }
            }
        }
    };

    let mut req = CreateRoomRequest::new();
    req.name = Some("peregrine".to_owned());
    req.preset = Some(RoomPreset::PrivateChat);
    req.invite = vec![peer_id.clone()];
    req.is_direct = true;

    let room = session
        .client()
        .create_room(req)
        .await
        .map_err(|e| {
            eprintln!("new_dm_probe: create_room failed: {e}");
            std::process::exit(3);
        })
        .unwrap_or_else(|()| unreachable!());
    println!("room    ok  {} (invite sent to {peer})", room.room_id());

    if let Err(e) = room.enable_encryption().await {
        eprintln!("new_dm_probe: enable_encryption failed: {e}");
        std::process::exit(3);
    }
    println!("encrypt ok  (m.room.encryption set)");

    // Sync once so the room is in client state for the adapter send.
    session
        .initial_sync()
        .await
        .map_err(|e| {
            eprintln!("new_dm_probe: initial_sync failed: {e}");
            std::process::exit(3);
        })
        .ok();

    let rid = RoomId::new(room.room_id().as_str().to_owned());
    let body = "peregrine DM v2 — first message (accept the invite on your phone)";
    match session.send_message(&rid, body).await {
        Ok(receipt) => println!("send    ok  txn={}", receipt.txn_id),
        Err(e) => {
            eprintln!("new_dm_probe: send failed: {e}");
            std::process::exit(4);
        }
    }
    println!("ROOM_ID={}", room.room_id());
}

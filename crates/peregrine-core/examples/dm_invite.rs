//! Cross-server E2EE test (flipped direction): peregrine creates an
//! encrypted room, invites a remote user, waits for their join, sends
//! an encrypted greeting, and reports every message that arrives
//! DECRYPTED from the remote user (the cross-server Megolm proof).
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! PEREGRINE_PEER=@user:remote.example \
//! cargo run -p peregrine-core --example dm_invite -- [wait-seconds]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

use std::time::Duration;

use matrix_sdk::ruma::{
    EventEncryptionAlgorithm, OwnedUserId,
    api::client::room::create_room::v3::RoomPreset,
    events::{
        EmptyStateKey,
        room::{encryption::RoomEncryptionEventContent, message::RoomMessageEventContent},
    },
};
use peregrine_core::{adapter_matrix::MatrixSession, protocol::ProtocolSession};

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("dm_invite: missing env {name}");
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
    let peer = require("PEREGRINE_PEER");
    let peer_id: OwnedUserId = peer.parse().unwrap_or_else(|e| {
        eprintln!("dm_invite: bad PEREGRINE_PEER: {e}");
        std::process::exit(2);
    });
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let session = MatrixSession::login(&hs, &user, &pass, "peregrine-invite", None)
        .await
        .unwrap_or_else(|e| {
            eprintln!("dm_invite: login failed: {e}");
            std::process::exit(1);
        });
    let client = session.client().clone();

    let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
    request.preset = Some(RoomPreset::TrustedPrivateChat);
    request.invite = vec![peer_id.clone()];
    let room_id = client
        .create_room(request)
        .await
        .unwrap_or_else(|e| {
            eprintln!("dm_invite: create_room failed: {e}");
            std::process::exit(1);
        })
        .room_id()
        .to_owned();
    println!("room    ok  {room_id} (invited {peer})");

    let room = client.get_room(&room_id).unwrap_or_else(|| {
        eprintln!("dm_invite: room not found after create");
        std::process::exit(1);
    });
    room.send_state_event_for_key(
        &EmptyStateKey,
        RoomEncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2),
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("dm_invite: enable encryption failed: {e}");
        std::process::exit(1);
    });
    println!("crypto  ok  megolm enabled; waiting for {peer} to join (max {secs}s)");

    // Wait for the peer's membership to reach join, then greet.
    let mut joined = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while !joined && tokio::time::Instant::now() < deadline {
        session.sync_once().await.unwrap_or_else(|e| {
            eprintln!("dm_invite: sync failed: {e}");
            std::process::exit(1);
        });
        let members = room
            .members(matrix_sdk::RoomMemberships::JOIN)
            .await
            .unwrap_or_default();
        joined = members.iter().any(|m| m.user_id() == peer_id);
        if !joined {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    if !joined {
        println!("peer    TIMEOUT: {peer} never joined — invite undelivered?");
        return;
    }
    println!("peer    joined");

    // Greet AFTER their join so the Megolm session is shared to their
    // device directly (no backward-key-request dance).
    room.send(RoomMessageEventContent::text_plain(format!(
        "hello from peregrine @ {hs} — encrypted over federation"
    )))
    .await
    .unwrap_or_else(|e| {
        eprintln!("dm_invite: send failed: {e}");
        std::process::exit(1);
    });
    println!("send    ok  encrypted greeting sent; listening for replies…");

    let mut seen_marker = String::new();
    let reply_deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < reply_deadline {
        let updates = session.sync_once().await.unwrap_or_default();
        for u in updates {
            if u.sender.as_str() == peer.as_str() && u.body != seen_marker {
                seen_marker.clone_from(&u.body);
                println!(
                    "decrypt ok  [{}] {}: {} — cross-server Megolm VERIFIED",
                    u.ingested_seq, u.sender, u.body
                );
            }
        }
        let utd = session.utd_pending().unwrap_or_default();
        if !utd.is_empty() {
            println!("utd     parked: {utd:?}");
        }
        tokio::time::sleep(Duration::from_millis(1_500)).await;
    }
    println!("done");
}

//! E2EE self round-trip: create a room, enable Megolm via the
//! encryption state event, send through the matrix-sdk encryption
//! path, sync, and verify the event comes back DECRYPTED through the
//! timeline handler (own-device Megolm loop). Also asserts the UTD
//! quarantine stays empty.
//!
//! ```text
//! PEREGRINE_HS_URL=https://matrix.example.org \
//! PEREGRINE_USER=alice PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example e2ee_self
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

use std::time::Duration;

use matrix_sdk::{
    Client,
    ruma::{
        EventEncryptionAlgorithm,
        api::client::room::create_room::v3::RoomPreset,
        events::{
            EmptyStateKey,
            room::{encryption::RoomEncryptionEventContent, message::RoomMessageEventContent},
        },
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
        eprintln!("e2ee_self: missing env {name}");
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

    let session = MatrixSession::login(&hs, &user, &pass, "peregrine-e2ee", None)
        .await
        .unwrap_or_else(|e| {
            eprintln!("e2ee_self: login failed: {e}");
            std::process::exit(1);
        });
    println!("login   ok");

    let client: Client = session.client().clone();
    let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
    request.preset = Some(RoomPreset::PrivateChat);
    let room_id = client
        .create_room(request)
        .await
        .unwrap_or_else(|e| {
            eprintln!("e2ee_self: create_room failed: {e}");
            std::process::exit(1);
        })
        .room_id()
        .to_owned();
    println!("room    ok  {room_id}");

    let room = client.get_room(&room_id).unwrap_or_else(|| {
        eprintln!("e2ee_self: room not found after create");
        std::process::exit(1);
    });
    room.send_state_event_for_key(
        &EmptyStateKey,
        RoomEncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2),
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("e2ee_self: enable encryption failed: {e}");
        std::process::exit(1);
    });
    println!("crypto  ok  m.room.encryption set (megolm v1 aes-sha2)");

    // Give the server a beat to settle the state event.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let marker = format!("peregrine-e2ee-marker-{}", std::process::id());
    room.send(RoomMessageEventContent::text_plain(marker.clone()))
        .await
        .unwrap_or_else(|e| {
            eprintln!("e2ee_self: send failed: {e}");
            std::process::exit(1);
        });
    println!("send    ok  (encrypted path engaged)");

    // Poll classic syncs until the marker surfaces decrypted, or fail.
    let mut seen = None;
    for _ in 0..10 {
        match session.sync_once().await {
            Ok(updates) => {
                if let Some(u) = updates.iter().find(|u| u.body.contains(&marker)) {
                    seen = Some(u.clone());
                    break;
                }
            }
            Err(e) => {
                eprintln!("e2ee_self: sync failed: {e}");
                std::process::exit(1);
            }
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
    let Some(u) = seen else {
        eprintln!("e2ee_self: marker never surfaced decrypted — FAIL");
        std::process::exit(1);
    };
    println!(
        "decrypt ok  [{}] {} in {} — Megolm round-trip VERIFIED",
        u.ingested_seq, u.sender, u.room
    );

    let utd = session.utd_pending().unwrap_or_else(|e| {
        eprintln!("e2ee_self: utd probe failed: {e}");
        std::process::exit(1);
    });
    if utd.is_empty() {
        println!("utd     ok  quarantine empty");
    } else {
        eprintln!("e2ee_self: unexpected UTD sessions: {utd:?}");
        std::process::exit(1);
    }
}

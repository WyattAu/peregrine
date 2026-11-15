//! Probe: what does the SDK's DM discovery see for the peer?
//! Prints m.direct account data, per-room direct_targets, and
//! get_dm_room resolution after one full sync.

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and error surface.

use matrix_sdk::config::SyncSettings;
use peregrine_core::adapter_matrix::MatrixSession;

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("dm_probe: missing env {name}");
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

    let session = match MatrixSession::login(&hs, &user, &pass, "peregrine-dmprobe", None).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dm_probe: login failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = session.client().sync_once(SyncSettings::default()).await {
        eprintln!("dm_probe: sync failed: {e}");
        std::process::exit(1);
    }

    let Some(peer_id) = matrix_sdk::ruma::UserId::parse(&peer).ok() else {
        eprintln!("dm_probe: bad peer");
        std::process::exit(2);
    };

    let dm = session.client().get_dm_room(&peer_id);
    println!(
        "get_dm_room -> {:?}",
        dm.as_ref().map(|r| r.room_id().to_owned())
    );

    for room in session.client().joined_rooms() {
        let targets = room.direct_targets();
        if !targets.is_empty() {
            println!(
                "room {} direct_targets={:?} members={}",
                room.room_id(),
                targets,
                room.members_no_sync(matrix_sdk::RoomMemberships::ACTIVE).await.map(|m| m.len()).unwrap_or(0)
            );
        }
    }
}

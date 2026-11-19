//! Cross-client SAS verification: peregrine requests verification of
//! a REMOTE client device (e.g. Element X on the phone), prints its
//! short auth string for the human to compare against the peer's
//! display, and completes the flow (our MAC is sent as soon as the
//! short auth string is presentable; the flow completes when the
//! peer's MAC arrives — tap timing on the peer is irrelevant).
//!
//! Modes:
//! - default: scan — send to-device requests to every unverified peer
//!   device (dead devices never ready; the live client pops the
//!   request on screen).
//! - `PEREGRINE_VIA=user`: user-identity request — the SDK picks or
//!   creates the DM room and posts an in-room request, which shows in
//!   the peer's timeline on every active session.
//! - `PEREGRINE_PEER_DEVICE=<id>`: target one device explicitly.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! PEREGRINE_PEER=@user:remote.example \
//! cargo run -p peregrine-core --example verify_peer
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

use std::time::Duration;

use matrix_sdk::{
    config::SyncSettings,
    encryption::verification::{Emoji, SasVerification, VerificationRequest},
    ruma::{OwnedDeviceId, UserId},
};
use peregrine_core::adapter_matrix::MatrixSession;

/// Poll cycles for the whole flow (~2 s each).
const MAX_CYCLES: usize = 120;
/// Scan budget per device (~30 s): enough for a live client to ready.
const SCAN_CYCLES: usize = 15;
/// Sync longpoll bound.
const SYNC_TIMEOUT: Duration = Duration::from_secs(1);
/// Step wait.
const STEP_WAIT: Duration = Duration::from_secs(1);

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("verify_peer: missing env {name}");
        std::process::exit(2);
    })
}

fn die(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("verify_peer: {what}: {e}");
    std::process::exit(1);
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
    let peer_id = match UserId::parse(&peer) {
        Ok(u) => u,
        Err(e) => die("bad PEREGRINE_PEER", e),
    };

    let session = match MatrixSession::login(&hs, &user, &pass, "peregrine-verify", None).await {
        Ok(s) => s,
        Err(e) => die("login failed", &e),
    };
    println!("login   ok");

    // Initial sync: loads shared rooms, key-queries the peer's devices.
    if let Err(e) = session.client().sync_once(SyncSettings::default()).await {
        die("initial sync failed", &e);
    }

    let ok = if env("PEREGRINE_VIA").as_deref() == Some("user") {
        verify_via_identity(&session, &peer_id).await
    } else {
        let targets: Vec<OwnedDeviceId> = match env("PEREGRINE_PEER_DEVICE") {
            Some(id) => vec![OwnedDeviceId::from(id)],
            None => session
                .client()
                .encryption()
                .get_user_devices(&peer_id)
                .await
                .map(|uds| {
                    uds.devices()
                        .filter(|d| !d.is_verified())
                        .map(|d| d.device_id().to_owned())
                        .collect()
                })
                .unwrap_or_default(),
        };
        verify_via_scan(&session, &peer_id, targets).await
    };

    if ok {
        println!("result  VERIFIED — cross-client SAS proven");
    } else {
        eprintln!("result  FAIL");
        std::process::exit(1);
    }
}

/// User-identity verification path: DM-room in-room request.
async fn verify_via_identity(session: &MatrixSession, peer_id: &UserId) -> bool {
    let Some(identity) = session
        .client()
        .encryption()
        .get_user_identity(peer_id)
        .await
        .ok()
        .flatten()
    else {
        die("get_user_identity failed", "peer unknown after sync");
    };
    let request: VerificationRequest = match identity.request_verification().await {
        Ok(r) => r,
        Err(e) => die("user request_verification failed", &e),
    };
    println!("request ok  user-identity flow {}", request.flow_id());
    println!(
        "ACTION on phone: open the new room from @peregrine-test, accept the verification request"
    );
    let (done, cancelled, our_done) = drive_ready_then_verify(session, request, MAX_CYCLES).await;
    let trust = session
        .client()
        .encryption()
        .get_user_devices(peer_id)
        .await
        .is_ok_and(|uds| uds.devices().any(|d| d.is_verified()));
    done && !cancelled && (our_done || trust)
}

/// Device-scan verification path: send to-device requests to every
/// unverified peer device; dead devices never ready.
async fn verify_via_scan(
    session: &MatrixSession,
    peer_id: &UserId,
    targets: Vec<OwnedDeviceId>,
) -> bool {
    for dev_id in targets {
        let Some(dev) = session
            .client()
            .encryption()
            .get_device(peer_id, &dev_id)
            .await
            .ok()
            .flatten()
        else {
            println!("skip    {dev_id} (no crypto device)");
            continue;
        };
        let request: VerificationRequest = match dev.request_verification().await {
            Ok(r) => r,
            Err(e) => {
                println!("skip    {dev_id} (request failed: {e})");
                continue;
            }
        };
        println!("request ok  flow {} -> {dev_id}", request.flow_id());
        println!("ACTION on phone: if Element X shows a verification request, accept it");
        let (done, cancelled, our_done) =
            drive_ready_then_verify(session, request, SCAN_CYCLES).await;
        let trust = session
            .client()
            .encryption()
            .get_device(peer_id, &dev_id)
            .await
            .ok()
            .flatten()
            .is_some_and(|d| d.is_verified());
        if done && !cancelled && (our_done || trust) {
            println!("result  VERIFIED — cross-client SAS proven (device {dev_id}, trust={trust})");
            return true;
        }
        println!("next    {dev_id} did not complete; trying next device");
    }
    false
}

/// Drive the verifier side: wait for the peer to ready the request,
/// open SAS, print emojis, confirm. Returns (`flow_done`, `cancelled`,
/// `our_done`).
async fn drive_ready_then_verify(
    session: &MatrixSession,
    request: VerificationRequest,
    max_cycles: usize,
) -> (bool, bool, bool) {
    let client = session.client().clone();
    let mut sas: Option<SasVerification> = None;
    let mut emojis: Option<[Emoji; 7]> = None;
    let mut confirmed = false;

    for _ in 0..max_cycles {
        let _ = client
            .sync_once(SyncSettings::new().timeout(SYNC_TIMEOUT))
            .await;

        // Start SAS once the peer readied the request.
        if sas.is_none()
            && request.is_ready()
            && let Ok(Some(s)) = request.start_sas().await
        {
            sas = Some(s);
            println!("start   ok  SAS flow opened");
        }

        if let Some(s) = &sas {
            if s.is_cancelled() {
                println!("flow    CANCELLED by peer");
                return (false, true, false);
            }
            // Print the short auth string once presentable; the human
            // compares it against the peer's screen.
            if s.can_be_presented()
                && emojis.is_none()
                && let Some(list) = s.emoji()
            {
                for (i, emoji) in list.iter().enumerate() {
                    println!("emoji   {}  {} {}", i + 1, emoji.symbol, emoji.description);
                }
                emojis = Some(list);
            }
            // Confirm as soon as the string is presentable: our MAC is
            // sent immediately and the flow completes when the peer's
            // MAC arrives — their tap timing is irrelevant to
            // correctness, so no grace window is needed.
            if emojis.is_some() && !confirmed && s.confirm().await.is_ok() {
                confirmed = true;
                println!("confirm ok  ours sent");
            }
            if s.is_done() {
                println!("flow    done");
                return (true, false, confirmed);
            }
        }
        tokio::time::sleep(STEP_WAIT).await;
    }
    (false, false, confirmed)
}

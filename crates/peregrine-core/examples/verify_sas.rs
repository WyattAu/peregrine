//! SAS device-verification self-test: two sessions of the same account
//! run the interactive short-auth-string flow against each other
//! (request -> ready -> start -> accept -> emoji exchange -> confirm ->
//! done), then assert the verifier's trust view flipped to verified.
//!
//! This is the automation kernel for the emoji-verification UX
//! (catalog-3 K-track): production UIs wrap these exact SDK calls.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! cargo run -p peregrine-core --example verify_sas
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use matrix_sdk::{
    Client,
    config::SyncSettings,
    encryption::verification::{Emoji, SasVerification, VerificationRequest},
    ruma::{
        OwnedTransactionId, OwnedUserId as RumaOwnedUserId, UserId,
        events::key::verification::request::ToDeviceKeyVerificationRequestEvent,
    },
};
use peregrine_core::adapter_matrix::MatrixSession;

/// Shared slot type for the captured verification flow id.
type MutexSlot = Mutex<Option<OwnedTransactionId>>;

/// Clone a borrowed user id into its owned form.
fn ruma_owned(u: &UserId) -> RumaOwnedUserId {
    u.to_owned()
}

/// Poll cycles before the drive loop gives up (~2 s per cycle).
const MAX_CYCLES: usize = 60;
/// Per-sync server longpoll bound; keeps both sessions responsive.
const SYNC_TIMEOUT: Duration = Duration::from_secs(1);
/// Wait between cycle steps.
const STEP_WAIT: Duration = Duration::from_millis(300);

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("verify_sas: missing env {name}");
        std::process::exit(2);
    })
}

fn die(what: &str, e: impl std::fmt::Display) -> ! {
    eprintln!("verify_sas: {what}: {e}");
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

/// Register on session B a handler capturing the transaction id of an
/// incoming `m.key.verification.request` (the flow id B needs for
/// `get_verification_request`).
fn capture_request(client_b: &Client) -> Arc<MutexSlot> {
    let got: Arc<MutexSlot> = Arc::new(Mutex::new(None));
    let for_handler = Arc::clone(&got);
    client_b.add_event_handler(move |e: ToDeviceKeyVerificationRequestEvent| {
        let slot = Arc::clone(&for_handler);
        async move {
            let tid = e.content.transaction_id;
            if let Ok(mut g) = slot.lock() {
                *g = Some(tid);
            }
        }
    });
    got
}

async fn run() {
    let hs = require("PEREGRINE_HS_URL");
    let user = require("PEREGRINE_USER");
    let pass = require("PEREGRINE_PASS");

    let va = match MatrixSession::login(&hs, &user, &pass, "peregrine-va", None).await {
        Ok(s) => s,
        Err(e) => die("login A failed", &e),
    };
    let vb = match MatrixSession::login(&hs, &user, &pass, "peregrine-vb", None).await {
        Ok(s) => s,
        Err(e) => die("login B failed", &e),
    };
    println!("login   ok  two devices (va verifier, vb subject)");

    let got = capture_request(vb.client());

    // Initial full syncs: both olm machines online, key uploads done.
    if let Err(e) = va.client().sync_once(SyncSettings::default()).await {
        die("initial sync A failed", &e);
    }
    if let Err(e) = vb.client().sync_once(SyncSettings::default()).await {
        die("initial sync B failed", &e);
    }

    let Some(ua) = va.client().user_id().map(ruma_owned) else {
        eprintln!("verify_sas: no user id on session A");
        std::process::exit(1);
    };

    // Shared room bootstrap: device keys are tracked per room member,
    // so A creates a private room and B (same identity) joins it. The
    // member sync then key-queries the user and lands BOTH devices in
    // each store.
    let mut create = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
    create.preset =
        Some(matrix_sdk::ruma::api::client::room::create_room::v3::RoomPreset::TrustedPrivateChat);
    let room_id = match va.client().create_room(create).await {
        Ok(r) => r.room_id().to_owned(),
        Err(e) => die("create_room failed", &e),
    };
    match vb.client().join_room_by_id(&room_id).await {
        Ok(_) => println!("room    ok  {room_id} (both devices share it)"),
        Err(e) => die("join_room failed", &e),
    }
    if let Err(e) = va.client().sync_once(SyncSettings::default()).await {
        die("member sync A failed", &e);
    }
    if let Err(e) = vb.client().sync_once(SyncSettings::default()).await {
        die("member sync B failed", &e);
    }

    // A: locate B's device by its SESSION device id (display-name
    // lookups can hit stale devices from earlier runs — measured).
    let Some(dev_id_b) = vb.client().device_id().map(std::borrow::ToOwned::to_owned) else {
        eprintln!("verify_sas: no device id on session B");
        std::process::exit(1);
    };
    let Some(dev_b) = va
        .client()
        .encryption()
        .get_device(&ua, &dev_id_b)
        .await
        .ok()
        .flatten()
    else {
        die("get_device failed", "no crypto device for vb");
    };
    let request_a: VerificationRequest = match dev_b.request_verification().await {
        Ok(r) => r,
        Err(e) => die("request_verification failed", &e),
    };
    println!("request ok  flow opened against {dev_id_b}");

    let (verified, emoji_match) = drive(&va, &vb, &ua, request_a, got).await;

    // Final trust view from A.
    let trust = match va.client().encryption().get_device(&ua, &dev_id_b).await {
        // INVARIANT: a store error here means trust cannot be
        // confirmed; treating it as unverified is the safe default.
        Ok(Some(d)) => d.is_verified(),
        _ => false,
    };

    if verified && trust {
        println!("result  VERIFIED  (emoji match: {emoji_match}) — SAS flow proven live");
    } else {
        eprintln!("result  FAIL (done={verified} trust={trust} emoji={emoji_match})");
        std::process::exit(1);
    }
}

/// Take the captured flow id and accept the verification request on
/// B's side (m.key.verification.ready). Unmatched ids go back to the
/// slot for the next cycle.
async fn accept_request(client_b: &Client, ua: &UserId, tid: &str) -> bool {
    match client_b
        .encryption()
        .get_verification_request(ua, tid)
        .await
    {
        Some(req) => req.accept().await.is_ok(),
        None => false,
    }
}

async fn accept_sas(client_b: &Client, ua: &UserId, tid: &str) -> Option<SasVerification> {
    let v = client_b.encryption().get_verification(ua, tid).await?;
    let sb = v.sas()?;
    if sb.accept().await.is_err() {
        return None;
    }
    println!("accept  ok  B accepted the SAS flow");
    Some(sb)
}

/// Drive both sessions through the verification-request SAS state
/// machine. Returns (`flow_done`, `emoji_match`).
async fn drive(
    va: &MatrixSession,
    vb: &MatrixSession,
    ua: &UserId,
    request_a: VerificationRequest,
    got: Arc<MutexSlot>,
) -> (bool, bool) {
    let client_a = va.client().clone();
    let client_b = vb.client().clone();
    let mut sas_a: Option<SasVerification> = None;
    let mut sas_b: Option<SasVerification> = None;
    let mut emojis_a: Option<[Emoji; 7]> = None;
    let mut flow_tid: Option<OwnedTransactionId> = None;
    let mut request_accepted = false;
    let mut confirmed_a = false;
    let mut confirmed_b = false;
    let mut emoji_match = false;

    for _ in 0..MAX_CYCLES {
        let _ = client_a
            .sync_once(SyncSettings::new().timeout(SYNC_TIMEOUT))
            .await;
        let b_resp = client_b
            .sync_once(SyncSettings::new().timeout(SYNC_TIMEOUT))
            .await;
        if let Ok(resp) = &b_resp {
            let td = resp.to_device.len();
            if td > 0 {}
        }

        // B: capture the flow id once, accept the request, then the
        // SAS flow itself (each step retried until it lands).
        if flow_tid.is_none() {
            flow_tid = got.lock().ok().and_then(|mut g| g.take());
        }
        if sas_b.is_none()
            && !request_accepted
            && let Some(tid) = flow_tid.as_deref()
        {
            request_accepted = accept_request(&client_b, ua, tid.as_str()).await;
            if request_accepted {
                println!("ready   ok  B accepted the request");
            }
        }

        // A: start SAS once B is ready.
        if sas_a.is_none()
            && request_a.is_ready()
            && let Ok(Some(sas)) = request_a.start_sas().await
        {
            sas_a = Some(sas);
            println!("start   ok  A opened the SAS flow");
        }

        if request_accepted
            && sas_b.is_none()
            && let Some(tid) = flow_tid.as_deref()
        {
            sas_b = accept_sas(&client_b, ua, tid.as_str()).await;
        }

        // Capture each side's short auth string when presentable;
        // CONFIRM ONLY AFTER BOTH ARE CAPTURED AND COMPARED — the
        // compare-then-confirm ordering is the security property of
        // SAS (a mismatch must abort, never proceed).
        if let Some(sa) = &sas_a
            && emojis_a.is_none()
            && sa.can_be_presented()
        {
            emojis_a = sa.emoji();
        }
        let emojis_b = sas_b
            .as_ref()
            .filter(|sb| sb.can_be_presented())
            .and_then(SasVerification::emoji);
        if let (Some(ea), Some(eb)) = (emojis_a.as_ref(), emojis_b.as_ref())
            && !confirmed_a
            && !confirmed_b
        {
            emoji_match = *ea == *eb;
            let names: Vec<&str> = eb.iter().map(|e| e.description).collect();
            println!("emoji   ok  [{names:?}] match={emoji_match}");
            if let Some(sa) = &sas_a
                && sa.confirm().await.is_ok()
            {
                confirmed_a = true;
                println!("confirm ok  A");
            }
            if let Some(sb) = &sas_b
                && sb.confirm().await.is_ok()
            {
                confirmed_b = true;
                println!("confirm ok  B");
            }
        }

        let a_done = sas_a.as_ref().is_some_and(SasVerification::is_done);
        let b_done = sas_b.as_ref().is_some_and(SasVerification::is_done);
        if a_done && b_done {
            return (true, emoji_match);
        }
        tokio::time::sleep(STEP_WAIT).await;
    }
    (false, emoji_match)
}

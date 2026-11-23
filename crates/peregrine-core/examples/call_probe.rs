//! Stage-1 signaling verification (ADR 0003): two sessions in one
//! room, caller sends `m.call.invite` (SDP offer), callee answers
//! (`m.call.answer`), callee hangs up (`m.call.hangup`). Media engine
//! not involved — this gate proves the signaling layer end-to-end.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! PEREGRINE_ROOM='!room:server' \
//! cargo run -p peregrine-core --example call_probe
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: example bins are CLI products; stdout/stderr are the surface.

use std::sync::Arc;
use std::time::Duration;

use matrix_sdk::Client;
use peregrine_core::calls::{CallSignal, SignalingService};
use tokio::sync::mpsc;

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

async fn connect(name: &str) -> Arc<Client> {
    let hs = env("PEREGRINE_HS_URL").unwrap_or_default();
    let user = env("PEREGRINE_USER").unwrap_or_default();

    let home = env("HOME").unwrap_or_else(|| '.'.to_string());
    let safe: String = user
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let path = format!("{home}/.local/share/peregrine/store-{safe}-{name}");
    let sidecar = std::path::Path::new(&path).with_extension("session.json");
    let client = if sidecar.exists() {
        match Client::builder()
            .homeserver_url(&hs)
            .sqlite_store(&path, None)
            .build()
            .await
        {
            Ok(c) => {
                println!("{name}: client restored");
                c
            }
            Err(_) => fresh(&hs, &path).await,
        }
    } else {
        fresh(&hs, &path).await
    };
    Arc::new(client)
}

async fn fresh(_hs: &str, path: &str) -> Client {
    let hs = env("PEREGRINE_HS_URL").unwrap_or_default();
    let user = env("PEREGRINE_USER").unwrap_or_default();
    let pass = env("PEREGRINE_PASS").unwrap_or_default();

    let c = Client::builder()
        .homeserver_url(hs)
        .sqlite_store(path, None)
        .build()
        .await
        .expect("client build");
    c.matrix_auth()
        .login_username(&user, &pass)
        .initial_device_display_name("peregrine-call-probe")
        .await
        .expect("login");
    println!("login ok");
    c
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let room = env("PEREGRINE_ROOM").unwrap_or_else(|| {
        eprintln!("call_probe: missing PEREGRINE_ROOM");
        std::process::exit(2);
    });

    // Two sessions: caller keeps its own client; callee is a second
    // login in the same account (two devices, one user — the exact
    // multi-device topology that wedged the old room, now exercised
    // for calls).
    let caller = connect("call-probe-a").await;
    let callee = connect("call-probe-b").await;

    // Simple bounded sync pumps.
    for (label, c) in [("caller", caller.clone()), ("callee", callee.clone())] {
        tokio::spawn(async move {
            let settings =
                matrix_sdk::config::SyncSettings::default().timeout(Duration::from_secs(8));
            loop {
                if let Err(e) = c.sync_once(settings.clone()).await {
                    eprintln!("{label}: sync error: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
    // Let the first syncs land (room + events state).
    tokio::time::sleep(Duration::from_secs(6)).await;

    let (tx_a, mut rx_a) = mpsc::channel::<CallSignal>(64);
    let (tx_b, mut rx_b) = mpsc::channel::<CallSignal>(64);
    let sig_a = SignalingService::new((*caller).clone(), tx_a);
    let sig_b = SignalingService::new((*callee).clone(), tx_b);

    let room = peregrine_core::protocol::RoomId::new(room.clone());
    let call_id = format!("probe-{}", std::process::id());
    let offer = "v=0\r\no=- 46117317 46117317 IN IP4 127.0.0.1\r\ns=peregrine stage1\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=rtpmap:111 opus/48000/2\r\n";

    println!("caller: sending invite");
    match sig_a.send_invite(&room, call_id.clone(), offer.to_owned(), 60_000).await {
        Ok(_) => println!("caller: invite sent"),
        Err(e) => {
            eprintln!("FAIL: invite: {e}");
            std::process::exit(4);
        }
    }

    let mut answered = false;
    let mut answer_seen = false;
    let mut callee_hangup = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);

    loop {
        if answered && answer_seen && callee_hangup {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::select! {
            maybe = rx_b.recv() => {
                match maybe {
                    Some(CallSignal::Invite { call_id: c, .. }) => {
                        println!("callee: invite received (call_id={c}) -> answering");
                        let sdp = "v=0\r\no=- 2 2 IN IP4 127.0.0.1\r\ns=peregrine answer\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=rtpmap:111 opus/48000/2\r\n";
                        if let Err(e) = sig_b.send_answer(&room, c.clone(), sdp.to_owned()).await {
                            eprintln!("FAIL: answer send: {e}");
                        }
                        answered = true;
                    }
                    Some(CallSignal::Hangup { call_id: c, .. }) => {
                        println!("callee: hangup received (call_id={c}) — full cycle closed");
                        callee_hangup = true;
                    }
                    _ => {}
                }
            }
            maybe = rx_a.recv() => {
                match maybe {
                    Some(CallSignal::Answer { room: _, call_id: c, sdp }) => {
                        println!("caller: answer received (call_id={c}, {} bytes SDP) -> hanging up", sdp.len());
                        answer_seen = true;
                        let _ = sig_a.send_hangup(&room, c).await;
                    }
                    _ => {}
                }
            }
        }
    }

    println!(
        "RESULT: {} (answered={answered}, answer_seen={answer_seen}, callee_hangup={callee_hangup})",
        if answered && answer_seen && callee_hangup { "PASS" } else { "FAIL" }
    );
    let _ = call_id;
}

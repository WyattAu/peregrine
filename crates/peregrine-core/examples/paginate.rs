//! Back-pagination live test: dumps history pages for a room via the
//! adapter's `paginate_history` (backward /messages with token
//! chaining), following `next_from` until exhausted or the page cap.
//!
//! ```text
//! PEREGRINE_HS_URL=... PEREGRINE_USER=... PEREGRINE_PASS=... \
//! PEREGRINE_ROOM=!room:server \
//! cargo run -p peregrine-core --example paginate -- [max-pages]
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: examples are CLI programs; stdout/stderr are the product
// and the error surface. Library crates keep the hygiene lints.

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

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("paginate: missing env {name}");
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
    let room = require("PEREGRINE_ROOM");
    let max_pages: u32 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    let session = match MatrixSession::login(
        &hs,
        &user,
        &pass,
        "peregrine-page",
        env("PEREGRINE_STORE_PATH").as_deref(),
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("paginate: login failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = session.sync_once().await {
        eprintln!("paginate: initial sync failed: {e}");
        std::process::exit(1);
    }

    let rid = RoomId::new(room.as_str());
    let mut from: Option<String> = None;
    let mut total = 0_usize;
    for page in 0..max_pages {
        let result = match from.as_deref() {
            Some(tok) => session.paginate_history(&rid, Some(tok), 30).await,
            None => session.paginate_history(&rid, None, 30).await,
        };
        let p = match result {
            Ok(p) => p,
            Err(e) => {
                eprintln!("paginate: page {} failed: {e}", page + 1);
                std::process::exit(1);
            }
        };
        println!(
            "page    {}  {} events (next_from={})",
            page + 1,
            p.updates.len(),
            p.next_from.as_deref().unwrap_or("<exhausted>")
        );
        for u in &p.updates {
            println!("  [{}] {}: {}", u.ingested_seq, u.sender, u.body);
        }
        total += p.updates.len();
        let Some(tok) = p.next_from else {
            println!("history exhausted after {total} events");
            return;
        };
        from = Some(tok);
    }
    println!("dumped {total} events across {max_pages} pages (cap reached)");
}

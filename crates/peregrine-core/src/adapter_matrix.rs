//! matrix-sdk adapter (substrate: matrix-sdk 0.19, rustls, memory store).
//! Feature-gated: `peregrine-core/matrix-sdk` (on by default; DST-only
//! consumers build std-only).
//!
//! Spike scope (phase 0): login, one bounded sync pass, plain-text send
//! with deterministic txn ids. E2EE + persistent stores land in phase 1;
//! the sync loop becomes the hysteresis-filtered sliding-sync pipeline
//! (catalog-1 D-track) after the spike proves the seam.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use matrix_sdk::{
    Client,
    config::SyncSettings,
    ruma::events::room::message::{OriginalSyncRoomMessageEvent, RoomMessageEventContent},
};

use crate::protocol::{ProtocolSession, RoomId, TimelineUpdate, TxnReceipt};

/// Substrate error. String-carried for the spike; typed taxonomy lands
/// with the retry/backoff policy (catalog-3 L#6).
#[derive(Debug)]
pub struct MatrixError(String);

impl core::fmt::Display for MatrixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "matrix: {}", self.0)
    }
}

impl std::error::Error for MatrixError {}

fn err<E: core::fmt::Display>(what: &str, e: E) -> MatrixError {
    MatrixError(format!("{what}: {e}"))
}

fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, MatrixError> {
    m.lock().map_err(|e| MatrixError(format!("poisoned: {e}")))
}

/// Raw ingest line as seen by the event handler, pre-normalization.
struct RawIngest {
    room: String,
    sender: String,
    body: String,
    origin_ts_ms: u64,
}

/// A live, authenticated matrix-sdk session implementing
/// [`ProtocolSession`]. Cheap to clone (the SDK client is an Arc).
pub struct MatrixSession {
    client: Client,
    user_id: String,
    device: String,
    inbox: Arc<Mutex<Vec<RawIngest>>>,
    ingest_seq: Arc<AtomicU64>,
    send_seq: Mutex<u64>,
}

impl MatrixSession {
    /// Password login against `homeserver_url`; registers the timeline
    /// ingest handler. Spike note: edits/relations are not yet normalized;
    /// the relations quarantine (catalog-3 L#7) owns that model.
    ///
    /// # Errors
    ///
    /// Fails on URL parse, client build, or credential rejection by the
    /// homeserver; the substrate message is carried in [`MatrixError`].
    pub async fn login(
        homeserver_url: &str,
        user: &str,
        pass: &str,
        device_name: &str,
    ) -> Result<Self, MatrixError> {
        let url = url::Url::parse(homeserver_url).map_err(|e| err("homeserver url", e))?;
        let client = Client::new(url).await.map_err(|e| err("client build", e))?;
        client
            .matrix_auth()
            .login_username(user, pass)
            .initial_device_display_name(device_name)
            .await
            .map_err(|e| err("login", e))?;

        let user_id = client
            .user_id()
            .map_or_else(|| user.to_string(), std::string::ToString::to_string);
        let device = client
            .device_id()
            .map_or_else(|| "unknown".to_string(), std::string::ToString::to_string);

        let inbox: Arc<Mutex<Vec<RawIngest>>> = Arc::new(Mutex::new(Vec::new()));
        let handler_inbox = Arc::clone(&inbox);
        client.add_event_handler(
            move |e: OriginalSyncRoomMessageEvent, room: matrix_sdk::room::Room| {
                let inbox = Arc::clone(&handler_inbox);
                async move {
                    // INVARIANT: bounded-discipline note — the ingest Vec is
                    // drained by sync_once every pass; the lock-free ring that
                    // makes the bound structural (catalog-1 B#12) lands with
                    // the pipeline rewrite.
                    if let Ok(mut guard) = inbox.lock() {
                        guard.push(RawIngest {
                            room: room.room_id().to_string(),
                            sender: e.sender.to_string(),
                            body: e.content.body().to_owned(),
                            origin_ts_ms: u64::from(e.origin_server_ts.get()),
                        });
                    }
                }
            },
        );

        Ok(Self {
            client,
            user_id,
            device,
            inbox,
            ingest_seq: Arc::new(AtomicU64::new(0)),
            send_seq: Mutex::new(0),
        })
    }

    /// The underlying substrate client (sliding-sync setup, rooms API).
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl ProtocolSession for MatrixSession {
    type Error = MatrixError;

    fn user_id(&self) -> &str {
        &self.user_id
    }

    async fn sync_once(&self) -> Result<Vec<TimelineUpdate>, Self::Error> {
        // Waterfall timing lives at the call site (see examples/spike.rs);
        // this pass is bounded: one request-response cycle.
        self.client
            .sync_once(SyncSettings::default())
            .await
            .map_err(|e| err("sync", e))?;

        let raw = lock(&self.inbox)?.drain(..).collect::<Vec<_>>();
        let updates = raw
            .into_iter()
            .map(|r| TimelineUpdate {
                ingested_seq: self.ingest_seq.fetch_add(1, Ordering::Relaxed),
                room: RoomId::new(r.room),
                sender: r.sender,
                body: r.body,
                origin_ts_ms: r.origin_ts_ms,
            })
            .collect::<Vec<_>>();
        Ok(updates)
    }

    async fn send_message(&self, room: &RoomId, body: &str) -> Result<TxnReceipt, Self::Error> {
        let seq = {
            let mut g = lock(&self.send_seq)?;
            let s = *g;
            *g += 1;
            s
        };
        let txn_id = format!("peregrine.{}.{}.{}", self.device, room.as_str(), seq);
        let rid =
            matrix_sdk::ruma::RoomId::parse(room.as_str()).map_err(|e| err("room id parse", e))?;
        let target = self
            .client
            .get_room(&rid)
            .ok_or_else(|| MatrixError(format!("room not in state: {room}")))?;
        let content = RoomMessageEventContent::text_plain(body);
        target
            .send(content)
            .with_transaction_id(txn_id.clone().into())
            .await
            .map_err(|e| err("send", e))?;
        Ok(TxnReceipt { txn_id })
    }
}

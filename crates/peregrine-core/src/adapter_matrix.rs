//! matrix-sdk adapter (substrate: matrix-sdk 0.19, rustls, memory store).
//! Feature-gated: `peregrine-core/matrix-sdk` (on by default; DST-only
//! consumers build std-only).
//!
//! Spike scope (phase 0): login, one bounded sync pass, plain-text send
//! with deterministic txn ids. E2EE + persistent stores land in phase 1;
//! the sync loop becomes the hysteresis-filtered sliding-sync pipeline
//! (catalog-1 D-track) after the spike proves the seam.

use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use matrix_sdk::{
    Client,
    config::SyncSettings,
    ruma::{
        UserId,
        events::room::{
            encrypted::OriginalSyncRoomEncryptedEvent,
            message::{OriginalSyncRoomMessageEvent, RoomMessageEventContent},
        },
    },
    sliding_sync::{SlidingSyncList, SlidingSyncMode},
};

/// One backward history page (see [`MatrixSession::paginate_history`]).
#[derive(Debug, Clone)]
pub struct HistoryPage {
    /// Token for the next backward call; `None` = exhausted.
    pub next_from: Option<String>,
    /// Normalized updates, chronological (oldest first).
    pub updates: Vec<crate::protocol::TimelineUpdate>,
}
use peregrine_crypto::keyreq::{
    KeyRequestTracker, RequestCfg, RetryVerdict, SessionId as MegolmSessionId,
};

/// UTD retry ladder (mirrors `RequestCfg::default_ladder`).
const UTD_CFG: RequestCfg = RequestCfg::default_ladder();

/// One bounded sliding-sync pass outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlidingPass {
    /// Lists the server reported as updated.
    pub lists: usize,
    /// Rooms the server reported as changed.
    pub rooms: usize,
}

/// Name of the main room list inside the sliding-sync session; re-adding
/// a list with this name replaces its ranges (the window-update path).
const SYNC_LIST: &str = "peregrine-main";

use crate::{
    clock::{Clock as _, MonotonicClock},
    protocol::{ProtocolSession, RoomId, TimelineUpdate, TxnReceipt},
};

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
    sliding: OnceLock<matrix_sdk::sliding_sync::SlidingSync>,
    /// UTD quarantine: sessions whose events failed Megolm decryption.
    /// Payload = event id string (the substrate re-decrypts stored events
    /// on key arrival; our cascade exposes diagnostics + resolution).
    utd: Arc<Mutex<KeyRequestTracker<String>>>,
}

/// Timeline events per room requested by the default window.
pub const DEFAULT_TIMELINE_LIMIT: u32 = 10;

/// One pipeline tick: window decision, optional range shift, one bounded
/// sliding pass, inbox drain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncTickOutcome {
    /// `Some(start)` when the hysteresis window issued a range shift.
    pub window_shift: Option<u32>,
    /// Lists the server reported updated.
    pub lists: usize,
    /// Rooms the server reported changed.
    pub rooms: usize,
    /// Normalized timeline updates observed this tick.
    pub updates: Vec<TimelineUpdate>,
}

impl MatrixSession {
    /// Password login against `homeserver_url`; registers the timeline
    /// ingest handler. `store_path` selects the persistent sqlite store
    /// (state + crypto); `None` keeps the memory store for DST-style
    /// throwaway sessions. E2EE is compiled in: encrypted rooms are
    /// transparently decrypted on ingest and encrypted on send by the
    /// substrate once sessions are established.
    ///
    /// Spike scope note: edits/relations are not yet normalized; the
    /// relations quarantine (catalog-3 L#7) owns that model.
    ///
    /// # Errors
    ///
    /// Fails on URL parse, client build, store open, or credential
    /// rejection by the homeserver; the substrate message is carried in
    /// [`MatrixError`].
    pub async fn login(
        homeserver_url: &str,
        user: &str,
        pass: &str,
        device_name: &str,
        store_path: Option<&str>,
    ) -> Result<Self, MatrixError> {
        let url = url::Url::parse(homeserver_url).map_err(|e| err("homeserver url", e))?;

        // INVARIANT: crypto keys must survive process restarts or every
        // new session loses megolm session state and all encrypted
        // history becomes permanently undecryptable (measured). The
        // default is therefore a persistent per-user+device store under
        // XDG data home; pass Some(":memory:") to opt out (tests).
        let resolved = if let Some(path) = store_path {
            path.to_owned()
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned());
            let safe = |s: &str| {
                s.chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '.' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>()
            };
            format!(
                "{home}/.local/share/peregrine/store-{}-{}",
                safe(user),
                safe(device_name)
            )
        };
        let client = Client::builder()
            .homeserver_url(url)
            .sqlite_store(&resolved, None)
            .build()
            .await
            .map_err(|e| err("client build (sqlite store)", e))?;
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

        Ok(Self::attach(client, user_id.as_str().to_owned(), device))
    }

    /// Wire the ingest handlers and assemble the session around an
    /// authenticated client (shared by login and restore).
    fn attach(client: Client, user_id: String, device: String) -> Self {
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

        let utd: Arc<Mutex<KeyRequestTracker<String>>> =
            Arc::new(Mutex::new(KeyRequestTracker::new()));
        let utd_for_handler = Arc::clone(&utd);
        client.add_event_handler(
            move |e: OriginalSyncRoomEncryptedEvent, room: matrix_sdk::room::Room| {
                async move {
                    let session = match &e.content.scheme {
                        matrix_sdk::ruma::events::room::encrypted::EncryptedEventScheme::MegolmV1AesSha2(c) => {
                            Some(MegolmSessionId::new(c.session_id.clone()))
                        }
                        _ => None,
                    };
                    let Some(sid) = session else { return };
                    // INVARIANT: wall time here is the sanctioned
                    // MonotonicClock wrapper (clock.rs); ladder scheduling
                    // stays deterministic per session.
                    let clock = MonotonicClock::new();
                    if let Ok(mut guard) = utd_for_handler.lock() {
                        let _ = guard.on_undecryptable(
                            sid,
                            format!("{}:{}", room.room_id(), e.event_id),
                            clock.now_ms(),
                            &UTD_CFG,
                        );
                    }
                }
            },
        );

        Self {
            client,
            user_id,
            device,
            inbox,
            ingest_seq: Arc::new(AtomicU64::new(0)),
            send_seq: Mutex::new(0),
            sliding: OnceLock::new(),
            utd: Arc::clone(&utd),
        }
    }

    /// The underlying substrate client (sliding-sync setup, rooms API).
    #[must_use]
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Drain the ingest inbox into normalized [`TimelineUpdate`]s
    /// (ingestion-sequence assigned here: the only layout ordering).
    ///
    /// # Errors
    ///
    /// Fails only on poisoned lock.
    pub fn drain_updates(&self) -> Result<Vec<TimelineUpdate>, MatrixError> {
        let raw = lock(&self.inbox)?.drain(..).collect::<Vec<_>>();
        Ok(raw
            .into_iter()
            .map(|r| TimelineUpdate {
                ingested_seq: self.ingest_seq.fetch_add(1, Ordering::Relaxed),
                room: RoomId::new(r.room),
                sender: r.sender,
                body: r.body,
                origin_ts_ms: r.origin_ts_ms,
            })
            .collect::<Vec<_>>())
    }

    /// UTD diagnostics: parked-event counts per session, sorted by
    /// session id (trust-DAG inspector + key-request tracing cards).
    ///
    /// # Errors
    ///
    /// Fails only on poisoned lock; carries the poison reason.
    pub fn utd_pending(&self) -> Result<Vec<(String, usize)>, MatrixError> {
        let guard = self
            .utd
            .lock()
            .map_err(|e| MatrixError(format!("utd poisoned: {e}")))?;
        Ok(guard
            .parked_counts()
            .into_iter()
            .map(|(sid, n)| (sid.as_str().to_owned(), n))
            .collect())
    }

    /// Key arrived for `session`: resolve it and return the quarantined
    /// event ids (cascade bookkeeping; the substrate re-decrypts stored
    /// events itself when the key lands).
    ///
    /// # Errors
    ///
    /// Fails only on poisoned lock.
    pub fn resolve_session(&self, session: &str) -> Result<Vec<String>, MatrixError> {
        let mut guard = self
            .utd
            .lock()
            .map_err(|e| MatrixError(format!("utd poisoned: {e}")))?;
        Ok(guard.on_key_received(&MegolmSessionId::new(session)))
    }

    /// Advance the UTD retry ladder (sync-loop tick). Verdicts drive the
    /// key-request transport; escalation surfaces to the user.
    ///
    /// # Errors
    ///
    /// Fails only on poisoned lock.
    pub fn utd_tick(&self, now_ms: u64) -> Result<Vec<RetryVerdict>, MatrixError> {
        let mut guard = self
            .utd
            .lock()
            .map_err(|e| MatrixError(format!("utd poisoned: {e}")))?;
        Ok(guard.tick(now_ms, &UTD_CFG))
    }

    /// One bounded sliding-sync pass: polls the stream for up to
    /// `max_wait_ms`, returns the update counts, then returns. The
    /// long-lived loop that owns the stream end-to-end is the phase-1
    /// pipeline (hysteresis window drives `apply_window` between passes).
    ///
    /// # Errors
    ///
    /// Fails on session build, stream error, or timeout expiry with no
    /// update.
    pub async fn sliding_pass(&self, max_wait_ms: u64) -> Result<SlidingPass, MatrixError> {
        // SlidingSync is Arc-backed: the clone is a handle, not a copy.
        if let Some(s) = self.sliding.get() {
            return self.sliding_pass_built(s, max_wait_ms).await;
        }
        let s = self
            .client
            .sliding_sync(SYNC_LIST)
            .map_err(|e| err("sliding session", e))?
            .build()
            .await
            .map_err(|e| err("sliding build", e))?;
        let _ = self.sliding.set(s.clone());
        self.sliding_pass_built(&s, max_wait_ms).await
    }

    /// Poll one update off an already-built sliding-sync session.
    async fn sliding_pass_built(
        &self,
        sliding: &matrix_sdk::sliding_sync::SlidingSync,
        max_wait_ms: u64,
    ) -> Result<SlidingPass, MatrixError> {
        use futures_util::StreamExt as _;
        let mut stream = std::pin::pin!(sliding.sync());
        let summary = tokio::time::timeout(
            std::time::Duration::from_millis(max_wait_ms),
            stream.as_mut().next(),
        )
        .await
        .map_err(|_| MatrixError("sliding pass timed out".into()))?
        .ok_or_else(|| MatrixError("sliding stream ended unexpectedly".into()))?
        .map_err(|e| err("sliding update", e))?;
        Ok(SlidingPass {
            lists: summary.lists.len(),
            rooms: summary.rooms.len(),
        })
    }

    /// One pipeline tick (the phase-1 loop body): the caller's
    /// [`SyncWindow`](crate::window::SyncWindow) decides whether the
    /// sliding-sync range shifts (hysteresis; D2#13), a bounded pass runs,
    /// and the inbox drains into normalized updates. The loop that calls
    /// this owns the window and the pacing; nothing here spins.
    ///
    /// # Errors
    ///
    /// Fails on range application, pass timeout/stream error, or poisoned
    /// locks; substrate messages are carried in [`MatrixError`].
    pub async fn sync_tick(
        &self,
        window: &mut crate::window::SyncWindow,
        anchor_pos: f32,
        ts_ms: u64,
        window_cfg: &crate::window::WindowCfg,
        max_wait_ms: u64,
    ) -> Result<SyncTickOutcome, MatrixError> {
        let mut out = SyncTickOutcome::default();
        match window.on_scroll(anchor_pos, ts_ms, window_cfg) {
            crate::window::WindowAction::Stay => {}
            crate::window::WindowAction::Shift { start } => {
                self.apply_window(start, window_cfg.size, DEFAULT_TIMELINE_LIMIT)
                    .await?;
                out.window_shift = Some(start);
            }
        }
        let pass = self.sliding_pass(max_wait_ms).await?;
        out.lists = pass.lists;
        out.rooms = pass.rooms;
        out.updates = self.drain_updates()?;
        Ok(out)
    }

    /// Point the sliding-sync room list at `[start, start + size)` with
    /// `timeline_limit` events per room. First call builds the session;
    /// later calls replace the list in place (the hysteresis window's
    /// `Shift` action lands here - catalog-1 D-track).
    ///
    /// # Errors
    ///
    /// Fails on session build or list replacement; the substrate message
    /// is carried in [`MatrixError`].
    pub async fn apply_window(
        &self,
        start: u32,
        size: u32,
        timeline_limit: u32,
    ) -> Result<(), MatrixError> {
        let end = start.saturating_add(size.saturating_sub(1));
        let list = SlidingSyncList::builder(SYNC_LIST)
            .sync_mode(SlidingSyncMode::new_selective().add_range(start..=end))
            .timeline_limit(timeline_limit);
        if let Some(sliding) = self.sliding.get() {
            // Window update path: same-name list replacement.
            sliding
                .add_list(list)
                .await
                .map_err(|e| err("sliding list update", e))?;
            return Ok(());
        }
        let sliding = self
            .client
            .sliding_sync(SYNC_LIST)
            .map_err(|e| err("sliding session", e))?
            .build()
            .await
            .map_err(|e| err("sliding build", e))?;
        sliding
            .add_list(list)
            .await
            .map_err(|e| err("sliding list add", e))?;
        // Racing builders: the loser's session is discarded; the winner
        // carries the initial window and later shifts go through the
        // update path above.
        let _ = self.sliding.set(sliding);
        Ok(())
    }
}

impl MatrixSession {
    /// One backward history page for `room_id`, normalized into
    /// [`TimelineUpdate`]s (chronological, oldest first). `from` is the
    /// token from the previous [`HistoryPage`] (`None` = start at the
    /// newest known event). `next_from` in the page feeds the next call;
    /// `None` means history is exhausted.
    ///
    /// # Errors
    ///
    /// Fails on unknown room or substrate request errors.
    pub async fn paginate_history(
        &self,
        room_id: &RoomId,
        from: Option<&str>,
        limit: u32,
    ) -> Result<HistoryPage, MatrixError> {
        use matrix_sdk::room::MessagesOptions;
        let rid = matrix_sdk::ruma::RoomId::parse(room_id.as_str())
            .map_err(|e| MatrixError(format!("room id parse: {e}")))?;
        let Some(room) = self.client.get_room(&rid) else {
            return Err(MatrixError(format!("room not in state: {room_id}")));
        };
        let mut opts = MessagesOptions::backward().from(from);
        opts.limit = matrix_sdk::ruma::UInt::from(limit);
        let messages = room.messages(opts).await.map_err(|e| err("messages", e))?;

        let count = messages.chunk.len();
        let seq_base = self.ingest_seq.fetch_add(
            count.try_into().unwrap_or(u64::MAX),
            std::sync::atomic::Ordering::Relaxed,
        );
        let mut updates = Vec::with_capacity(count);
        for (i, te) in messages.chunk.iter().enumerate() {
            let Ok(any) = te.raw().deserialize() else {
                continue;
            };
            let matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(mle) = any else {
                continue;
            };
            let matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(m) = mle else {
                continue;
            };
            // Original carries the plaintext body; redacted events have
            // an empty redacted body — both map to the timeline shape.
            // INVARIANT: redacted messages carry an empty redacted body
            // (the payload was stripped); the timeline renders them as
            // blank lines, which is the spec-correct presentation.
            let (sender, body, ts) = match m {
                matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(orig) => (
                    orig.sender.as_str().to_owned(),
                    orig.content.body().to_owned(),
                    u64::from(orig.origin_server_ts.get()),
                ),
                matrix_sdk::ruma::events::SyncMessageLikeEvent::Redacted(red) => (
                    red.sender.as_str().to_owned(),
                    String::new(),
                    u64::from(red.origin_server_ts.get()),
                ),
            };
            // INVARIANT: widen usize->u64 for the seq counter; loop index
            // is bounded by the page limit.
            #[allow(clippy::cast_possible_truncation)]
            updates.push(TimelineUpdate {
                ingested_seq: seq_base + i as u64,
                room: RoomId::new(room_id.as_str()),
                sender,
                body,
                origin_ts_ms: ts,
            });
        }
        updates.reverse();
        Ok(HistoryPage {
            next_from: messages.end,
            updates,
        })
    }

    /// Restore a persisted session from the sqlite store at
    /// `store_path` (same device id, same megolm session keys — no
    /// fresh login). Fails when the store holds no session.
    ///
    /// # Errors
    ///
    /// Fails on client build, missing stored session, or session
    /// restore errors.
    pub async fn restore(homeserver_url: &str, store_path: &str) -> Result<Self, MatrixError> {
        let url = url::Url::parse(homeserver_url).map_err(|e| err("homeserver url", e))?;
        // INVARIANT: a store-built client loads its session (user,
        // tokens, megolm keys) from the store during build — no
        // restore_session call needed here (that API is for sessions
        // held OUTSIDE the store).
        let client = Client::builder()
            .homeserver_url(url)
            .sqlite_store(store_path, None)
            .build()
            .await
            .map_err(|e| err("client build (restore)", e))?;

        let Some(user_id) = client.user_id() else {
            return Err(MatrixError("store holds no session".into()));
        };
        let user_id = user_id.to_owned();
        let device = client
            .device_id()
            .map_or_else(|| "unknown".to_string(), std::string::ToString::to_string);

        Ok(Self::attach(client, user_id.as_str().to_owned(), device))
    }

    /// Create a direct-message room with `peer`: the SDK's native
    /// [`Client::create_dm`] (`is_direct` invite semantics, encryption
    /// state, and the room's direct-target list set for DM discovery).
    /// The precondition for interactive cross-client verification,
    /// which resolves the DM through the room's direct targets.
    ///
    /// # Errors
    ///
    /// Fails on room creation; substrate messages are carried in
    /// [`MatrixError`].
    pub async fn create_dm(&self, peer: &UserId) -> Result<RoomId, MatrixError> {
        let room = self
            .client
            .create_dm(peer)
            .await
            .map_err(|e| err("create_dm", e))?;
        Ok(RoomId::new(room.room_id().as_str()))
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
        self.drain_updates()
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

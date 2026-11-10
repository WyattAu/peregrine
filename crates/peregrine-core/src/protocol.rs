//! Protocol seam: the boundary between peregrine-core and any client
//! substrate. Frontends see these types only; the matrix-sdk adapter
//! (feature `matrix-sdk`) is the first implementation.
//!
//! Ordering invariant (catalog-3 L#5): timeline data carries both the
//! origin server timestamp (metadata, display only) and the client
//! ingestion sequence (the only sort key for layout). Federated clock
//! skew can never reorder the visible timeline.

use core::{fmt, future::Future};

/// Room identifier. Interning to int handles lands with the CMPH pass
/// (catalog-1 B#14); strings here are the seam, not the storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RoomId(String);

impl RoomId {
    /// Wrap a protocol room id (`!room:server`).
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The raw protocol id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One timeline update, already normalized: substrate event types never
/// escape this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineUpdate {
    /// Room the update belongs to.
    pub room: RoomId,
    /// Sender id as the protocol presents it.
    pub sender: String,
    /// Plain-text body (formatted bodies are a later model concern).
    pub body: String,
    /// Origin server timestamp; display metadata only, never a sort key.
    pub origin_ts_ms: u64,
    /// Client ingestion sequence; the only layout ordering.
    pub ingested_seq: u64,
}

/// Receipt for an accepted send: carries our deterministic transaction id
/// (catalog-3 L#2: `peregrine.<device>.<room>.<seq>`), which the sync
/// dedup probe matches against server echoes to suppress "failed to send"
/// false positives on network handoffs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxnReceipt {
    /// The deterministic transaction id assigned at staging time.
    pub txn_id: String,
}

/// A session with a homeserver. Async by nature; implementors are the
/// substrate adapters.
pub trait ProtocolSession {
    /// Substrate error type.
    type Error: fmt::Debug + fmt::Display;

    /// The authenticated user id.
    fn user_id(&self) -> &str;

    /// One bounded sync pass; returns the timeline updates observed.
    fn sync_once(&self) -> impl Future<Output = Result<Vec<TimelineUpdate>, Self::Error>> + Send;

    /// Send a plain-text message; returns the deterministic txn receipt.
    fn send_message(
        &self,
        room: &RoomId,
        body: &str,
    ) -> impl Future<Output = Result<TxnReceipt, Self::Error>> + Send;
}

/// Outbox send state (catalog-3 L#4: STAGED -> DISPATCHED -> ACKED).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendState {
    /// Staged durably, not yet handed to the wire.
    Staged,
    /// Handed to the substrate; ACK not yet observed.
    Dispatched,
    /// Server ACK observed (echo matched by txn id).
    Acked,
}

/// One outbox entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    /// Monotonic local sequence (the txn-id source).
    pub seq: u64,
    /// Destination room.
    pub room: RoomId,
    /// Plain-text body.
    pub body: String,
    /// Deterministic transaction id.
    pub txn_id: String,
    /// Current send state.
    pub state: SendState,
}

/// The crash-consistent outbox kernel. In-memory now; the WAL ring with
/// two-phase commit lands against `peregrine-store` (catalog-3 L#4), and
/// the dual-lane scheduler separates text from chunked media (L#1).
#[derive(Debug)]
pub struct Outbox {
    device: String,
    next_seq: u64,
    entries: Vec<OutboxEntry>,
}

impl Outbox {
    /// New outbox for `device`; the device id namespaces txn ids.
    #[must_use]
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            next_seq: 0,
            entries: Vec::new(),
        }
    }

    fn txn_id(&self, room: &RoomId, seq: u64) -> String {
        format!("peregrine.{}.{}.{}", self.device, room.as_str(), seq)
    }

    /// Stage one send; returns the deterministic transaction id. The id
    /// exists before the wire is touched, so a crash after staging can
    /// never produce a duplicate timeline entry.
    pub fn stage(&mut self, room: RoomId, body: impl Into<String>) -> String {
        let seq = self.next_seq;
        self.next_seq += 1;
        let txn_id = self.txn_id(&room, seq);
        self.entries.push(OutboxEntry {
            seq,
            room,
            body: body.into(),
            txn_id: txn_id.clone(),
            state: SendState::Staged,
        });
        txn_id
    }

    /// Advance an entry by one state (Staged->Dispatched->Acked).
    /// Out-of-order ACKs are legal: idempotent on Acked.
    pub fn advance(&mut self, txn_id: &str) -> Option<SendState> {
        let entry = self.entries.iter_mut().find(|e| e.txn_id == txn_id)?;
        entry.state = match entry.state {
            SendState::Staged => SendState::Dispatched,
            SendState::Dispatched | SendState::Acked => SendState::Acked,
        };
        Some(entry.state)
    }

    /// Dedup probe (L#2): is this server-echoed txn id one of ours? If a
    /// "failed" send's id probes true, the message reached the homeserver;
    /// reconcile, never resend.
    #[must_use]
    pub fn is_ours(&self, txn_id: &str) -> bool {
        self.entries.iter().any(|e| e.txn_id == txn_id)
    }

    /// Entries not yet Acked, in staging order.
    pub fn pending(&self) -> impl Iterator<Item = &OutboxEntry> {
        self.entries.iter().filter(|e| e.state != SendState::Acked)
    }

    /// All entries in staging order (audit + DST inspection).
    #[must_use]
    pub fn entries(&self) -> &[OutboxEntry] {
        &self.entries
    }

    /// Total entries staged this session.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing staged yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)] // tests: panics are the assertion mechanism
mod tests {
    use super::*;

    #[test]
    fn txn_ids_are_deterministic_and_monotonic() {
        let mut ob = Outbox::new("DEV");
        let room = RoomId::new("!r:server");
        let a = ob.stage(room.clone(), "one");
        let b = ob.stage(room.clone(), "two");
        assert_eq!(a, "peregrine.DEV.!r:server.0");
        assert_eq!(b, "peregrine.DEV.!r:server.1");
    }

    #[test]
    fn advance_is_state_and_idempotent_on_acked() {
        let mut ob = Outbox::new("D");
        let t = ob.stage(RoomId::new("!r:x"), "m");
        assert_eq!(ob.advance(&t), Some(SendState::Dispatched));
        assert_eq!(ob.advance(&t), Some(SendState::Acked));
        assert_eq!(ob.advance(&t), Some(SendState::Acked));
        assert_eq!(ob.advance("peregrine.D.!r:x.999"), None);
        assert!(ob.pending().next().is_none());
    }

    #[test]
    fn dedup_probe_matches_staged_txn_ids_only() {
        let mut ob = Outbox::new("D");
        let t = ob.stage(RoomId::new("!r:x"), "m");
        assert!(ob.is_ours(&t));
        assert!(!ob.is_ours("peregrine.D.!r:x.42"));
    }
}

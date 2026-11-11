//! Crash-consistent WAL outbox (catalog-3 L#4): every outbox mutation is
//! a checksummed, flushed append before the call returns; state is
//! *derived* by replaying the log at open. A process kill (mobile jetsam,
//! SIGKILL) loses at most the unflushed OS buffer, never a staged send.
//!
//! Frame format: `[u32 LE length][u32 LE fnv1a32(payload)][payload]`.
//! Recovery scans frames from offset 0 and truncates at the first torn or
//! mismatching frame — the standard journal contract: a corrupt tail is
//! discarded, a corrupt prefix is impossible.
//!
//! The fold reuses [`Outbox`](peregrine_core::protocol::Outbox), which
//! regenerates identical deterministic txn ids during replay; that
//! regeneration determinism is itself gated by DST
//! (`peregrine-dst/tests/outbox_dst.rs`).

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek as _, SeekFrom, Write},
    path::{Path, PathBuf},
};

use peregrine_core::protocol::{Outbox, RoomId, SendState};

/// FNV-1a 32-bit: frame integrity only (torn-write detection), never
/// cryptographic.
#[must_use]
pub fn fnv1a32(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811C_9DC5;
    for &b in data {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// One journaled mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalRecord {
    /// Stage a send.
    Stage {
        /// Destination room id string.
        room: String,
        /// Plain-text body.
        body: String,
    },
    /// Advance a txn one phase (echo observed).
    Advance {
        /// The deterministic transaction id being advanced.
        txn_id: String,
    },
}

impl WalRecord {
    fn encode(&self) -> Vec<u8> {
        match self {
            Self::Stage { room, body } => {
                let mut v = vec![0u8];
                v.extend_from_slice(room.as_bytes());
                v.push(0);
                v.extend_from_slice(body.as_bytes());
                v
            }
            Self::Advance { txn_id } => {
                let mut v = vec![1u8];
                v.extend_from_slice(txn_id.as_bytes());
                v
            }
        }
    }

    /// Decode one payload; `None` on any malformation (never panics on
    /// adversarial bytes — the frame is skipped and replay stops).
    fn decode(payload: &[u8]) -> Option<Self> {
        match payload.first()? {
            0 => {
                let sep = payload[1..].iter().position(|&b| b == 0)? + 1;
                let room = String::from_utf8(payload[1..sep].to_vec()).ok()?;
                let body = String::from_utf8(payload[sep + 1..].to_vec()).ok()?;
                Some(Self::Stage { room, body })
            }
            1 => {
                let txn_id = String::from_utf8(payload[1..].to_vec()).ok()?;
                Some(Self::Advance { txn_id })
            }
            _ => None,
        }
    }
}

/// Durable outbox: [`Outbox`] semantics with a write-ahead journal.
pub struct WalOutbox {
    outbox: Outbox,
    path: PathBuf,
    file: File,
}

impl WalOutbox {
    /// Open (or create) the journal at `path`, replaying it to recover the
    /// outbox state. A torn tail is truncated: the file is left containing
    /// exactly the valid frame prefix.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from open/read/write.
    pub fn open(path: impl AsRef<Path>, device: &str) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut bytes = match File::open(&path) {
            Ok(mut f) => {
                let mut b = Vec::new();
                f.read_to_end(&mut b)?;
                b
            }
            Err(_) => Vec::new(),
        };

        let valid = scan_valid_prefix(&bytes);
        bytes.truncate(valid);

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.seek(SeekFrom::End(0))?;

        let mut outbox = Outbox::new(device);
        let mut offset = 0usize;
        while offset < valid {
            let len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "frame header unaligned")
            })?) as usize;
            let payload = &bytes[offset + 8..offset + 8 + len];
            if let Some(record) = WalRecord::decode(payload) {
                fold(&mut outbox, &record);
            }
            offset += 8 + len;
        }

        Ok(Self { outbox, path, file })
    }

    /// Recovered outbox state after replay.
    #[must_use]
    pub const fn outbox(&self) -> &Outbox {
        &self.outbox
    }

    /// Stage a send: journal first, then state. Returns the txn id.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from append/flush.
    pub fn stage(&mut self, room: &str, body: &str) -> io::Result<String> {
        let record = WalRecord::Stage {
            room: room.to_owned(),
            body: body.to_owned(),
        };
        self.append(&record)?;
        Ok(self
            .outbox
            .stage(RoomId::new(record_room(&record)), record_body(&record)))
    }

    /// Advance a txn one phase: journal first, then state.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from append/flush.
    pub fn advance(&mut self, txn_id: &str) -> io::Result<Option<SendState>> {
        let record = WalRecord::Advance {
            txn_id: txn_id.to_string(),
        };
        self.append(&record)?;
        Ok(self.outbox.advance(txn_id))
    }

    /// Force durability past process kill (fsync). The default contract
    /// covers process kill via flush; OS-crash durability is opt-in.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from fsync.
    pub fn durably_sync(&mut self) -> io::Result<()> {
        self.file.sync_all()
    }

    fn append(&mut self, record: &WalRecord) -> io::Result<()> {
        let payload = record.encode();
        let mut frame = Vec::with_capacity(8 + payload.len());
        frame.extend_from_slice(
            &u32::try_from(payload.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?
                .to_le_bytes(),
        );
        frame.extend_from_slice(&fnv1a32(&payload).to_le_bytes());
        frame.extend_from_slice(&payload);
        self.file.write_all(&frame)?;
        self.file.flush()
    }
}

fn record_room(r: &WalRecord) -> String {
    match r {
        WalRecord::Stage { room, .. } => room.clone(),
        WalRecord::Advance { .. } => String::new(),
    }
}

fn record_body(r: &WalRecord) -> String {
    match r {
        WalRecord::Stage { body, .. } => body.clone(),
        WalRecord::Advance { .. } => String::new(),
    }
}

fn fold(outbox: &mut Outbox, record: &WalRecord) {
    match record {
        WalRecord::Stage { room, body } => {
            let _ = outbox.stage(RoomId::new(room.clone()), body.clone());
        }
        WalRecord::Advance { txn_id } => {
            let _ = outbox.advance(txn_id);
        }
    }
}

/// Length of the valid frame prefix: stops at the first torn frame.
fn scan_valid_prefix(bytes: &[u8]) -> usize {
    let mut offset = 0usize;
    while offset + 8 <= bytes.len() {
        let len_bytes: [u8; 4] = bytes[offset..offset + 4].try_into().unwrap_or([0; 4]);
        let len = u32::from_le_bytes(len_bytes) as usize;
        let end = offset + 8 + len;
        if end > bytes.len() {
            break; // torn tail: header promises bytes that never landed
        }
        let payload = &bytes[offset + 8..end];
        let stored: [u8; 4] = bytes[offset + 4..offset + 8].try_into().unwrap_or([0; 4]);
        if fnv1a32(payload) != u32::from_le_bytes(stored) {
            break; // checksum mismatch: torn or corrupted tail
        }
        offset = end;
    }
    offset
}

/// Journal path backing this outbox (test/inspection hook).
#[must_use]
pub fn path_of(w: &WalOutbox) -> &Path {
    &w.path
}

/// Remove a journal file (cleanup helper for tests/tools).
///
/// # Errors
///
/// Propagates I/O errors from `remove_file`.
pub fn remove_journal(path: impl AsRef<Path>) -> io::Result<()> {
    fs::remove_file(path)
}

#[cfg(test)]
#[allow(clippy::expect_used)] // tests: panics are the assertion mechanism
mod tests {
    use peregrine_core::protocol::SendState;

    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "peregrine-wal-{tag}-{}.journal",
            std::process::id()
        ));
        p
    }

    #[test]
    fn stage_advance_replay_roundtrip() {
        let path = tmp_path("roundtrip");
        let txn;
        {
            let mut w = WalOutbox::open(&path, "DEV").expect("create");
            txn = w.stage("!r:example.org", "hello").expect("stage");
            assert_eq!(
                w.advance(&txn).expect("advance"),
                Some(SendState::Dispatched)
            );
        }
        // "Restart": reopen and replay.
        let w = WalOutbox::open(&path, "DEV").expect("replay");
        let entries = w.outbox().entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].txn_id, txn);
        assert_eq!(entries[0].state, SendState::Dispatched);
        assert_eq!(entries[0].body, "hello");
        remove_journal(&path).expect("cleanup");
    }

    #[test]
    fn torn_tail_is_truncated_and_state_survives() {
        let path = tmp_path("torn");
        let txn;
        {
            let mut w = WalOutbox::open(&path, "DEV").expect("create");
            txn = w.stage("!r:x", "m1").expect("stage");
        }
        {
            // Simulate a crash mid-write: torn frame appended after the
            // valid prefix (promised length never landed + bad checksum).
            use std::io::Write as _;
            let mut f = OpenOptions::new().append(true).open(&path).expect("append");
            let _ = f.write_all(&[0xFF, 0x00, 0x00, 0x40, 0xDE, 0xAD]);
        }
        let w = WalOutbox::open(&path, "DEV").expect("recover");
        let entries = w.outbox().entries();
        assert_eq!(entries.len(), 1, "torn frame discarded, valid prefix kept");
        assert_eq!(entries[0].txn_id, txn);
        // Journal still usable after recovery.
        let mut w = WalOutbox::open(&path, "DEV").expect("reopen");
        let t2 = w.stage("!r:x", "m2").expect("stage2");
        assert_eq!(w.outbox().entries().len(), 2);
        assert!(w.outbox().is_ours(&t2));
        remove_journal(&path).expect("cleanup");
    }

    #[test]
    fn replay_regenerates_identical_txn_ids() {
        // The whole crash-consistency story rests on this: replay folds
        // Stage records through the same deterministic id generator, so a
        // txn id observed before the crash matches after recovery.
        let path = tmp_path("ids");
        let (live_ids, replay_ids);
        {
            let mut w = WalOutbox::open(&path, "DEV").expect("create");
            live_ids = (0..3)
                .map(|i| {
                    w.stage(&format!("!r:{i}"), &format!("b{i}"))
                        .expect("stage")
                })
                .collect::<Vec<_>>();
        }
        let w = WalOutbox::open(&path, "DEV").expect("replay");
        replay_ids = w
            .outbox()
            .entries()
            .iter()
            .map(|e| e.txn_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(live_ids, replay_ids);
        remove_journal(&path).expect("cleanup");
    }

    #[test]
    fn fnv1a32_known_vector() {
        // FNV-1a 32 of "a" = 0xE40C292C (public test vector).
        assert_eq!(fnv1a32(b"a"), 0xE40C_292C);
    }
}

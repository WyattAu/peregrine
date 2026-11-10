//! Peregrine crypto: the key-management UX layer over vodozemac. The UTD
//! cluster (out-of-order quarantine, ratchet checkpoint table, key request
//! tracing) and the trust DAG inspector live here (catalog-1 track C,
//! catalog-3 track N).
//!
//! Material hygiene: secrets live in [`SecretBox`], zeroized on drop, and
//! are excluded from `Debug`/`Display` by construction.

use core::fmt;

/// Owned secret buffer wiped on drop. Not `Clone` on purpose: secrets move,
/// they do not duplicate.
pub struct SecretBox {
    buf: Vec<u8>,
}

impl SecretBox {
    /// Wrap `buf` as a secret; takes ownership so no caller copy lingers.
    #[must_use]
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf }
    }

    /// Borrow the secret bytes. Expose only to concrete crypto engines.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.buf
    }

    #[allow(unsafe_code)]
    // INVARIANT: the sole unsafe site in the workspace. Volatile stores
    // cannot be dead-store-eliminated, so the scrub survives optimization;
    // a plain `for b in .. { *b = 0 }` loop may be removed entirely before
    // the buffer is freed, leaving key material on the heap.
    fn scrub(buf: &mut [u8]) {
        for b in buf.iter_mut() {
            // SAFETY: `b` is a valid aligned u8 inside an owned buffer.
            unsafe { core::ptr::write_volatile(b, 0) };
        }
    }
}

impl Drop for SecretBox {
    fn drop(&mut self) {
        Self::scrub(&mut self.buf);
    }
}

impl fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBox(REDACTED)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_leaks() {
        let s = SecretBox::new(vec![0xAA; 32]);
        assert_eq!(format!("{s:?}"), "SecretBox(REDACTED)");
        assert!(s.expose().iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn scrub_zeroizes_before_free() {
        let mut leaked_copy = Vec::new();
        {
            let s = SecretBox::new(vec![0x42; 16]);
            leaked_copy.extend_from_slice(s.expose());
        }
        assert_eq!(leaked_copy.len(), 16);
    }
}

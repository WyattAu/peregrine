//! Peregrine crypto: the key-management UX layer over vodozemac. The UTD
//! cluster (out-of-order quarantine, ratchet checkpoint table, key request
//! tracing) and the trust DAG inspector live here (catalog-1 track C,
//! catalog-3 track N).
//!
//! Material hygiene: secrets live in [`secret::SecretBox`], zeroized on
//! drop, and are excluded from `Debug`/`Display` by construction.

pub mod keyreq;
pub mod secret;

pub use secret::SecretBox;

//! Peregrine core: the sync pipeline, outbox, relations engine and
//! notification model live here. Protocol logic lives in the matrix-sdk
//! substrate below this crate; frontends never touch protocol types.
//!
//! Determinism first: wall-clock time enters only through
//! [`clock::Clock`], which the DST harness replaces with a virtual clock
//! (architecture: budgets as release gates, docs/GOALS.md SLO table).

pub mod clock;
pub mod protocol;

#[cfg(feature = "matrix-sdk")]
pub mod adapter_matrix;

/// Crate purpose version, reported by the `peregrine` binary.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

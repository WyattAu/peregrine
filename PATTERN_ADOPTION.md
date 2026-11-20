# Pattern adoption

| Pattern | Status | Evidence |
|---|---|---|
| T36 | adopted | crates/peregrine-dst/src/lib.rs:1 (deterministic simulation crate: seeded SplitMix64, Fault injection, Scenario) |
| T21 | adopted | crates/peregrine-store/src/wal.rs:1 (state derived by replaying a checksummed, flushed append log) |
| T32 | partial | crates/peregrine-store/src/wal.rs:91 (WalOutbox: crash-consistent send outbox; the CDC pairing half is out of scope) |
| P4 | adopted | crates/peregrine-crypto/src/secret.rs:1 (zeroize-on-drop, redacted Debug, no Clone) |

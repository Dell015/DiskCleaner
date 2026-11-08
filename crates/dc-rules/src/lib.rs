//! Rules engine: TOML rule packs that set a deterministic risk level and reason codes.
//!
//! **SAFETY-CRITICAL.** See SAFETY.md. No `unwrap`, `expect` or `panic!` in non-test code.
//!
//! Status: stub (Phase B0). Implemented in Phase B4.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

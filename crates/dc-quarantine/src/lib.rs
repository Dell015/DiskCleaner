//! Quarantine and restore: reversible removal with a manifest (feature-gated, off by default).
//!
//! **SAFETY-CRITICAL.** See SAFETY.md. No `unwrap`, `expect` or `panic!` in non-test code.
//!
//! Status: stub (Phase B0). Implemented in Phase B9.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

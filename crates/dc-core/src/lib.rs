//! Shared building blocks used by every other crate.
//!
//! Status: Phase B0 provides only the [`Risk`] type, [`CoreError`] and [`SCHEMA_VERSION`].
//! The scanner types (`Node`, `Tree`) arrive in Phase B1.
//!
//! This crate is held to the safety-critical lint rules: no `unwrap`, `expect` or `panic!`
//! in non-test code. See `SAFETY.md`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod error;
mod risk;

pub use error::CoreError;
pub use risk::Risk;

/// Version of persisted and API-visible schemas. Bumped on every breaking change.
pub const SCHEMA_VERSION: u32 = 1;

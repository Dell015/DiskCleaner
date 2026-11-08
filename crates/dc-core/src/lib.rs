//! Shared building blocks used by every other crate.
//!
//! Contents: the [`Risk`] type, the scan data model ([`Node`], [`Tree`], [`ScanResult`]),
//! simple queries over a tree, [`CoreError`] and [`SCHEMA_VERSION`].
//!
//! This crate is held to the safety-critical lint rules: no `unwrap`, `expect` or `panic!`
//! in non-test code. See `SAFETY.md`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod error;
mod node;
pub mod query;
mod result;
mod risk;
mod tree;

pub use error::CoreError;
pub use node::{Aggregate, Category, Flags, Kind, Node, NodeId, Timestamp};
pub use result::{ScanErrorRecord, ScanResult, ScanStatus, StoredOptions, Totals};
pub use risk::Risk;
pub use tree::Tree;

/// Version of persisted and API-visible schemas. Bumped on every breaking change.
pub const SCHEMA_VERSION: u32 = 1;

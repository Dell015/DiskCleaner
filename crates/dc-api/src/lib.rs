//! What a user interface needs from the backend, in one place:
//!
//! - [`live`]: scan a folder and stream what is found as events, so a view can grow while the
//!   scan runs, then the analysis (rule candidates, leftovers, owners).
//! - [`view`]: the compact `dc-view` file (also written by `dc view-data`).
//! - [`analysis`]: rules, attribution and leftovers for one scan, as marks a view can show.
//! - [`system`]: things read from this computer (protected list, installed apps).
//! - [`dupes`]: exact duplicate files in a finished scan, on request (reads files, changes nothing).
//!
//! - [`inspect`], [`advice`], [`report`]: questions about a finished scan, answered from its view
//!   file, and the scan report.
//! - [`mcp`]: a Model Context Protocol server so a person's own AI assistant can read the scan.
//! - [`datadir`]: DiskCleaner's own data folder (latest scan, report, AI access, AI log).
//!
//! Read-only on everything scanned: nothing here deletes, moves or changes a scanned file. The
//! only writes are inside DiskCleaner's own data folder ([`datadir`]).

#![forbid(unsafe_code)]

pub mod advice;
pub mod analysis;
pub mod cleanup;
pub mod datadir;
pub mod dupes;
pub mod inspect;
pub mod live;
pub mod mcp;
pub mod report;
pub mod system;
pub mod view;

mod error;

pub use error::ApiError;

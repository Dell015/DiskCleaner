//! What a user interface needs from the backend, in one place:
//!
//! - [`live`]: scan a folder and stream what is found as events, so a view can grow while the
//!   scan runs, then the analysis (rule candidates, leftovers, owners).
//! - [`view`]: the compact `dc-view` file (also written by `dc view-data`).
//! - [`analysis`]: rules, attribution and leftovers for one scan, as marks a view can show.
//! - [`system`]: things read from this computer (protected list, installed apps).
//! - [`dupes`]: exact duplicate files in a finished scan, on request (reads files, changes nothing).
//!
//! Read-only: nothing here deletes, moves or writes anything on disk.

#![forbid(unsafe_code)]

pub mod analysis;
pub mod dupes;
pub mod live;
pub mod system;
pub mod view;

mod error;

pub use error::ApiError;

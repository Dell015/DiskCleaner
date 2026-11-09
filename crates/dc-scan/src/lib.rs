//! Read-only directory scanner.
//!
//! Walks one folder tree in parallel and produces a [`dc_core::ScanResult`]. Links are never
//! followed, hardlinks are counted once, unreadable entries are collected instead of aborting,
//! and nothing is ever written, renamed or deleted.
//!
//! Everything the standard library cannot give portably goes through [`FileProbe`]; the Windows
//! implementation lives in `dc-winmeta` so this crate can stay `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

mod attrs;
mod error;
mod options;
mod probe;
mod walk;

pub use attrs::flags_from_windows_attributes;
pub use error::ScanError;
pub use options::{CancelToken, Listing, ListingFn, Progress, ProgressFn, ScanOptions};
#[cfg(unix)]
pub use probe::UnixProbe;
pub use probe::{default_probe, FileId, FileProbe, NullProbe, Probe};
pub use walk::scan;

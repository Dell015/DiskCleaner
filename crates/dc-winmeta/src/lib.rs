//! Windows metadata.
//!
//! Phase B1 provides [`WindowsProbe`], the Windows implementation of the scanner's
//! [`dc_scan::FileProbe`]. Later phases add installed apps, known folders, version info and
//! signatures.
//!
//! This is one of the two crates allowed to contain `unsafe` code (the other is `dc-mft`).
//! Every `unsafe` block carries a `// SAFETY:` comment, and nothing here modifies anything:
//! files are opened with attribute-read access only, never with data access.

#[cfg(windows)]
mod probe;

#[cfg(windows)]
pub use probe::WindowsProbe;

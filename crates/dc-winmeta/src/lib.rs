//! Windows metadata.
//!
//! Phase B1 provides [`WindowsProbe`], the Windows implementation of the scanner's
//! [`dc_scan::FileProbe`]. Phase B2 adds known folders and installed apps (read from the
//! registry), built behind the [`SystemInfoSource`] trait so all logic is testable with a
//! [`FakeSource`] on any platform. Later steps add shortcuts, version info and signatures.
//!
//! This is one of the two crates allowed to contain `unsafe` code (the other is `dc-mft`),
//! and only the Windows modules may use it. Every `unsafe` block carries a `// SAFETY:`
//! comment, and nothing here modifies anything: files are opened with attribute-read access
//! only, and the registry is opened with read-only rights.

#![deny(unsafe_code)]

mod apps;
mod model;
mod parse;
mod source;

#[cfg(windows)]
#[allow(unsafe_code)]
mod probe;
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

pub use apps::build_snapshot;
pub use model::{
    AppFlags, AppSnapshot, FolderOrigin, InstalledApp, KnownFolder, Problem, RawUninstallEntry,
    RegistrySource, SNAPSHOT_SCHEMA_VERSION,
};
pub use parse::{
    derive_install_folder, expand_env, is_absolute, is_too_broad, normalize_folder, parent_folder,
    path_from_command, path_key,
};
pub use source::{FakeSource, SourceReport, SystemInfoSource};

#[cfg(windows)]
pub use probe::WindowsProbe;
#[cfg(windows)]
pub use windows::WindowsSource;

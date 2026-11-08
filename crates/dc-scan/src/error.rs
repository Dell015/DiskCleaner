//! Errors that stop a scan before it starts. Problems with individual entries are not errors:
//! they are collected in the result.

use std::path::PathBuf;

/// Why a scan could not start.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScanError {
    /// The root does not exist or cannot be read.
    #[error("cannot read scan root {path}: {source}")]
    Root {
        /// The root as given.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The root is not a directory.
    #[error("scan root is not a directory: {0}")]
    NotADirectory(PathBuf),
    /// The root is itself a link; links are never followed.
    #[error("scan root is a link, and links are never followed: {0}")]
    RootIsLink(PathBuf),
    /// The root is a drive or file-system root and the caller did not opt in.
    #[error("refusing to scan a drive root without explicit permission: {0}")]
    DriveRootRefused(PathBuf),
    /// The tree grew beyond what the id type can hold.
    #[error("scan produced too many entries")]
    TooManyEntries,
}

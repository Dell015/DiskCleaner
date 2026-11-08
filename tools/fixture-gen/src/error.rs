//! Errors for the fixture generator.

use std::path::PathBuf;

/// Everything that can stop the generator.
///
/// Skippable conditions (for example "symlinks need a privilege we do not hold") are **not**
/// errors: they are recorded in the manifest as `skipped` entries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GenError {
    /// The output directory exists and already holds something.
    #[error("output directory is not empty: {0}")]
    OutDirNotEmpty(PathBuf),
    /// The output directory is a drive root or the file system root.
    #[error("refusing to write to a drive or file system root: {0}")]
    OutDirIsRoot(PathBuf),
    /// The output directory is a symlink or junction; we never write through links.
    #[error("output directory is a link, refusing to write through it: {0}")]
    OutDirIsLink(PathBuf),
    /// The output path exists but is not a directory.
    #[error("output path exists and is not a directory: {0}")]
    OutDirNotADirectory(PathBuf),
    /// The output path could not be resolved to a real location (fail closed).
    #[error("output path could not be resolved: {0}")]
    OutDirUnresolvable(PathBuf),
    /// The output path contains a `..` component.
    #[error("output path must not contain `..`: {0}")]
    OutDirHasParentRef(PathBuf),
    /// The generator's own layout used an unusable entry name (a bug in the layout).
    #[error("invalid entry name in generator layout: {0}")]
    InvalidName(String),
    /// A path that must stay inside the fixture tree did not (a bug in the layout).
    #[error("path escapes the fixture tree: {0}")]
    PathEscapesTree(String),
    /// An I/O operation failed.
    #[error("{context} failed for {path}: {source}")]
    Io {
        /// What the generator was doing.
        context: &'static str,
        /// The entry, relative to the fixture tree.
        path: String,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// Build a closure that wraps an [`std::io::Error`] into [`GenError::Io`].
pub(crate) fn io_err<'a>(
    context: &'static str,
    path: &'a str,
) -> impl FnOnce(std::io::Error) -> GenError + 'a {
    move |source| GenError::Io {
        context,
        path: path.to_owned(),
        source,
    }
}

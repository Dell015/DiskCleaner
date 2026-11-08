//! Typed errors for `dc-core`.

/// Errors produced by `dc-core`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    /// A string did not name a known [`Risk`](crate::Risk) level.
    #[error("invalid risk value: {value:?}")]
    InvalidRisk {
        /// The rejected input, exactly as given.
        value: String,
    },
}

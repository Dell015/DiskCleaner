use thiserror::Error;

/// Errors from the API layer.
#[derive(Debug, Error)]
pub enum ApiError {
    /// The scan could not start or failed.
    #[error(transparent)]
    Scan(#[from] dc_scan::ScanError),
    /// The scan data was inconsistent.
    #[error("the scan data is damaged: {0}")]
    Damaged(#[from] dc_core::CoreError),
    /// More entries than fit in one view.
    #[error("too many entries for one view")]
    TooMany,
}

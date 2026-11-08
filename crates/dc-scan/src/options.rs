//! Scan options, the cancel token and progress reporting.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Asks a running scan to stop. Cheap to clone; all clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Has cancellation been requested?
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A snapshot of how far a scan has got.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Progress {
    /// Files seen so far.
    pub files: u64,
    /// Directories seen so far.
    pub dirs: u64,
    /// Logical bytes seen so far.
    pub bytes: u64,
    /// A directory currently being read (best effort).
    pub current_dir: String,
}

/// Receives throttled [`Progress`] snapshots, from a helper thread.
pub type ProgressFn = Arc<dyn Fn(&Progress) + Send + Sync>;

/// How to scan.
#[derive(Clone)]
pub struct ScanOptions {
    /// Store only nodes at most this deep (totals still include everything).
    pub max_depth: Option<u32>,
    /// Store only files at least this large (totals still include everything).
    pub min_size: Option<u64>,
    /// Allow scanning a drive or file-system root.
    pub allow_drive_root: bool,
    /// Lets the caller stop the scan.
    pub cancel: CancelToken,
    /// Optional progress callback.
    pub progress: Option<ProgressFn>,
    /// Minimum time between progress callbacks.
    pub progress_interval: Duration,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            min_size: None,
            allow_drive_root: false,
            cancel: CancelToken::new(),
            progress: None,
            progress_interval: Duration::from_millis(100),
        }
    }
}

impl std::fmt::Debug for ScanOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScanOptions")
            .field("max_depth", &self.max_depth)
            .field("min_size", &self.min_size)
            .field("allow_drive_root", &self.allow_drive_root)
            .field("cancelled", &self.cancel.is_cancelled())
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

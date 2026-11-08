//! The stored form of a scan: what `dc scan` writes and `dc top` reads.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::node::Node;
use crate::tree::Tree;
use crate::SCHEMA_VERSION;

/// How the scan ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    /// Everything was read.
    Complete,
    /// Finished, but some entries could not be read (see `errors`).
    Partial,
    /// Stopped early on request.
    Cancelled,
}

/// One entry the scanner could not read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanErrorRecord {
    /// Path as scanned.
    pub path: String,
    /// What was being attempted (`read_dir`, `metadata`, `probe`, ...).
    pub op: String,
    /// Error class (`permission_denied`, `not_found`, ...).
    pub kind: String,
    /// The operating system's message.
    pub message: String,
}

/// The options that shaped what is stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredOptions {
    /// Directories deeper than this were not stored (totals still include them).
    pub max_depth: Option<u32>,
    /// Files smaller than this were not stored (totals still include them).
    pub min_size: Option<u64>,
}

/// Headline numbers, computed over everything scanned (not just what is stored).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// Counted files.
    pub files: u64,
    /// Directories below the root.
    pub dirs: u64,
    /// Links (never followed).
    pub links: u64,
    /// Sum of logical sizes of counted files.
    pub logical_bytes: u64,
    /// Sum of sizes on disk of counted files.
    pub size_on_disk_bytes: u64,
    /// Extra hardlink paths that were not counted again.
    pub hardlink_extra_paths: u64,
    /// Entries that could not be read.
    pub errors: u64,
}

/// A finished scan, ready to save as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanResult {
    /// Format version of this file.
    pub schema_version: u32,
    /// Version of the tool that wrote it.
    pub tool_version: String,
    /// The scanned path, as the user gave it.
    pub root: String,
    /// Start time, seconds since the Unix epoch (UTC).
    pub started_unix: i64,
    /// End time, seconds since the Unix epoch (UTC).
    pub finished_unix: i64,
    /// How it ended.
    pub status: ScanStatus,
    /// Storage options in effect.
    pub options: StoredOptions,
    /// Headline numbers.
    pub totals: Totals,
    /// Entries that could not be read.
    pub errors: Vec<ScanErrorRecord>,
    /// The stored nodes, root first.
    pub nodes: Vec<Node>,
}

impl ScanResult {
    /// Rebuild the [`Tree`], validating it. Refuses files from a newer schema.
    pub fn tree(&self) -> Result<Tree, CoreError> {
        if self.schema_version > SCHEMA_VERSION {
            return Err(CoreError::InvalidTree {
                reason: format!(
                    "scan file schema {} is newer than supported {SCHEMA_VERSION}",
                    self.schema_version
                ),
            });
        }
        Tree::from_nodes(self.nodes.clone())
    }
}

//! The journal of one cleanup in one holding folder: one JSON object per line, appended and
//! flushed before each step. The state of a cleanup is whatever its journal says, read back.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What an item looked like when it was held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Where it was.
    pub original: PathBuf,
    /// Where it is held.
    pub held: PathBuf,
    /// A folder.
    pub is_dir: bool,
    /// Bytes (file lengths).
    pub bytes: u64,
    /// Files (1 for a file).
    pub files: u64,
    /// "safe", "probably_safe" or "your_call".
    pub group: String,
    /// Short title.
    pub title: String,
    /// Reason id.
    pub reason: String,
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Line {
    /// The cleanup started.
    Start {
        /// When (Unix seconds).
        time: i64,
    },
    /// About to move item `n` in one step.
    Hold {
        /// Item number.
        n: u32,
        /// What it is.
        record: Record,
    },
    /// Item `n` moved.
    Held {
        /// Item number.
        n: u32,
    },
    /// Item `n` could not be moved as a whole; its files are moved one by one.
    Split {
        /// Item number.
        n: u32,
    },
    /// About to move one file of a split item (path relative to the item).
    File {
        /// Item number.
        n: u32,
        /// Relative path.
        rel: String,
    },
    /// That file moved.
    FileHeld {
        /// Item number.
        n: u32,
        /// Relative path.
        rel: String,
    },
    /// Item `n` was not moved (nothing of it, or nothing more).
    Failed {
        /// Item number.
        n: u32,
        /// Why.
        why: String,
    },
    /// About to move item `n` back to `to`.
    Restore {
        /// Item number.
        n: u32,
        /// Where to.
        to: PathBuf,
    },
    /// Item `n` is back.
    Restored {
        /// Item number.
        n: u32,
        /// Where it went.
        to: PathBuf,
    },
    /// About to delete the held items for good.
    Empty {
        /// When.
        time: i64,
    },
    /// The held items are gone.
    Emptied {
        /// When.
        time: i64,
    },
}

/// Read a journal. Lines that cannot be read (a crash in the middle of a write) are ignored:
/// the step they announced is then judged by looking at the disk.
pub fn read(path: &Path) -> Vec<Line> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// A line as text.
pub fn text(line: &Line) -> String {
    serde_json::to_string(line).unwrap_or_default()
}

//! The few operations that change the disk, behind a trait so tests can make them fail at any
//! step (disk full, access denied, a crash half-way).

// This file is the one place allowed to call rename and remove (SAFETY.md rule 2).
#![allow(clippy::disallowed_methods)]

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Operations that change the disk.
pub trait Ops: Sync {
    /// Rename (move within one drive). Never copies.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Create a folder and its parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Append one line and make sure it reached the disk before returning.
    fn append_line(&self, path: &Path, line: &str) -> io::Result<()>;
    /// Delete a folder and everything in it, without following links. Only ever called on a
    /// folder inside a holding folder (checked by the caller).
    fn remove_tree(&self, path: &Path) -> io::Result<()>;
}

/// The real disk.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealOps;

impl Ops for RealOps {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn append_line(&self, path: &Path, line: &str) -> io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_data()
    }

    fn remove_tree(&self, path: &Path) -> io::Result<()> {
        // std's remove_dir_all does not follow links (it removes the link itself).
        fs::remove_dir_all(path)
    }
}

//! The few operations that change the disk, behind a trait so tests can make them fail at any
//! step (disk full, access denied, a crash half-way).

// This file is the one place allowed to call rename and remove (SAFETY.md rule 2).
#![allow(clippy::disallowed_methods)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Operations that change the disk.
pub trait Ops: Sync {
    /// Move within one drive. Never copies, and never replaces something already at `to`
    /// (that is an error). On Windows the item is opened first and its real location checked,
    /// so a folder above it that was swapped for a link cannot redirect the move.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Create a folder and its parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Append one line and make sure it reached the disk before returning. The line and its
    /// line break are written in one piece, after a line break if the file does not end with
    /// one (so a damaged last line never swallows the next).
    fn append_line(&self, path: &Path, line: &str) -> io::Result<()>;
    /// Delete a folder and everything in it, without following links. Only ever called on a
    /// folder inside a holding folder (checked by the caller).
    fn remove_tree(&self, path: &Path) -> io::Result<()>;
    /// A new holding folder: hidden, and on Windows readable only by its owner, SYSTEM and
    /// administrators. Best effort (drives without permissions, like FAT, cannot do it).
    fn protect_dir(&self, path: &Path) -> io::Result<()>;
}

/// The real disk.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealOps;

impl Ops for RealOps {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            crate::win::move_checked(from, to)
        }
        #[cfg(not(windows))]
        {
            // Other systems are only used for tests; a plain rename after checking the target.
            if fs::symlink_metadata(to).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "something is already there",
                ));
            }
            fs::rename(from, to)
        }
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn append_line(&self, path: &Path, line: &str) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)?;
        let mut text = Vec::with_capacity(line.len() + 2);
        if file.metadata()?.len() > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::End(-1))?;
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                text.push(b'\n');
            }
        }
        text.extend_from_slice(line.as_bytes());
        text.push(b'\n');
        // Append mode: this always lands at the end, whatever the position after the read.
        file.write_all(&text)?;
        file.sync_data()
    }

    fn remove_tree(&self, path: &Path) -> io::Result<()> {
        // std's remove_dir_all does not follow links (it removes the link itself).
        fs::remove_dir_all(path)
    }

    fn protect_dir(&self, path: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            crate::win::hide_and_protect(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Ok(())
        }
    }
}

/// Hold the lock that lets only one removal, restore or emptying run at a time (also across
/// two DiskCleaner windows). Released when the returned file is dropped or the program ends.
/// `Ok(None)`: someone else holds it.
pub fn try_lock(path: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

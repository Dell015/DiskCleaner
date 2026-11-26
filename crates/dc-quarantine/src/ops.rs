//! The few operations that change the disk, behind a trait so tests can make them fail at any
//! step (disk full, access denied, a crash half-way).

// This file is the one place allowed to call rename and remove (SAFETY.md rule 2).
#![allow(clippy::disallowed_methods)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

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

/// What a drive is, for moving an app onto it (B11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Volume {
    /// File system name ("NTFS", "exFAT", "FAT32", ...); empty when unknown.
    pub filesystem: String,
    /// Bytes free for this user.
    pub free: u64,
    /// Bytes in all.
    pub total: u64,
    /// A USB stick or other removable drive.
    pub removable: bool,
    /// A mapped network drive.
    pub network: bool,
}

/// The extra operations moving an app needs (B11, `mover`). Kept apart from [`Ops`] so removing
/// cannot reach them.
pub trait MoveOps: Ops {
    /// Copy one file's bytes (and, where the system does it, its attributes and times) to a
    /// new file `to`. Never replaces: `to` must not exist.
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Make a new folder (one level; it must not exist).
    fn create_dir(&self, path: &Path) -> io::Result<()>;
    /// Make a link at `link` (which must not exist) to the folder `target`: a junction on
    /// Windows, a symbolic link elsewhere (tests only).
    fn make_link(&self, link: &Path, target: &Path) -> io::Result<()>;
    /// Where the link at `link` points; `Ok(None)` when `link` is not a link at all.
    fn read_link(&self, link: &Path) -> io::Result<Option<PathBuf>>;
    /// Remove the link at `link` (never what it points to). Callers check its target first.
    fn remove_link(&self, link: &Path) -> io::Result<()>;
    /// Can this file be opened by Rhizome alone right now? `Err` with "in use" (a sharing
    /// violation) when a program has it open. Read access only; nothing is changed.
    fn open_alone(&self, file: &Path) -> io::Result<()>;
    /// The drive holding `path`.
    fn volume(&self, path: &Path) -> io::Result<Volume>;
    /// Which drive `path` is on (two paths with the same answer are on the same drive); `None`
    /// when it cannot be told. Windows: the drive letter.
    fn drive(&self, path: &Path) -> Option<String>;
}

impl MoveOps for RealOps {
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        if fs::symlink_metadata(to).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "something is already there",
            ));
        }
        // Windows: CopyFileExW (copies attributes and times too).
        fs::copy(from, to).map(|_| ())
    }

    fn create_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }

    fn make_link(&self, link: &Path, target: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            crate::win::make_junction(link, target)
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (link, target);
            Err(io::Error::other("links are not supported here"))
        }
    }

    fn read_link(&self, link: &Path) -> io::Result<Option<PathBuf>> {
        #[cfg(windows)]
        {
            crate::win::read_junction(link)
        }
        #[cfg(not(windows))]
        {
            let meta = fs::symlink_metadata(link)?;
            if !meta.file_type().is_symlink() {
                return Ok(None);
            }
            fs::read_link(link).map(Some)
        }
    }

    fn remove_link(&self, link: &Path) -> io::Result<()> {
        let meta = fs::symlink_metadata(link)?;
        if !crate::check::is_link(&meta) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "it is not a link (nothing was removed)",
            ));
        }
        #[cfg(windows)]
        {
            // A junction is a folder entry: RemoveDirectoryW removes the link, never its target.
            fs::remove_dir(link)
        }
        #[cfg(not(windows))]
        {
            fs::remove_file(link)
        }
    }

    fn open_alone(&self, file: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            crate::win::open_alone(file)
        }
        #[cfg(not(windows))]
        {
            File::open(file).map(|_| ())
        }
    }

    fn drive(&self, path: &Path) -> Option<String> {
        #[cfg(windows)]
        {
            crate::places::drive_of(path).map(String::from)
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // The nearest folder that exists, and its device.
            let mut p = path.to_path_buf();
            loop {
                if let Ok(m) = fs::symlink_metadata(&p) {
                    return Some(m.dev().to_string());
                }
                if !p.pop() {
                    return None;
                }
            }
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = path;
            None
        }
    }

    fn volume(&self, path: &Path) -> io::Result<Volume> {
        #[cfg(windows)]
        {
            crate::win::volume(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            // Tests only: an NTFS-like drive with plenty of room.
            Ok(Volume {
                filesystem: "NTFS".to_owned(),
                free: u64::MAX / 4,
                total: u64::MAX / 2,
                removable: false,
                network: false,
            })
        }
    }
}

/// The drive letters there are right now (Windows; elsewhere none).
#[must_use]
pub fn drive_letters() -> Vec<char> {
    #[cfg(windows)]
    {
        crate::win::drive_letters()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Is drive `letter` a network drive (Windows only; elsewhere never)?
#[must_use]
pub fn is_network_drive(letter: char) -> bool {
    #[cfg(windows)]
    {
        crate::win::is_network_drive(letter)
    }
    #[cfg(not(windows))]
    {
        let _ = letter;
        false
    }
}

/// Start a program or open a settings page with its own window (Windows only; D133: only an
/// app's own uninstaller or the Installed apps page, see `uninstall`).
pub fn shell_open(target: &Path, args: Option<&str>) -> io::Result<()> {
    #[cfg(windows)]
    {
        crate::win::shell_open(target, args)
    }
    #[cfg(not(windows))]
    {
        let _ = (target, args);
        Err(io::Error::other("this only works on Windows"))
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

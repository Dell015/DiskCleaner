//! The seam between the portable walker and the operating system.
//!
//! The walker asks a [`FileProbe`] for the few facts that `std` cannot give portably: the
//! identity of a file (to count hardlinks once) and the space it really occupies. The Windows
//! implementation lives in `dc-winmeta` (the only crate allowed `unsafe` code); this crate has
//! a safe Unix implementation and tests use scripted fakes.

use std::fs::Metadata;
use std::io;
use std::path::Path;

use dc_core::Flags;

/// Identity of a file on a volume: two paths with the same id are the same file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId {
    /// Volume (Windows volume serial, Unix device).
    pub volume: u64,
    /// File index (Windows file index, Unix inode).
    pub index: u128,
}

/// What a probe learned about one regular file.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    /// Identity, when the platform can tell.
    pub file_id: Option<FileId>,
    /// Number of paths that name this file (1 for an ordinary file).
    pub link_count: u32,
    /// Space allocated on disk, when the platform can tell.
    pub size_on_disk: Option<u64>,
    /// Extra flags the probe discovered (for example sparse).
    pub flags: Flags,
}

/// Looks up OS-specific facts about a file. Must never modify anything and must never read
/// file contents.
pub trait FileProbe: Sync {
    /// Probe a regular file. `meta` is the (non-following) metadata the walker already has.
    fn probe(&self, path: &Path, meta: &Metadata) -> io::Result<Probe>;
}

/// A probe that knows nothing: no identity, no allocation size. Sizes on disk fall back to the
/// logical size rounded up to 4 KiB. Used on platforms without a real probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullProbe;

impl FileProbe for NullProbe {
    fn probe(&self, _path: &Path, _meta: &Metadata) -> io::Result<Probe> {
        Ok(Probe {
            link_count: 1,
            ..Probe::default()
        })
    }
}

/// A probe built on the Unix `stat` fields (device, inode, link count, allocated blocks).
#[cfg(unix)]
#[derive(Debug, Clone, Copy, Default)]
pub struct UnixProbe;

#[cfg(unix)]
impl FileProbe for UnixProbe {
    fn probe(&self, _path: &Path, meta: &Metadata) -> io::Result<Probe> {
        use std::os::unix::fs::MetadataExt;
        let allocated = meta.blocks().saturating_mul(512);
        let flags = Flags {
            sparse: meta.len() > 0 && allocated > 0 && allocated < meta.len(),
            ..Flags::default()
        };
        Ok(Probe {
            file_id: Some(FileId {
                volume: meta.dev(),
                index: u128::from(meta.ino()),
            }),
            link_count: u32::try_from(meta.nlink()).unwrap_or(u32::MAX),
            size_on_disk: Some(allocated),
            flags,
        })
    }
}

/// The best probe this crate can offer on the current platform. On Windows, the command-line
/// tool uses the probe from `dc-winmeta` instead.
#[must_use]
pub fn default_probe() -> Box<dyn FileProbe> {
    #[cfg(unix)]
    {
        Box::new(UnixProbe)
    }
    #[cfg(not(unix))]
    {
        Box::new(NullProbe)
    }
}

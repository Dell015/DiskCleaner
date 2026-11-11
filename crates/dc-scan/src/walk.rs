//! The walker. Read-only: it lists directories and asks for metadata, nothing else.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use dc_core::{
    Kind, Node, NodeId, ScanErrorRecord, ScanResult, ScanStatus, StoredOptions, Timestamp, Totals,
    Tree, SCHEMA_VERSION,
};
use rayon::prelude::*;

use crate::attrs::{flags_from_windows_attributes, is_reparse};
use crate::error::ScanError;
use crate::options::{Progress, ScanOptions};
use crate::probe::{FileId, FileProbe};

/// Fallback allocation unit when a probe cannot say how much space a file occupies.
const FALLBACK_CLUSTER: u64 = 4096;

/// A scanned entry before it is placed in the arena.
struct Raw {
    node: Node,
    /// The name as the file system gave it (the node's name is a lossy copy).
    os_name: OsString,
    file_id: Option<FileId>,
    link_count: u32,
    children: Vec<Raw>,
}

struct Shared<'a> {
    probe: &'a dyn FileProbe,
    cancel: &'a crate::CancelToken,
    listing: Option<&'a crate::options::ListingFn>,
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    current_dir: Mutex<String>,
    errors: Mutex<Vec<ScanErrorRecord>>,
    done: AtomicBool,
}

impl Shared<'_> {
    fn record(&self, path: &Path, op: &str, err: &io::Error) {
        let record = ScanErrorRecord {
            path: path.to_string_lossy().into_owned(),
            op: op.to_owned(),
            kind: error_kind_name(err.kind()),
            message: err.to_string(),
        };
        if let Ok(mut errors) = self.errors.lock() {
            errors.push(record);
        }
    }

    fn snapshot(&self) -> Progress {
        Progress {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            current_dir: self
                .current_dir
                .lock()
                .map(|d| d.clone())
                .unwrap_or_default(),
        }
    }
}

fn error_kind_name(kind: io::ErrorKind) -> String {
    // `Debug` of ErrorKind is the variant name, e.g. "PermissionDenied".
    let debug = format!("{kind:?}");
    let mut out = String::new();
    for (i, ch) in debug.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

#[cfg(windows)]
fn attributes(meta: &Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes()
}

#[cfg(not(windows))]
fn attributes(_meta: &Metadata) -> u32 {
    0
}

fn timestamp(time: io::Result<SystemTime>) -> Option<Timestamp> {
    time.ok().and_then(Timestamp::from_system_time)
}

/// Scan `root` and return the result.
///
/// Individual unreadable entries do not fail the scan: they are listed in the result and the
/// status becomes `Partial`. Links are never followed. Nothing is written anywhere.
pub fn scan(
    root: &Path,
    probe: &dyn FileProbe,
    options: &ScanOptions,
) -> Result<ScanResult, ScanError> {
    let started = unix_now();
    let root_err = |source| ScanError::Root {
        path: root.to_path_buf(),
        source,
    };

    let root_meta = fs::symlink_metadata(root).map_err(root_err)?;
    if root_meta.file_type().is_symlink() || is_reparse(attributes(&root_meta)) {
        return Err(ScanError::RootIsLink(root.to_path_buf()));
    }
    if !root_meta.is_dir() {
        return Err(ScanError::NotADirectory(root.to_path_buf()));
    }
    let canonical = fs::canonicalize(root).map_err(root_err)?;
    if canonical.parent().is_none() && !options.allow_drive_root {
        return Err(ScanError::DriveRootRefused(root.to_path_buf()));
    }

    let shared = Shared {
        probe,
        cancel: &options.cancel,
        listing: options.listing.as_ref(),
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        current_dir: Mutex::new(String::new()),
        errors: Mutex::new(Vec::new()),
        done: AtomicBool::new(false),
    };

    let children = std::thread::scope(|scope| {
        if let Some(callback) = &options.progress {
            let (shared, callback) = (&shared, callback.clone());
            let interval = options.progress_interval;
            scope.spawn(move || {
                while !shared.done.load(Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    callback(&shared.snapshot());
                }
                callback(&shared.snapshot());
            });
        }
        let children = scan_dir(&canonical, 0, &shared);
        shared.done.store(true, Ordering::Relaxed);
        children
    });

    let mut root_node = Node::new(root.to_string_lossy().into_owned(), Kind::Dir);
    root_node.mtime = timestamp(root_meta.modified());
    root_node.ctime = timestamp(root_meta.created());
    root_node.atime = timestamp(root_meta.accessed());
    root_node.flags = flags_from_windows_attributes(attributes(&root_meta));

    let (tree, hardlink_extra) = assemble(root_node, children)?;
    let mut tree = tree;
    tree.aggregate();

    let errors = shared.errors.into_inner().unwrap_or_default();
    let status = if options.cancel.is_cancelled() {
        ScanStatus::Cancelled
    } else if errors.is_empty() {
        ScanStatus::Complete
    } else {
        ScanStatus::Partial
    };

    let agg = tree.root().map(|r| r.agg).unwrap_or_default();
    let totals = Totals {
        files: agg.file_count,
        dirs: agg.dir_count,
        links: agg.link_count,
        logical_bytes: agg.total_logical,
        size_on_disk_bytes: agg.total_on_disk,
        hardlink_extra_paths: hardlink_extra,
        errors: errors.len() as u64,
    };

    let stored = if options.max_depth.is_some() || options.min_size.is_some() {
        tree.pruned(|node| {
            options.max_depth.is_none_or(|max| node.depth <= max)
                && (node.kind != Kind::File
                    || options.min_size.is_none_or(|min| node.logical_size >= min))
        })
    } else {
        tree
    };

    Ok(ScanResult {
        schema_version: SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        root: root.to_string_lossy().into_owned(),
        started_unix: started,
        finished_unix: unix_now(),
        status,
        options: StoredOptions {
            max_depth: options.max_depth,
            min_size: options.min_size,
        },
        totals,
        errors,
        nodes: stored.into_nodes(),
        app_labels: Vec::new(),
    })
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// Read one directory and, in parallel, everything below it.
///
/// Two steps: first every entry of this folder is described (in parallel), then the folders among
/// them are read. Between the two, a listing callback (if any) gets this folder's entries, so a
/// live view always hears about a folder before anything inside it.
fn scan_dir(dir: &Path, depth: u32, shared: &Shared<'_>) -> Vec<Raw> {
    if shared.cancel.is_cancelled() {
        return Vec::new();
    }
    shared.dirs.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut current) = shared.current_dir.try_lock() {
        *current = dir.to_string_lossy().into_owned();
    }

    let reader = match fs::read_dir(dir) {
        Ok(reader) => reader,
        Err(err) => {
            shared.record(dir, "read_dir", &err);
            return Vec::new();
        }
    };
    let mut names: Vec<OsString> = Vec::new();
    for entry in reader {
        match entry {
            Ok(entry) => names.push(entry.file_name()),
            Err(err) => shared.record(dir, "read_dir_entry", &err),
        }
    }
    names.sort();
    // DiskCleaner's own holding folders (removed items waiting to be emptied) are not scanned:
    // what is in them was already removed.
    names.retain(|name| !is_holding(dir, &name.to_string_lossy()));

    let mut raws: Vec<Raw> = names
        .par_iter()
        .filter_map(|name| describe(&dir.join(name), name, shared))
        .collect();

    if let Some(listing) = shared.listing {
        if !shared.cancel.is_cancelled() {
            listing(&crate::options::Listing {
                dir: dir.to_string_lossy().into_owned(),
                depth,
                entries: raws.iter().map(|r| r.node.clone()).collect(),
            });
        }
    }

    // Folders where people decide most (Users at the top of a drive; Downloads, Desktop and
    // Documents in a profile) are read first, so they are complete early. The result is the same.
    let first = |raw: &Raw| raw.node.kind == Kind::Dir && read_first(dir, &raw.node.name);
    raws.par_iter_mut()
        .filter(|raw| first(raw))
        .for_each(|raw| {
            raw.children = scan_dir(&dir.join(&raw.os_name), depth + 1, shared);
        });
    raws.par_iter_mut()
        .filter(|raw| raw.node.kind == Kind::Dir && !first(raw))
        .for_each(|raw| {
            raw.children = scan_dir(&dir.join(&raw.os_name), depth + 1, shared);
        });
    raws
}

/// Is `name` (inside `dir`) one of DiskCleaner's holding folders (`X:\\.diskcleaner-holding`, or
/// `...\\AppData\\Local\\DiskCleaner\\Holding`)?
pub(crate) fn is_holding(dir: &Path, name: &str) -> bool {
    if name.eq_ignore_ascii_case(".diskcleaner-holding") {
        return true;
    }
    name.eq_ignore_ascii_case("holding")
        && dir
            .file_name()
            .is_some_and(|d| d.to_string_lossy().eq_ignore_ascii_case("diskcleaner"))
        && dir
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|l| l.to_string_lossy().eq_ignore_ascii_case("local"))
}

/// Is `name` (inside `dir`) one of the folders read before its siblings?
pub(crate) fn read_first(dir: &Path, name: &str) -> bool {
    let lower = name.to_lowercase();
    // The top of a drive (or file-system root): Users first.
    if dir.parent().is_none() {
        return lower == "users";
    }
    // A profile (`...\Users\<name>`): the folders people sort through.
    let in_profile = dir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case("users"));
    in_profile && matches!(lower.as_str(), "downloads" | "desktop" | "documents")
}

/// Describe one directory entry (not what is inside it).
fn describe(path: &Path, name: &OsString, shared: &Shared<'_>) -> Option<Raw> {
    if shared.cancel.is_cancelled() {
        return None;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) => {
            shared.record(path, "metadata", &err);
            return None;
        }
    };
    let attrs = attributes(&meta);
    let file_type = meta.file_type();

    let mut node = Node::new(name.to_string_lossy().into_owned(), Kind::Other);
    node.mtime = timestamp(meta.modified());
    node.ctime = timestamp(meta.created());
    node.atime = timestamp(meta.accessed());
    node.flags = flags_from_windows_attributes(attrs);

    let mut raw = Raw {
        node,
        os_name: name.clone(),
        file_id: None,
        link_count: 1,
        children: Vec::new(),
    };

    if file_type.is_symlink() || (file_type.is_dir() && is_reparse(attrs)) {
        // Symlinks, junctions and any other reparse-point directory: recorded, never followed.
        raw.node.kind = Kind::Link;
        raw.node.flags.reparse = true;
        raw.node.link_target = fs::read_link(path)
            .ok()
            .map(|t| t.to_string_lossy().into_owned());
    } else if file_type.is_dir() {
        raw.node.kind = Kind::Dir;
    } else if file_type.is_file() {
        raw.node.kind = Kind::File;
        raw.node.logical_size = meta.len();
        raw.node.set_extension_from_name();
        match shared.probe.probe(path, &meta) {
            Ok(probe) => {
                raw.file_id = probe.file_id;
                raw.link_count = probe.link_count.max(1);
                raw.node.size_on_disk = probe
                    .size_on_disk
                    .unwrap_or_else(|| meta.len().next_multiple_of(FALLBACK_CLUSTER));
                let extra = probe.flags;
                let flags = &mut raw.node.flags;
                flags.sparse |= extra.sparse;
                flags.compressed |= extra.compressed;
                flags.cloud |= extra.cloud;
                flags.reparse |= extra.reparse;
            }
            Err(err) => {
                shared.record(path, "probe", &err);
                raw.node.size_on_disk = meta.len().next_multiple_of(FALLBACK_CLUSTER);
            }
        }
        shared.files.fetch_add(1, Ordering::Relaxed);
        shared.bytes.fetch_add(meta.len(), Ordering::Relaxed);
    }
    Some(raw)
}

/// Place the scanned entries into the arena in a fixed (sorted, depth-first) order, deciding
/// which hardlink path is the one that counts. Returns the tree and the number of extra paths.
fn assemble(root: Node, children: Vec<Raw>) -> Result<(Tree, u64), ScanError> {
    let mut tree = Tree::with_root(root);
    let mut seen: HashSet<FileId> = HashSet::new();
    let mut extra = 0u64;

    // Explicit stack, so very deep trees cannot overflow the call stack.
    let mut stack: Vec<(NodeId, std::vec::IntoIter<Raw>)> = vec![(0, children.into_iter())];
    while let Some((parent, iter)) = stack.last_mut() {
        let parent = *parent;
        let Some(mut raw) = iter.next() else {
            stack.pop();
            continue;
        };
        if raw.node.kind == Kind::File && raw.link_count > 1 {
            if let Some(id) = raw.file_id {
                if !seen.insert(id) {
                    raw.node.counted = false;
                    raw.node.flags.hardlink = true;
                    extra += 1;
                }
            }
        }
        let kind = raw.node.kind;
        let id = tree
            .push(parent, raw.node)
            .map_err(|_| ScanError::TooManyEntries)?;
        if kind == Kind::Dir {
            stack.push((id, raw.children.into_iter()));
        }
    }
    Ok((tree, extra))
}

#[cfg(test)]
mod order_tests {
    use super::read_first;
    use std::path::Path;

    #[test]
    fn holding_folders_are_recognised() {
        use super::is_holding;
        let root = if cfg!(windows) { r"E:\" } else { "/" };
        assert!(is_holding(Path::new(root), ".diskcleaner-holding"));
        let local = Path::new(root)
            .join("Users")
            .join("me")
            .join("AppData")
            .join("Local");
        assert!(is_holding(&local.join("DiskCleaner"), "Holding"));
        assert!(!is_holding(&local.join("Other"), "Holding"));
        assert!(!is_holding(Path::new(root), "Holding"));
    }

    #[test]
    fn users_and_the_profile_folders_people_sort_come_first() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        assert!(read_first(Path::new(root), "Users"));
        assert!(!read_first(Path::new(root), "Windows"));
        let profile = Path::new(root).join("Users").join("bob");
        for name in ["Downloads", "desktop", "Documents"] {
            assert!(read_first(&profile, name), "{name}");
        }
        assert!(!read_first(&profile, "AppData"));
        assert!(!read_first(&Path::new(root).join("Data"), "Downloads"));
    }
}

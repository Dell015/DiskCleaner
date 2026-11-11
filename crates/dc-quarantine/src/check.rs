//! Looking at an item on the disk right before it moves: what it is, whether a link is involved,
//! how big it is, when it last changed. Read-only.

use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Most entries one item may hold.
pub const MAX_ENTRIES: u64 = 100_000;

/// What an item is right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    /// A folder.
    pub is_dir: bool,
    /// Bytes (file lengths).
    pub bytes: u64,
    /// Files.
    pub files: u64,
    /// Entries (files and folders) inside.
    pub entries: u64,
    /// Latest change of the item or anything inside, Unix seconds (0 unknown).
    pub newest: i64,
}

/// A link, junction, or any other reparse point (cloud placeholders included): never moved,
/// never entered.
#[must_use]
pub fn is_link(meta: &Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x4_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
        if meta.file_attributes()
            & (REPARSE_POINT | OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS)
            != 0
        {
            return true;
        }
    }
    false
}

fn mtime(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// A path in a form that compares equal for the same place (no `\\?\`, one kind of slash, no
/// trailing slash; case-insensitive on Windows).
#[must_use]
pub fn same_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let mut k = if cfg!(windows) {
        text.replace('/', "\\").to_lowercase()
    } else {
        text.to_string()
    };
    while k.len() > 1 && (k.ends_with('\\') || k.ends_with('/')) && !k.ends_with(":\\") {
        k.pop();
    }
    k
}

/// Is `inner` the same as, or inside, `outer`?
#[must_use]
pub fn inside(inner: &Path, outer: &Path) -> bool {
    let (i, o) = (same_key(inner), same_key(outer));
    // A drive root keeps its separator ("c:\\", "/"); compare without it.
    let base = o.trim_end_matches(['\\', '/']);
    i == o
        || i.strip_prefix(base)
            .is_some_and(|rest| rest.starts_with(['\\', '/']))
}

/// The item's facts, or why it cannot be moved. Links anywhere (the item itself, a folder above
/// it, or anything inside) refuse it.
pub fn look(path: &Path) -> Result<Facts, String> {
    if !path.is_absolute() || path.parent().is_none() {
        return Err("it is not a full path below a drive".to_owned());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "it is not there any more".to_owned()
        } else {
            format!("it cannot be read ({e})")
        }
    })?;
    if is_link(&meta) {
        return Err("it is a link, a junction or a cloud-only file".to_owned());
    }
    let real =
        fs::canonicalize(path).map_err(|e| format!("its real location cannot be found ({e})"))?;
    if same_key(&real) != same_key(path) {
        return Err("a folder above it is a link: its real location is elsewhere".to_owned());
    }
    let mut facts = Facts {
        is_dir: meta.is_dir(),
        newest: mtime(&meta),
        ..Facts::default()
    };
    if !meta.is_dir() {
        facts.bytes = meta.len();
        facts.files = 1;
        return Ok(facts);
    }
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let reader =
            fs::read_dir(&dir).map_err(|e| format!("{} cannot be read ({e})", dir.display()))?;
        for entry in reader {
            let entry = entry.map_err(|e| format!("{} cannot be read ({e})", dir.display()))?;
            let m = fs::symlink_metadata(entry.path())
                .map_err(|e| format!("{} cannot be read ({e})", entry.path().display()))?;
            if is_link(&m) {
                return Err(format!(
                    "it contains a link or cloud-only file ({})",
                    entry.path().display()
                ));
            }
            facts.entries += 1;
            if facts.entries > MAX_ENTRIES {
                return Err(format!(
                    "it holds more than {MAX_ENTRIES} entries; remove smaller parts"
                ));
            }
            facts.newest = facts.newest.max(mtime(&m));
            if m.is_dir() {
                stack.push(entry.path());
            } else {
                facts.bytes += m.len();
                facts.files += 1;
            }
        }
    }
    Ok(facts)
}

/// Every file inside a folder, relative to it (for moving file by file). Links are left out
/// (`look` has already refused folders that contain any).
pub fn files_inside(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let Ok(reader) = fs::read_dir(path.join(&rel)) else {
            continue;
        };
        for entry in reader.flatten() {
            let Ok(m) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if is_link(&m) {
                continue;
            }
            let child = rel.join(entry.file_name());
            if m.is_dir() {
                stack.push(child);
            } else {
                out.push(child);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_works_for_roots_and_ignores_lookalike_names() {
        assert!(inside(Path::new("/a/b"), Path::new("/")));
        assert!(inside(Path::new("/a/b"), Path::new("/a")));
        assert!(inside(Path::new("/a"), Path::new("/a/")));
        assert!(!inside(Path::new("/ab"), Path::new("/a")));
        assert!(!inside(Path::new("/a"), Path::new("/a/b")));
        if cfg!(windows) {
            assert!(inside(Path::new(r"C:\Users\me"), Path::new(r"c:\")));
            assert!(inside(Path::new(r"\\?\C:\Users"), Path::new(r"C:\users")));
        }
    }
}

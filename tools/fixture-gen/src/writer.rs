//! The only module that creates files. Everything it does is create-only.
//!
//! Every path is built from validated name components, files are opened with `create_new`
//! (an atomic "must not exist"), directories with `create_dir`, and nothing is ever deleted,
//! renamed or overwritten. Every created entry is recorded in a ledger, and the manifest's
//! totals are computed from that ledger, so a skipped entry can never inflate them.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{io_err, GenError};
use crate::manifest::{SpecialEntry, SpecialStatus};
use crate::platform::{self, Outcome};
use crate::rng::{rng_for, Xoshiro256StarStar};

/// Chunk size for streaming random content.
const CHUNK: usize = 64 * 1024;
/// 2015-01-01T00:00:00Z. File mtimes are drawn from `[MTIME_LO, MTIME_HI)`.
const MTIME_LO: u64 = 1_420_070_400;
/// 2025-01-01T00:00:00Z (exclusive), so the last possible day is 2024-12-31.
const MTIME_HI: u64 = 1_735_689_600;
/// Every text file starts with this line.
pub(crate) const FIXTURE_MARKER: &str = "DiskCleaner fixture - dummy data";
/// Executable-looking files start with these bytes instead of `MZ`, so none can be a valid PE.
const FAKE_HEADER: &[u8; 4] = b"DCFX";

/// Names Windows reserves for devices. They are invalid file names there, in any case.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// What kind of entry the ledger holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Dir,
    File,
    Hardlink,
    Link,
}

/// One created entry.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub kind: EntryKind,
    /// Relative to the tree root, `/` separated.
    pub path: String,
    /// Logical size in bytes (0 for directories and links).
    pub size: u64,
}

/// A category as recorded while generating.
#[derive(Debug, Clone)]
pub(crate) struct CategoryRec {
    pub name: String,
    pub summarized: bool,
    pub paths: Vec<String>,
    pub count: u64,
}

/// Everything the generator created.
#[derive(Debug, Default)]
pub(crate) struct Parts {
    pub entries: Vec<Entry>,
    pub categories: Vec<CategoryRec>,
    pub special: Vec<SpecialEntry>,
}

/// How a file's content is produced.
enum Body {
    Random(u64),
    Executable(u64),
    Text(String),
    Empty,
}

/// The generation context: tree root, RNG, and the ledger.
pub(crate) struct Ctx {
    root: PathBuf,
    seed: u64,
    rng: Xoshiro256StarStar,
    dirs_made: HashSet<String>,
    current: Option<usize>,
    parts: Parts,
}

impl Ctx {
    pub(crate) fn new(root: PathBuf, seed: u64) -> Self {
        Self {
            root,
            seed,
            rng: rng_for(seed, "default"),
            dirs_made: HashSet::new(),
            current: None,
            parts: Parts::default(),
        }
    }

    pub(crate) fn into_parts(self) -> Parts {
        self.parts
    }

    // ----- categories and randomness -------------------------------------------------------

    /// Start a category. Its content comes from its own independent RNG stream.
    pub(crate) fn begin_category(&mut self, name: &str) {
        self.rng = rng_for(self.seed, name);
        self.parts.categories.push(CategoryRec {
            name: name.to_owned(),
            summarized: false,
            paths: Vec::new(),
            count: 0,
        });
        self.current = Some(self.parts.categories.len() - 1);
    }

    /// Start a category that lists only `root` and counts its files (for huge directories).
    pub(crate) fn begin_summarized_category(&mut self, name: &str, root: &str) {
        self.begin_category(name);
        if let Some(rec) = self.current.and_then(|i| self.parts.categories.get_mut(i)) {
            rec.summarized = true;
            rec.paths.push(root.to_owned());
        }
    }

    /// `digits` lowercase hex digits (at most 16) from the current category's stream.
    pub(crate) fn hex(&mut self, digits: usize) -> String {
        let text = format!("{:016x}", self.rng.next_u64());
        text[..digits.min(16)].to_owned()
    }

    // ----- directories and files -----------------------------------------------------------

    /// Create a directory and every missing ancestor, one level at a time.
    pub(crate) fn dirs(&mut self, comps: &[&str]) -> Result<(), GenError> {
        for name in comps {
            validate_component(name)?;
        }
        for depth in 1..=comps.len() {
            let prefix = &comps[..depth];
            let rel = rel_string(prefix);
            if self.dirs_made.contains(&rel) {
                continue;
            }
            let abs = self.abs(prefix);
            fs::create_dir(&abs).map_err(io_err("create directory", &rel))?;
            self.dirs_made.insert(rel.clone());
            self.record(EntryKind::Dir, rel, 0);
        }
        Ok(())
    }

    /// A file of random bytes with a size drawn from `lo..hi`.
    pub(crate) fn bin(
        &mut self,
        dir: &[&str],
        name: &str,
        lo: u64,
        hi: u64,
    ) -> Result<(), GenError> {
        let size = self.rng.range(lo, hi);
        self.make_file(dir, name, Body::Random(size))
    }

    /// An executable-looking file (`.exe`, `.dll`, `.sys`) that is deliberately not a valid PE.
    pub(crate) fn exe(
        &mut self,
        dir: &[&str],
        name: &str,
        lo: u64,
        hi: u64,
    ) -> Result<(), GenError> {
        let size = self.rng.range(lo, hi).max(FAKE_HEADER.len() as u64);
        self.make_file(dir, name, Body::Executable(size))
    }

    /// A short text file: the marker line, `body`, and a random token line.
    pub(crate) fn text(&mut self, dir: &[&str], name: &str, body: &str) -> Result<(), GenError> {
        let token = self.rng.next_u64();
        let content = format!("{FIXTURE_MARKER}\n{body}token={token:016x}\n");
        self.make_file(dir, name, Body::Text(content))
    }

    /// A zero-byte file.
    pub(crate) fn empty(&mut self, dir: &[&str], name: &str) -> Result<(), GenError> {
        self.make_file(dir, name, Body::Empty)
    }

    fn make_file(&mut self, dir: &[&str], name: &str, body: Body) -> Result<(), GenError> {
        let mut comps: Vec<&str> = dir.to_vec();
        comps.push(name);
        validate_component(name)?;
        self.dirs(dir)?;

        let rel = rel_string(&comps);
        let abs = self.abs(&comps);
        let mtime = self.next_mtime();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&abs)
            .map_err(io_err("create file", &rel))?;

        let size = match body {
            Body::Random(len) => {
                write_random(&mut file, &mut self.rng, len).map_err(io_err("write file", &rel))?;
                len
            }
            Body::Executable(len) => {
                file.write_all(FAKE_HEADER)
                    .map_err(io_err("write file", &rel))?;
                let rest = len - FAKE_HEADER.len() as u64;
                write_random(&mut file, &mut self.rng, rest).map_err(io_err("write file", &rel))?;
                len
            }
            Body::Text(content) => {
                file.write_all(content.as_bytes())
                    .map_err(io_err("write file", &rel))?;
                content.len() as u64
            }
            Body::Empty => 0,
        };
        file.set_modified(mtime)
            .map_err(io_err("set modified time", &rel))?;
        drop(file);
        self.record(EntryKind::File, rel, size);
        Ok(())
    }

    // ----- special entries -----------------------------------------------------------------

    /// A second path to the data of an existing file (a hardlink).
    pub(crate) fn hardlink(&mut self, existing: &[&str], new: &[&str]) -> Result<(), GenError> {
        for name in existing.iter().chain(new) {
            validate_component(name)?;
        }
        let existing_rel = rel_string(existing);
        let size = self
            .parts
            .entries
            .iter()
            .find(|e| e.kind == EntryKind::File && e.path == existing_rel)
            .map(|e| e.size)
            .ok_or_else(|| {
                GenError::InvalidName(format!("hardlink source {existing_rel:?} was not created"))
            })?;
        if let Some((_, parent)) = new.split_last() {
            self.dirs(parent)?;
        }
        let new_rel = rel_string(new);
        let (existing_abs, new_abs) = (self.abs(existing), self.abs(new));
        self.require_inside(&existing_abs)?;
        self.require_inside(&new_abs)?;
        fs::hard_link(&existing_abs, &new_abs).map_err(io_err("create hardlink", &new_rel))?;
        self.record(EntryKind::Hardlink, new_rel.clone(), size);
        self.parts.special.push(SpecialEntry {
            kind: "hardlink".to_owned(),
            path: new_rel,
            target: Some(existing_rel),
            status: SpecialStatus::Created,
            reason: None,
        });
        Ok(())
    }

    /// A file symlink. `target` must be a sibling of `link`, so the link points to its own folder.
    pub(crate) fn symlink_file(&mut self, link: &[&str], target: &[&str]) -> Result<(), GenError> {
        self.symlink("symlink_file", link, target, platform::symlink_file)
    }

    /// A directory symlink. `target` must be a sibling of `link`.
    pub(crate) fn symlink_dir(&mut self, link: &[&str], target: &[&str]) -> Result<(), GenError> {
        self.symlink("symlink_dir", link, target, platform::symlink_dir)
    }

    fn symlink(
        &mut self,
        kind: &str,
        link: &[&str],
        target: &[&str],
        create: fn(&Path, &str) -> io::Result<Outcome>,
    ) -> Result<(), GenError> {
        for name in link.iter().chain(target) {
            validate_component(name)?;
        }
        let (Some((link_name, link_parent)), Some((target_name, target_parent))) =
            (link.split_last(), target.split_last())
        else {
            return Err(GenError::InvalidName("empty link path".to_owned()));
        };
        if link_parent != target_parent {
            return Err(GenError::PathEscapesTree(format!(
                "{} must point to a sibling, not {}",
                rel_string(link),
                rel_string(target)
            )));
        }
        let _ = link_name;
        self.dirs(link_parent)?;
        let link_rel = rel_string(link);
        let link_abs = self.abs(link);
        self.require_inside(&link_abs)?;
        let outcome =
            create(&link_abs, target_name).map_err(io_err("create symlink", &link_rel))?;
        self.record_special(kind, link_rel, Some(rel_string(target)), outcome);
        Ok(())
    }

    /// A directory junction (Windows only; skipped elsewhere).
    pub(crate) fn junction(&mut self, link: &[&str], target: &[&str]) -> Result<(), GenError> {
        for name in link.iter().chain(target) {
            validate_component(name)?;
        }
        if let Some((_, parent)) = link.split_last() {
            self.dirs(parent)?;
        }
        let link_rel = rel_string(link);
        let (link_abs, target_abs) = (self.abs(link), self.abs(target));
        // Anything handed to an external command must be inside the tree.
        self.require_inside(&link_abs)?;
        self.require_inside(&target_abs)?;
        let outcome = platform::junction(&link_abs, &target_abs)
            .map_err(io_err("create junction", &link_rel))?;
        self.record_special("junction", link_rel, Some(rel_string(target)), outcome);
        Ok(())
    }

    /// A sparse file: logical length `logical`, with `data_len` random bytes at `offset`.
    ///
    /// If the file cannot be made sparse it stays an ordinary zero-byte file and is recorded as
    /// skipped. It is never deleted and never filled to its logical size.
    pub(crate) fn sparse(
        &mut self,
        dir: &[&str],
        name: &str,
        logical: u64,
        offset: u64,
        data_len: usize,
    ) -> Result<(), GenError> {
        let mut comps: Vec<&str> = dir.to_vec();
        comps.push(name);
        validate_component(name)?;
        self.dirs(dir)?;
        let rel = rel_string(&comps);
        let abs = self.abs(&comps);
        self.require_inside(&abs)?;
        let mtime = self.next_mtime();

        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&abs)
            .map_err(io_err("create sparse file", &rel))?;
        let mut data = vec![0u8; data_len];
        self.rng.fill_bytes(&mut data);
        let outcome = platform::make_sparse(&abs, logical, offset, &data)
            .map_err(io_err("make sparse file", &rel))?;
        let created = matches!(outcome, Outcome::Created);

        let file = OpenOptions::new()
            .write(true)
            .open(&abs)
            .map_err(io_err("reopen sparse file", &rel))?;
        file.set_modified(mtime)
            .map_err(io_err("set modified time", &rel))?;
        drop(file);

        self.record(
            EntryKind::File,
            rel.clone(),
            if created { logical } else { 0 },
        );
        let (status, reason) = match outcome {
            Outcome::Created => (SpecialStatus::Created, None),
            Outcome::Skipped(why) | Outcome::SkippedButExists(why) => {
                (SpecialStatus::Skipped, Some(why))
            }
        };
        self.parts.special.push(SpecialEntry {
            kind: "sparse".to_owned(),
            path: rel,
            target: None,
            status,
            reason,
        });
        Ok(())
    }

    // ----- helpers -------------------------------------------------------------------------

    fn record_special(
        &mut self,
        kind: &str,
        rel: String,
        target: Option<String>,
        outcome: Outcome,
    ) {
        let (status, reason, exists) = match outcome {
            Outcome::Created => (SpecialStatus::Created, None, true),
            Outcome::Skipped(why) => (SpecialStatus::Skipped, Some(why), false),
            Outcome::SkippedButExists(why) => (SpecialStatus::Skipped, Some(why), true),
        };
        if exists {
            self.record(EntryKind::Link, rel.clone(), 0);
        }
        self.parts.special.push(SpecialEntry {
            kind: kind.to_owned(),
            path: rel,
            target,
            status,
            reason,
        });
    }

    fn record(&mut self, kind: EntryKind, path: String, size: u64) {
        if let Some(rec) = self.current.and_then(|i| self.parts.categories.get_mut(i)) {
            if rec.summarized {
                if matches!(kind, EntryKind::File | EntryKind::Hardlink) {
                    rec.count += 1;
                }
            } else {
                rec.paths.push(path.clone());
            }
        }
        self.parts.entries.push(Entry { kind, path, size });
    }

    fn abs(&self, comps: &[&str]) -> PathBuf {
        let mut path = self.root.clone();
        path.extend(comps);
        path
    }

    fn require_inside(&self, path: &Path) -> Result<(), GenError> {
        if path.starts_with(&self.root) {
            Ok(())
        } else {
            Err(GenError::PathEscapesTree(path.display().to_string()))
        }
    }

    fn next_mtime(&mut self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.rng.range(MTIME_LO, MTIME_HI))
    }
}

/// Join name components into a relative path with `/` separators.
pub(crate) fn rel_string(comps: &[&str]) -> String {
    comps.join("/")
}

/// Stream `len` random bytes into `file` in 64 KiB chunks.
fn write_random(file: &mut fs::File, rng: &mut Xoshiro256StarStar, len: u64) -> io::Result<()> {
    let mut buf = vec![0u8; CHUNK.min(usize::try_from(len).unwrap_or(CHUNK))];
    let mut remaining = len;
    while remaining > 0 {
        let take = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        rng.fill_bytes(&mut buf[..take]);
        file.write_all(&buf[..take])?;
        remaining -= take as u64;
    }
    Ok(())
}

/// Check that a single name component is valid on NTFS and on ext4.
///
/// A failure here means the generator's own layout is wrong, so generation aborts.
pub(crate) fn validate_component(name: &str) -> Result<(), GenError> {
    let bad = |why: &str| GenError::InvalidName(format!("{name:?}: {why}"));
    if name.is_empty() {
        return Err(bad("empty"));
    }
    if name == "." || name == ".." {
        return Err(bad("dot component"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(bad("trailing dot or space"));
    }
    if name.chars().any(|c| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
    }) {
        return Err(bad("forbidden character"));
    }
    if name.len() > 255 {
        return Err(bad("longer than 255 bytes"));
    }
    let stem = name.split('.').next().unwrap_or_default();
    if RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        return Err(bad("reserved device name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_names_are_accepted() {
        for name in [
            "steam.exe",
            "Program Files (x86)",
            ".leading_dot",
            "日本語フォルダ",
            "Кириллица.txt",
            "ملف_عربي.txt",
            "📁emoji🎮",
            "e\u{301}combining.txt",
            "~DF0123ABCD.tmp",
            "CONSOLE.txt",
        ] {
            assert!(validate_component(name).is_ok(), "{name:?} should be valid");
        }
        assert!(validate_component(&"L".repeat(255)).is_ok());
    }

    #[test]
    fn bad_names_are_rejected() {
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "a\0b",
            "trailing.",
            "trailing ",
            "CON",
            "con",
            "NUL.txt",
            "Com1",
            "LPT9.log",
        ] {
            assert!(
                validate_component(name).is_err(),
                "{name:?} should be rejected"
            );
        }
        assert!(validate_component(&"L".repeat(256)).is_err());
    }

    #[test]
    fn rel_string_uses_forward_slashes() {
        assert_eq!(rel_string(&["a", "b c", "d.txt"]), "a/b c/d.txt");
    }
}

//! Reading Windows shortcuts (`.lnk` files) in safe Rust, without COM and without running
//! anything. Only the parts needed to learn what a shortcut points to are read (the
//! `[MS-SHLLINK]` header, the LinkInfo block and the string data). Every read is bounds-checked;
//! anything inconsistent gives `None`, never a panic.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Problem, Shortcut};
use crate::source::SourceReport;

const HEADER_SIZE: u32 = 0x4C;
const LNK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

const HAS_ID_LIST: u32 = 0x01;
const HAS_LINK_INFO: u32 = 0x02;
const HAS_NAME: u32 = 0x04;
const HAS_RELATIVE_PATH: u32 = 0x08;
const HAS_WORKING_DIR: u32 = 0x10;
const HAS_ARGUMENTS: u32 = 0x20;
const HAS_ICON_LOCATION: u32 = 0x40;
const IS_UNICODE: u32 = 0x80;

/// Shortcut files bigger than this are ignored (real ones are a few KiB).
const MAX_LNK_BYTES: u64 = 256 * 1024;
/// How deep below a Start Menu folder to look.
const MAX_DEPTH: usize = 8;
/// Upper bound on shortcuts collected in one run.
const MAX_SHORTCUTS: usize = 50_000;

/// What could be read out of one `.lnk` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedLink {
    /// Absolute target path from the LinkInfo block.
    pub target: Option<String>,
    /// Command-line arguments.
    pub arguments: Option<String>,
    /// Working directory.
    pub working_dir: Option<String>,
}

fn u16_at(data: &[u8], pos: usize) -> Option<u16> {
    let bytes = data.get(pos..pos.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    let bytes = data.get(pos..pos.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn usize_at(data: &[u8], pos: usize) -> Option<usize> {
    usize::try_from(u32_at(data, pos)?).ok()
}

/// UTF-16 text from `bytes` (an odd trailing byte is ignored), stopping at the first NUL.
fn utf16_text(bytes: &[u8]) -> String {
    let mut units = Vec::with_capacity(bytes.len() / 2);
    let mut at = 0;
    while let Some(unit) = u16_at(bytes, at) {
        if unit == 0 {
            break;
        }
        units.push(unit);
        at += 2;
    }
    String::from_utf16_lossy(&units)
}

fn ansi_text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// A NUL-terminated string starting at `pos`, or `None` if `pos` is outside the data.
fn text_at(data: &[u8], pos: usize, unicode: bool) -> Option<String> {
    let rest = data.get(pos..)?;
    Some(if unicode {
        utf16_text(rest)
    } else {
        ansi_text(rest)
    })
}

/// The local target path stored in a LinkInfo block (`info` is exactly that block).
fn link_info_target(info: &[u8]) -> Option<String> {
    let header_size = usize_at(info, 4)?;
    let flags = u32_at(info, 8)?;
    if flags & 1 == 0 {
        return None; // no local path (network target)
    }
    let base_offset = usize_at(info, 16)?;
    let suffix_offset = usize_at(info, 24)?;

    let (mut base, mut suffix) = (None, None);
    if header_size >= 0x24 {
        let base_unicode = usize_at(info, 28)?;
        let suffix_unicode = usize_at(info, 32)?;
        if base_unicode != 0 {
            base = text_at(info, base_unicode, true);
        }
        if suffix_unicode != 0 {
            suffix = text_at(info, suffix_unicode, true);
        }
    }
    let base = base.or_else(|| text_at(info, base_offset, false))?;
    let suffix = suffix
        .or_else(|| text_at(info, suffix_offset, false))
        .unwrap_or_default();

    if base.trim().is_empty() {
        return None;
    }
    if suffix.is_empty() {
        return Some(base);
    }
    let separator = if base.ends_with('\\') { "" } else { "\\" };
    Some(format!("{base}{separator}{suffix}"))
}

/// Parse a `.lnk` file. Returns `None` if it is not a valid shortcut.
pub fn parse_lnk(data: &[u8]) -> Option<ParsedLink> {
    if u32_at(data, 0)? != HEADER_SIZE || data.get(4..20)? != LNK_CLSID {
        return None;
    }
    let flags = u32_at(data, 20)?;
    let unicode = flags & IS_UNICODE != 0;
    let mut pos = usize::try_from(HEADER_SIZE).ok()?;

    if flags & HAS_ID_LIST != 0 {
        let size = usize::from(u16_at(data, pos)?);
        pos = pos.checked_add(2)?.checked_add(size)?;
    }

    let mut link = ParsedLink::default();
    if flags & HAS_LINK_INFO != 0 {
        let size = usize_at(data, pos)?;
        let end = pos.checked_add(size)?;
        link.target = link_info_target(data.get(pos..end)?);
        pos = end;
    }

    // String data comes in a fixed order; each present item is a u16 character count + text.
    for bit in [
        HAS_NAME,
        HAS_RELATIVE_PATH,
        HAS_WORKING_DIR,
        HAS_ARGUMENTS,
        HAS_ICON_LOCATION,
    ] {
        if flags & bit == 0 {
            continue;
        }
        let Some(count) = u16_at(data, pos) else {
            break;
        };
        pos += 2;
        let length = usize::from(count) * if unicode { 2 } else { 1 };
        let Some(raw) = data.get(pos..pos.saturating_add(length)) else {
            break;
        };
        pos += length;
        let text = if unicode {
            utf16_text(raw)
        } else {
            ansi_text(raw)
        };
        let text = text.trim().to_owned();
        if text.is_empty() {
            continue;
        }
        match bit {
            HAS_WORKING_DIR => link.working_dir = Some(text),
            HAS_ARGUMENTS => link.arguments = Some(text),
            _ => {}
        }
    }
    Some(link)
}

struct Collector {
    shortcuts: Vec<Shortcut>,
    unreadable: usize,
    not_shortcuts: usize,
}

fn walk(dir: &Path, depth: usize, out: &mut Collector) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if out.shortcuts.len() >= MAX_SHORTCUTS {
            return;
        }
        // `symlink_metadata`: links are never followed.
        let Ok(meta) = fs::symlink_metadata(&path) else {
            out.unreadable += 1;
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, out);
            }
            continue;
        }
        let is_lnk = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
        if !meta.is_file() || !is_lnk {
            continue;
        }
        if meta.len() > MAX_LNK_BYTES {
            out.not_shortcuts += 1;
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            out.unreadable += 1;
            continue;
        };
        let Some(link) = parse_lnk(&bytes) else {
            out.not_shortcuts += 1;
            continue;
        };
        out.shortcuts.push(Shortcut {
            lnk: path.display().to_string(),
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            target: link.target,
            arguments: link.arguments,
            working_dir: link.working_dir,
        });
    }
}

/// Collect every readable `.lnk` below the given folders (missing folders are skipped). Files
/// are only read, links are not followed, and broken shortcuts are counted in one problem.
pub fn collect_shortcuts(dirs: &[PathBuf]) -> SourceReport<Vec<Shortcut>> {
    let mut out = Collector {
        shortcuts: Vec::new(),
        unreadable: 0,
        not_shortcuts: 0,
    };
    for dir in dirs {
        walk(dir, 0, &mut out);
    }
    out.shortcuts.sort_by(|a, b| a.lnk.cmp(&b.lnk));
    let mut problems = Vec::new();
    if out.unreadable + out.not_shortcuts > 0 {
        problems.push(Problem {
            source: "shortcuts".to_owned(),
            message: format!(
                "{} shortcut files could not be read and {} were not valid shortcuts",
                out.unreadable, out.not_shortcuts
            ),
        });
    }
    SourceReport {
        items: out.shortcuts,
        problems,
    }
}

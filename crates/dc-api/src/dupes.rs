//! Exact duplicate files, found on request. Read-only: files are opened for reading only, and
//! nothing is changed.
//!
//! Built for hard disks: one file at a time, biggest sizes first (most space shows up first).
//! 1. Files of the same size are candidates (most files have a size nobody else has).
//! 2. A quick fingerprint of the first and last 64 KiB splits most candidates apart.
//! 3. Files still alike are read in full and fingerprinted (BLAKE3); equal fingerprints are
//!    duplicates.
//!
//! Left out: links, cloud placeholders (reading one would download it), second names of hard
//! links (the same file, no extra space), system files, empty and small files, and Windows' own
//! top folders.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use dc_core::{Kind, ScanResult};
use dc_scan::CancelToken;
use serde::Serialize;

use crate::analysis::full_paths;
use crate::live::display_path;
use crate::ApiError;

/// Bytes read from each end of a file for the quick fingerprint.
const EDGE: u64 = 64 * 1024;
/// Read buffer for full fingerprints.
const CHUNK: usize = 1024 * 1024;
/// Top folders of a drive that belong to Windows.
const SYSTEM_TOP: [&str; 8] = [
    "windows",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
    "$windows.~bt",
    "$windows.~ws",
    "programdata\\package cache",
];

/// What to look at.
#[derive(Debug, Clone)]
pub struct DupOptions {
    /// Only files below this folder (display form, as the view shows it). `None`: everything.
    pub within: Option<String>,
    /// Files smaller than this are left out.
    pub min_size: u64,
    /// Stop early when cancelled.
    pub cancel: CancelToken,
    /// How often progress is reported.
    pub progress_interval: Duration,
}

impl Default for DupOptions {
    fn default() -> Self {
        Self {
            within: None,
            min_size: 1024 * 1024,
            cancel: CancelToken::new(),
            progress_interval: Duration::from_millis(250),
        }
    }
}

/// Something that happened while looking for duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DupEvent {
    /// The candidates are known: `files` share a size with at least one other file.
    Started {
        /// Candidate files.
        files: u64,
        /// Their total size: the most that may have to be read.
        bytes: u64,
    },
    /// How far it has got.
    Progress {
        /// Bytes read or ruled out so far (of `Started::bytes`).
        done: u64,
        /// The file being read.
        current: String,
    },
    /// A set of identical files (paths sorted).
    Group {
        /// Size of one copy.
        size: u64,
        /// Every copy.
        paths: Vec<String>,
    },
    /// Done (or stopped).
    Finished {
        /// "complete" or "cancelled".
        status: &'static str,
        /// Sets of identical files found.
        groups: u64,
        /// Space the extra copies take (every copy but one per set).
        wasted: u64,
        /// Files that could not be read (in use, no access); they were left out.
        unreadable: u64,
    },
}

struct Candidate {
    path: String,
    size: u64,
}

fn under_system(lower: &str) -> bool {
    // "c:\windows\..." -> the part after the drive.
    let rest = lower.get(3..).unwrap_or("");
    SYSTEM_TOP
        .iter()
        .any(|top| rest == *top || rest.starts_with(&format!("{top}\\")))
}

fn inside(lower_path: &str, lower_folder: &str) -> bool {
    let folder = lower_folder.trim_end_matches(['\\', '/']);
    lower_path.len() > folder.len()
        && lower_path.starts_with(folder)
        && matches!(lower_path.as_bytes().get(folder.len()), Some(b'\\' | b'/'))
}

/// The files worth comparing, grouped by size (only sizes shared by two or more), biggest first.
fn candidates(scan: &ScanResult, opts: &DupOptions) -> Result<Vec<Vec<Candidate>>, ApiError> {
    let tree = scan.tree()?;
    let real = full_paths(&tree, &scan.root);
    let within = opts.within.as_deref().map(str::to_lowercase);
    let mut by_size: HashMap<u64, Vec<Candidate>> = HashMap::new();
    for node in tree.nodes() {
        let f = &node.flags;
        if node.kind != Kind::File
            || !node.counted
            || f.reparse
            || f.cloud
            || f.hardlink
            || f.system
            || node.logical_size < opts.min_size.max(1)
        {
            continue;
        }
        let Some(path) = real.get(node.id as usize) else {
            continue;
        };
        let lower = display_path(path).to_lowercase();
        if under_system(&lower) || within.as_deref().is_some_and(|w| !inside(&lower, w)) {
            continue;
        }
        by_size
            .entry(node.logical_size)
            .or_default()
            .push(Candidate {
                path: path.clone(),
                size: node.logical_size,
            });
    }
    let mut groups: Vec<Vec<Candidate>> = by_size.into_values().filter(|g| g.len() > 1).collect();
    for g in &mut groups {
        g.sort_by(|a, b| a.path.cmp(&b.path));
    }
    groups.sort_by(|a, b| b[0].size.cmp(&a[0].size).then(a[0].path.cmp(&b[0].path)));
    Ok(groups)
}

/// First and last `EDGE` bytes (the whole file when it is small).
fn quick_print(path: &Path, size: u64) -> std::io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());
    let mut buf = vec![0u8; usize::try_from(EDGE).unwrap_or(65_536)];
    let first = usize::try_from(size.min(EDGE)).unwrap_or(0);
    file.read_exact(&mut buf[..first])?;
    hasher.update(&buf[..first]);
    if size > 2 * EDGE {
        file.seek(SeekFrom::Start(size - EDGE))?;
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    } else if size > EDGE {
        let rest = usize::try_from(size - EDGE).unwrap_or(0);
        file.read_exact(&mut buf[..rest])?;
        hasher.update(&buf[..rest]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// The whole file. `Ok(None)` if cancelled half-way.
fn full_print(path: &Path, cancel: &CancelToken) -> std::io::Result<Option<[u8; 32]>> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Some(*hasher.finalize().as_bytes()))
}

/// Look for identical files in a finished scan. Events go to `sink`; the last one is always
/// `Finished`. Nothing is changed.
pub fn find_duplicates(
    scan: &ScanResult,
    opts: &DupOptions,
    mut sink: impl FnMut(DupEvent),
) -> Result<(), ApiError> {
    let groups = candidates(scan, opts)?;
    let files: u64 = groups.iter().map(|g| g.len() as u64).sum();
    let bytes: u64 = groups.iter().flatten().map(|c| c.size).sum();
    sink(DupEvent::Started { files, bytes });

    let mut done = 0u64;
    let mut last = Instant::now();
    let mut found = 0u64;
    let mut wasted = 0u64;
    let mut unreadable = 0u64;
    let mut cancelled = false;
    let mut report = |done: u64, current: &str, force: bool, sink: &mut dyn FnMut(DupEvent)| {
        if force || last.elapsed() >= opts.progress_interval {
            last = Instant::now();
            sink(DupEvent::Progress {
                done,
                current: display_path(current),
            });
        }
    };

    'sizes: for group in groups {
        let size = group[0].size;
        // Quick fingerprints.
        let mut by_quick: HashMap<[u8; 32], Vec<&Candidate>> = HashMap::new();
        for c in &group {
            if opts.cancel.is_cancelled() {
                cancelled = true;
                break 'sizes;
            }
            report(done, &c.path, false, &mut sink);
            match quick_print(Path::new(&c.path), size) {
                Ok(print) => by_quick.entry(print).or_default().push(c),
                Err(_) => unreadable += 1,
            }
            done += size.min(2 * EDGE);
        }
        // What the quick look already told apart needs no more reading.
        let rest = size.saturating_sub(2 * EDGE);
        let mut alike: Vec<Vec<&Candidate>> = Vec::new();
        for v in by_quick.into_values() {
            if v.len() > 1 {
                alike.push(v);
            } else {
                done += rest;
            }
        }
        // Read in full what is still alike (a small file was already read whole).
        alike.sort_by(|a, b| a[0].path.cmp(&b[0].path));
        for set in alike {
            let sets: Vec<Vec<&Candidate>> = if size <= 2 * EDGE {
                vec![set]
            } else {
                let mut by_full: HashMap<[u8; 32], Vec<&Candidate>> = HashMap::new();
                for c in set {
                    report(done, &c.path, true, &mut sink);
                    let result = full_print(Path::new(&c.path), &opts.cancel);
                    done += rest;
                    match result {
                        Ok(Some(print)) => by_full.entry(print).or_default().push(c),
                        Ok(None) => {
                            cancelled = true;
                            break 'sizes;
                        }
                        Err(_) => unreadable += 1,
                    }
                }
                by_full.into_values().filter(|v| v.len() > 1).collect()
            };
            for same in sets {
                let mut paths: Vec<String> = same.iter().map(|c| display_path(&c.path)).collect();
                paths.sort();
                found += 1;
                wasted += size * (paths.len() as u64 - 1);
                sink(DupEvent::Group { size, paths });
            }
        }
    }
    report(done.min(bytes), "", true, &mut sink);
    sink(DupEvent::Finished {
        status: if cancelled { "cancelled" } else { "complete" },
        groups: found,
        wasted,
        unreadable,
    });
    Ok(())
}

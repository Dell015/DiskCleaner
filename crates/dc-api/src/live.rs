//! A live scan: what the scanner finds is sent out in small batches while it runs (a folder always
//! before anything inside it), then the analysis. A view can grow as the disk is read.
//!
//! Read-only, like every scan. Paths are sent in display form (no `\\?\` prefix).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dc_core::{ScanResult, ScanStatus, SCHEMA_VERSION};
use dc_rules::Classifier;
use dc_scan::{CancelToken, Listing, Progress, ScanOptions};
use dc_winmeta::AppSnapshot;
use serde::Serialize;

use crate::analysis::{analyze, full_paths, Mark, Owner};
use crate::view::{category_index, flag_bits, kind_code, own_size};
use crate::ApiError;

/// Version of the live event format.
pub const LIVE_SCHEMA_VERSION: u32 = 1;

/// One entry found in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveEntry {
    /// Name.
    pub name: String,
    /// 0 file, 1 folder, 2 link, 3 other.
    pub kind: u8,
    /// Own size on disk (files).
    pub size: u64,
    /// Last change, seconds since 1970; 0 when unknown.
    pub mtime: i64,
    /// Flag bits (see [`crate::view::flag`]).
    pub flags: u32,
    /// Category index, or -1.
    pub cat: i8,
}

/// One folder's entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveDir {
    /// Full path of the folder (display form).
    pub dir: String,
    /// Its entries, sorted by name.
    pub entries: Vec<LiveEntry>,
}

/// A mark placed by path (the view finds the entry by its path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathMark {
    /// Full path of the entry.
    pub path: String,
    /// The mark (its `node` is the index in the finished scan; views use `path`).
    #[serde(flatten)]
    pub mark: Mark,
}

/// An owner assignment placed by path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathAssignment {
    /// Full path of the folder.
    pub path: String,
    /// Index into `owners`.
    pub owner: u32,
    /// 0 low, 1 medium, 2 high.
    pub confidence: u32,
}

/// What the finished scan adds to what was already sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveSummary {
    /// "complete", "partial" (some entries could not be read) or "cancelled".
    pub status: &'static str,
    /// Entries that could not be read.
    pub errors: u64,
    /// Files found.
    pub files: u64,
    /// Folders found.
    pub dirs: u64,
    /// Total size on disk, hard links counted once.
    pub bytes_on_disk: u64,
    /// What the checks say, by path.
    pub marks: Vec<PathMark>,
    /// Owners.
    pub owners: Vec<Owner>,
    /// Owner assignments, by path.
    pub assignments: Vec<PathAssignment>,
    /// Second names of hard-linked files: their size was sent but must not be counted again.
    pub uncounted: Vec<String>,
    /// Whether installed apps were known (owners and leftovers need them).
    pub apps_known: bool,
}

/// Something that happened in a live scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveEvent {
    /// The scan started; `root` is the folder as all later paths begin.
    Started {
        /// [`LIVE_SCHEMA_VERSION`].
        schema_version: u32,
        /// The scanned folder (display form).
        root: String,
        /// The folder is a whole drive (then `volume_used` says how much there is to read).
        drive_root: bool,
        /// Size of the drive holding the folder, when known.
        volume_total: Option<u64>,
        /// Space in use on that drive, when known (a progress bar can compare against it).
        volume_used: Option<u64>,
    },
    /// Folders read since the last batch, each before anything inside it.
    Batch {
        /// The folders.
        dirs: Vec<LiveDir>,
    },
    /// How far the scan has got.
    Progress {
        /// Files seen.
        files: u64,
        /// Folders seen.
        dirs: u64,
        /// Bytes seen (file lengths).
        bytes: u64,
        /// A folder being read now.
        current: String,
    },
    /// Reading is done; the checks are running.
    Analysing,
    /// All done.
    Finished(Box<LiveSummary>),
    /// The scan could not run.
    Failed {
        /// Plain-language reason.
        message: String,
    },
}

/// Reads the installed apps (on its own thread, while the disk is being read).
pub type AppsSource = Box<dyn FnOnce() -> Option<AppSnapshot> + Send>;

/// How to run a live scan.
pub struct LiveOptions {
    /// The folder to scan.
    pub root: PathBuf,
    /// Allow a whole drive (the user confirmed it).
    pub allow_drive_root: bool,
    /// Stops the scan.
    pub cancel: CancelToken,
    /// Installed apps, for owners and leftovers (optional; read in parallel with the scan).
    pub apps: Option<AppsSource>,
    /// How often batches are sent.
    pub batch_interval: Duration,
    /// Send a batch early when it holds this many entries.
    pub max_batch_entries: usize,
}

impl LiveOptions {
    /// Defaults for a folder: batches every 60 ms or 20,000 entries.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allow_drive_root: false,
            cancel: CancelToken::new(),
            apps: None,
            batch_interval: Duration::from_millis(60),
            max_batch_entries: 20_000,
        }
    }
}

/// A path without the `\\?\` prefix Windows adds to canonical paths.
#[must_use]
pub fn display_path(p: &str) -> String {
    if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        p.to_owned()
    }
}

enum Msg {
    Listing(Listing),
    Progress(Progress),
}

fn entry(n: &dc_core::Node) -> LiveEntry {
    LiveEntry {
        name: n.name.clone(),
        kind: kind_code(n.kind),
        size: own_size(n),
        mtime: n.mtime.map_or(0, |t| t.secs),
        flags: flag_bits(n),
        cat: category_index(n),
    }
}

impl std::fmt::Debug for LiveOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveOptions")
            .field("root", &self.root)
            .field("allow_drive_root", &self.allow_drive_root)
            .field("apps", &self.apps.is_some())
            .finish_non_exhaustive()
    }
}

/// Run a live scan, sending events to `sink` (from the calling thread). Returns the finished scan.
pub fn run(mut opts: LiveOptions, mut sink: impl FnMut(LiveEvent)) -> Result<ScanResult, ApiError> {
    let apps_reader = opts.apps.take().map(std::thread::spawn);
    let canonical = std::fs::canonicalize(&opts.root).ok();
    let shown_root = canonical
        .as_ref()
        .map(|p| display_path(&p.to_string_lossy()))
        .unwrap_or_else(|| opts.root.to_string_lossy().into_owned());
    let drive_root = canonical.as_ref().is_some_and(|p| p.parent().is_none());
    let volume = canonical.as_deref().and_then(crate::system::volume_usage);
    let (tx, rx) = mpsc::channel::<Msg>();
    let listing_tx = tx.clone();
    let progress_tx = tx;
    let scan_opts = ScanOptions {
        allow_drive_root: opts.allow_drive_root,
        cancel: opts.cancel.clone(),
        listing: Some(Arc::new(move |l: &Listing| {
            // A closed channel only means nobody listens any more; the scan goes on.
            let _ = listing_tx.send(Msg::Listing(l.clone()));
        })),
        progress: Some(Arc::new(move |p: &Progress| {
            let _ = progress_tx.send(Msg::Progress(p.clone()));
        })),
        progress_interval: Duration::from_millis(250),
        ..ScanOptions::default()
    };
    let root = opts.root.clone();
    let worker = std::thread::spawn(move || {
        let probe = probe();
        dc_scan::scan(&root, probe.as_ref(), &scan_opts)
    });

    sink(LiveEvent::Started {
        schema_version: LIVE_SCHEMA_VERSION,
        root: shown_root.clone(),
        drive_root,
        volume_total: volume.map(|v| v.0),
        volume_used: volume.map(|v| v.1),
    });
    let mut pending: Vec<LiveDir> = Vec::new();
    let mut pending_entries = 0usize;
    let mut last = Instant::now();
    loop {
        let msg = rx.recv_timeout(opts.batch_interval);
        match msg {
            Ok(Msg::Listing(l)) => {
                pending_entries += l.entries.len();
                pending.push(LiveDir {
                    dir: display_path(&l.dir),
                    entries: l.entries.iter().map(entry).collect(),
                });
            }
            Ok(Msg::Progress(p)) => sink(LiveEvent::Progress {
                files: p.files,
                dirs: p.dirs,
                bytes: p.bytes,
                current: display_path(&p.current_dir),
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !pending.is_empty()
            && (last.elapsed() >= opts.batch_interval || pending_entries >= opts.max_batch_entries)
        {
            sink(LiveEvent::Batch {
                dirs: std::mem::take(&mut pending),
            });
            pending_entries = 0;
            last = Instant::now();
        }
    }
    if !pending.is_empty() {
        sink(LiveEvent::Batch { dirs: pending });
    }
    let scan = match worker.join() {
        Ok(Ok(scan)) => scan,
        Ok(Err(e)) => {
            sink(LiveEvent::Failed {
                message: e.to_string(),
            });
            return Err(e.into());
        }
        Err(_) => {
            sink(LiveEvent::Failed {
                message: "the scan stopped unexpectedly".to_owned(),
            });
            return Err(ApiError::Damaged(dc_core::CoreError::InvalidTree {
                reason: "scanner thread failed".to_owned(),
            }));
        }
    };

    sink(LiveEvent::Analysing);
    // If the apps could not be read, the checks still run without owners and leftovers.
    let apps = apps_reader.and_then(|h| h.join().ok()).flatten();
    let summary = summarize(&scan, &shown_root, apps.as_ref());
    match summary {
        Ok(s) => sink(LiveEvent::Finished(Box::new(s))),
        Err(e) => {
            sink(LiveEvent::Failed {
                message: e.to_string(),
            });
            return Err(e);
        }
    }
    Ok(scan)
}

fn summarize(
    scan: &ScanResult,
    shown_root: &str,
    apps: Option<&AppSnapshot>,
) -> Result<LiveSummary, ApiError> {
    let tree = scan.tree()?;
    let paths = full_paths(&tree, shown_root);
    let path = |n: u32| paths.get(n as usize).cloned().unwrap_or_default();
    // A cancelled scan is incomplete: its checks would be unreliable, so none are run.
    let analysis = if scan.status == ScanStatus::Cancelled {
        crate::analysis::ScanAnalysis::default()
    } else {
        let classifier = Classifier::builtin_with(crate::system::protected_list());
        analyze(scan, &tree, apps, &classifier)?
    };
    Ok(LiveSummary {
        status: match scan.status {
            ScanStatus::Complete => "complete",
            ScanStatus::Partial => "partial",
            ScanStatus::Cancelled => "cancelled",
        },
        errors: scan.totals.errors,
        files: scan.totals.files,
        dirs: scan.totals.dirs,
        bytes_on_disk: scan.totals.size_on_disk_bytes,
        marks: analysis
            .marks
            .into_iter()
            .map(|m| PathMark {
                path: path(m.node),
                mark: m,
            })
            .collect(),
        owners: analysis.owners,
        assignments: analysis
            .assignments
            .iter()
            .map(|a| PathAssignment {
                path: path(a[0]),
                owner: a[1],
                confidence: a[2],
            })
            .collect(),
        uncounted: tree
            .nodes()
            .iter()
            .filter(|n| !n.counted)
            .map(|n| path(n.id))
            .collect(),
        apps_known: apps.is_some(),
    })
}

/// The file probe: on Windows it reads file ids, link counts and real sizes on disk.
fn probe() -> Box<dyn dc_scan::FileProbe> {
    #[cfg(windows)]
    {
        Box::new(dc_winmeta::WindowsProbe)
    }
    #[cfg(not(windows))]
    {
        dc_scan::default_probe()
    }
}

/// The scan root as later paths begin (for callers that need it before the scan starts).
#[must_use]
pub fn shown_root(root: &Path) -> String {
    std::fs::canonicalize(root)
        .map(|p| display_path(&p.to_string_lossy()))
        .unwrap_or_else(|_| root.to_string_lossy().into_owned())
}

/// Version of the scan format the finished scan uses.
pub const SCAN_SCHEMA_VERSION: u32 = SCHEMA_VERSION;

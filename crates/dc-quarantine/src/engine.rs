//! Remove (move into holding), restore, empty. See the crate documentation for the rules.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;

use crate::check::{self, inside, look, same_key};
use crate::journal::{self, Line, Record};
use crate::ops::Ops;
use crate::places::{Guard, Places};

const DAY: i64 = 86_400;
/// Seconds of slack when comparing change times (file systems round them).
const TIME_SLACK: i64 = 2;

/// Which group an item is in, and the person's consent for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// A finding DiskCleaner marks Safe.
    Safe,
    /// A finding marked Probably safe; `checked` is the person's tick for this item.
    ProbablySafe {
        /// Ticked "I checked this".
        checked: bool,
    },
    /// Something the person picked themselves; `understood` is "I understand these are my own
    /// files".
    YourCall {
        /// Confirmed.
        understood: bool,
    },
}

impl Group {
    fn id(self) -> &'static str {
        match self {
            Group::Safe => "safe",
            Group::ProbablySafe { .. } => "probably_safe",
            Group::YourCall { .. } => "your_call",
        }
    }
}

/// Something to remove.
#[derive(Debug, Clone)]
pub struct Item {
    /// Full path, as the scan saw it.
    pub path: PathBuf,
    /// Group and consent.
    pub group: Group,
    /// Short title (for the record and the log).
    pub title: String,
    /// Reason id.
    pub reason: String,
    /// Its latest change when it was scanned (Unix seconds; 0 unknown). For "your call" items,
    /// anything newer means it changed since the scan, and it is skipped.
    pub newest_at_scan: i64,
}

/// Something that was not removed (or not completely), and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    /// The item.
    pub path: PathBuf,
    /// Plain-language reason.
    pub why: String,
}

/// Where a held item stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum State {
    /// In the holding folder; can be restored.
    Held,
    /// Moved back.
    Restored {
        /// Where it went (the original place, or "... (restored)").
        to: PathBuf,
    },
    /// Deleted for good.
    Emptied,
}

/// An item of a cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeldItem {
    /// The holding folder it is in.
    pub root: PathBuf,
    /// Its number in the cleanup (per holding folder).
    pub n: u32,
    /// What it was.
    #[serde(flatten)]
    pub record: Record,
    /// Where it stands.
    #[serde(flatten)]
    pub state: State,
    /// Moved file by file because some files were in use: the files that moved (relative).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_files: Option<Vec<String>>,
}

/// One cleanup: everything removed in one go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cleanup {
    /// Id (also the folder name in each holding folder).
    pub id: String,
    /// When it was made (Unix seconds).
    pub created: i64,
    /// Its items.
    pub items: Vec<HeldItem>,
}

impl Cleanup {
    /// Bytes still held.
    #[must_use]
    pub fn held_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.state == State::Held)
            .map(|i| i.record.bytes)
            .sum()
    }
}

/// The result of removing.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Outcome {
    /// The cleanup id (empty if nothing moved).
    pub id: String,
    /// What is now held.
    pub held: Vec<HeldItem>,
    /// What was not moved, or not completely.
    pub skipped: Vec<Skipped>,
}

/// The result of restoring.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Restored {
    /// (original place, where it went).
    pub back: Vec<(PathBuf, PathBuf)>,
    /// What could not be restored, and notes.
    pub problems: Vec<Skipped>,
}

/// The engine. Everything that changes the disk goes through `ops`.
pub struct Engine<'a> {
    data_dir: PathBuf,
    places: &'a dyn Places,
    guard: &'a dyn Guard,
    ops: &'a dyn Ops,
}

fn now() -> (i64, u32) {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or((0, 0), |d| {
            (i64::try_from(d.as_secs()).unwrap_or(0), d.subsec_millis())
        })
}

fn plain(e: &io::Error) -> String {
    match e.raw_os_error() {
        // Windows: access denied, sharing violation, lock violation.
        Some(5) if cfg!(windows) => {
            "access denied: it needs administrator rights, or something in it is in use".to_owned()
        }
        Some(32 | 33) if cfg!(windows) => "it is in use by a program".to_owned(),
        // Windows ERROR_NOT_SAME_DEVICE, Unix EXDEV.
        Some(17) if cfg!(windows) => "it is on another drive than its holding folder".to_owned(),
        Some(18) if !cfg!(windows) => "it is on another drive than its holding folder".to_owned(),
        Some(112 | 39) if cfg!(windows) => "the drive is full".to_owned(),
        _ => match e.kind() {
            io::ErrorKind::PermissionDenied => {
                "access denied: it needs administrator rights, or it is in use".to_owned()
            }
            io::ErrorKind::NotFound => "it is not there any more".to_owned(),
            _ => format!("it could not be moved ({e})"),
        },
    }
}

/// Errors after which moving the files one by one may still work (something inside is in use).
fn worth_splitting(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5 | 32 | 33) if cfg!(windows))
        || e.kind() == io::ErrorKind::PermissionDenied
        || e.kind() == io::ErrorKind::ResourceBusy
}

/// A free name next to `wanted`: itself, or "name (restored).ext", "name (restored 2).ext", ...
fn free_name(wanted: &Path) -> PathBuf {
    if std::fs::symlink_metadata(wanted).is_err() {
        return wanted.to_path_buf();
    }
    let parent = wanted.parent().unwrap_or_else(|| Path::new(""));
    let name = wanted
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (name[..dot].to_owned(), name[dot..].to_owned()),
        _ => (name.clone(), String::new()),
    };
    for k in 1..10_000 {
        let label = if k == 1 {
            "restored".to_owned()
        } else {
            format!("restored {k}")
        };
        let candidate = parent.join(format!("{stem} ({label}){ext}"));
        if std::fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    parent.join(format!("{stem} (restored {}){ext}", now().0))
}

/// One cleanup's part in one holding folder, read from its journal (and, for steps the journal
/// does not finish, from the disk).
fn read_part(root: &Path, id: &str) -> Option<(i64, Vec<HeldItem>)> {
    let dir = root.join(id);
    let lines = journal::read(&dir.join("journal.jsonl"));
    if lines.is_empty() {
        return None;
    }
    let mut created = 0;
    let mut records: BTreeMap<u32, Record> = BTreeMap::new();
    let mut whole: BTreeMap<u32, bool> = BTreeMap::new(); // n -> moved in one step (true) / failed (false)
    let mut split: BTreeMap<u32, (Vec<String>, Option<String>)> = BTreeMap::new(); // moved files, pending file
    let mut restore: BTreeMap<u32, (PathBuf, bool)> = BTreeMap::new(); // target, done
    let mut empty: Option<bool> = None; // started, finished
    for line in lines {
        match line {
            Line::Start { time } => created = time,
            Line::Hold { n, record } => {
                records.insert(n, record);
            }
            Line::Held { n } => {
                whole.insert(n, true);
            }
            Line::Failed { n, .. } => {
                whole.entry(n).or_insert(false);
            }
            Line::Split { n } => {
                split.entry(n).or_default();
            }
            Line::File { n, rel } => {
                split.entry(n).or_default().1 = Some(rel);
            }
            Line::FileHeld { n, rel } => {
                let s = split.entry(n).or_default();
                s.0.push(rel);
                s.1 = None;
            }
            Line::Restore { n, to } => {
                restore.insert(n, (to, false));
            }
            Line::Restored { n, to } => {
                restore.insert(n, (to, true));
            }
            Line::Empty { .. } => empty = Some(false),
            Line::Emptied { .. } => empty = Some(true),
        }
    }
    let exists = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    let emptied = match empty {
        Some(true) => true,
        Some(false) => !exists(&dir.join("items")),
        None => false,
    };
    let mut items = Vec::new();
    for (n, record) in records {
        let (held, only_files) = if let Some((mut files, pending)) = split.remove(&n) {
            // A pending file whose move happened before the crash is judged by the disk.
            if let Some(rel) = pending {
                if exists(&record.held.join(&rel)) {
                    files.push(rel);
                }
            }
            (!files.is_empty(), Some(files))
        } else {
            match whole.get(&n) {
                Some(true) => (true, None),
                Some(false) => (false, None),
                // Announced but not finished: where is it now? (A rename is all or nothing.)
                None => (exists(&record.held), None),
            }
        };
        if !held {
            continue;
        }
        let state = match restore.get(&n) {
            Some((to, true)) => State::Restored { to: to.clone() },
            Some((to, false)) if exists(to) && !exists(&record.held) => {
                State::Restored { to: to.clone() }
            }
            _ if emptied => State::Emptied,
            _ => State::Held,
        };
        items.push(HeldItem {
            root: root.to_path_buf(),
            n,
            record,
            state,
            only_files,
        });
    }
    Some((created, items))
}

impl<'a> Engine<'a> {
    /// An engine keeping its list of holding folders and its log in `data_dir`.
    #[must_use]
    pub fn new(
        data_dir: PathBuf,
        places: &'a dyn Places,
        guard: &'a dyn Guard,
        ops: &'a dyn Ops,
    ) -> Self {
        Self {
            data_dir,
            places,
            guard,
            ops,
        }
    }

    fn roots_file(&self) -> PathBuf {
        self.data_dir.join("holding-roots.txt")
    }

    /// The holding folders used so far.
    #[must_use]
    pub fn roots(&self) -> Vec<PathBuf> {
        let text = std::fs::read_to_string(self.roots_file()).unwrap_or_default();
        let mut seen: Vec<PathBuf> = Vec::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let p = PathBuf::from(line);
            if !seen.iter().any(|s| same_key(s) == same_key(&p)) {
                seen.push(p);
            }
        }
        seen
    }

    fn log(&self, action: &str, path: &Path, bytes: u64, why: &str) {
        let _ = self.ops.create_dir_all(&self.data_dir);
        let line =
            json!({ "time": now().0, "action": action, "path": path, "bytes": bytes, "why": why });
        let _ = self
            .ops
            .append_line(&self.data_dir.join("cleaning-log.jsonl"), &line.to_string());
    }

    /// Is `path` (or something inside it) a holding folder or DiskCleaner's data folder?
    fn touches_our_folders(&self, path: &Path, root: Option<&Path>) -> bool {
        let mut ours: Vec<PathBuf> = self.roots();
        ours.push(self.data_dir.clone());
        if let Some(r) = root {
            ours.push(r.to_path_buf());
        }
        ours.iter().any(|o| inside(path, o) || inside(o, path))
    }

    /// Why an item may not move now (consent, protection, links, changes), or its facts.
    fn vet(&self, item: &Item) -> Result<(check::Facts, PathBuf), String> {
        match item.group {
            Group::ProbablySafe { checked: false } => {
                return Err("it is Probably safe and was not ticked as checked".into())
            }
            Group::YourCall { understood: false } => {
                return Err("removing your own files was not confirmed".into())
            }
            _ => {}
        }
        if let Some(why) = self.guard.refuse(&item.path) {
            return Err(why);
        }
        let root = self
            .places
            .holding_root(&item.path)
            .ok_or("there is no holding folder for this drive")?;
        if self.touches_our_folders(&item.path, Some(&root)) {
            return Err("it is (or holds) DiskCleaner's own holding or data folder".into());
        }
        let facts = look(&item.path)?;
        if let Group::YourCall { .. } = item.group {
            if item.newest_at_scan > 0 && facts.newest > item.newest_at_scan + TIME_SLACK {
                return Err("it changed after the scan; scan again and check it".into());
            }
        }
        Ok((facts, root))
    }

    /// Remove `items`: each is checked again, then moved into the holding folder of its drive.
    /// `progress` is called after each item with (done, total).
    pub fn remove(&self, items: &[Item], mut progress: impl FnMut(usize, usize)) -> Outcome {
        let (secs, millis) = now();
        let id = format!("{secs:012}-{millis:03}");
        let mut out = Outcome::default();
        let mut started: Vec<PathBuf> = Vec::new();
        let mut next: BTreeMap<String, u32> = BTreeMap::new();
        for (k, item) in items.iter().enumerate() {
            progress(k, items.len());
            let skip = |out: &mut Outcome, why: String| {
                self.log("skipped", &item.path, 0, &why);
                out.skipped.push(Skipped {
                    path: item.path.clone(),
                    why,
                });
            };
            let (facts, root) = match self.vet(item) {
                Ok(v) => v,
                Err(why) => {
                    skip(&mut out, why);
                    continue;
                }
            };
            let dir = root.join(&id);
            let journal_path = dir.join("journal.jsonl");
            if !started.iter().any(|r| same_key(r) == same_key(&root)) {
                let begun = self
                    .ops
                    .create_dir_all(&dir.join("items"))
                    .and_then(|()| {
                        self.ops
                            .append_line(&journal_path, &journal::text(&Line::Start { time: secs }))
                    })
                    .and_then(|()| self.ops.create_dir_all(&self.data_dir))
                    .and_then(|()| {
                        self.ops
                            .append_line(&self.roots_file(), &root.to_string_lossy())
                    });
                if let Err(e) = begun {
                    skip(
                        &mut out,
                        format!("its holding folder could not be prepared: {}", plain(&e)),
                    );
                    continue;
                }
                started.push(root.clone());
            }
            let n = {
                let c = next.entry(same_key(&root)).or_insert(0);
                *c += 1;
                *c
            };
            let name = item
                .path
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            let slot = dir.join("items").join(n.to_string());
            let held = slot.join(&name);
            let record = Record {
                original: item.path.clone(),
                held: held.clone(),
                is_dir: facts.is_dir,
                bytes: facts.bytes,
                files: facts.files,
                group: item.group.id().to_owned(),
                title: item.title.clone(),
                reason: item.reason.clone(),
            };
            let announce = self.ops.create_dir_all(&slot).and_then(|()| {
                self.ops.append_line(
                    &journal_path,
                    &journal::text(&Line::Hold {
                        n,
                        record: record.clone(),
                    }),
                )
            });
            if let Err(e) = announce {
                // Nothing moved: without its journal line, the move is not attempted.
                skip(
                    &mut out,
                    format!("the journal could not be written: {}", plain(&e)),
                );
                continue;
            }
            match self.ops.rename(&item.path, &held) {
                Ok(()) => {
                    let _ = self
                        .ops
                        .append_line(&journal_path, &journal::text(&Line::Held { n }));
                    self.log("removed", &item.path, facts.bytes, "");
                    out.held.push(HeldItem {
                        root: root.clone(),
                        n,
                        record,
                        state: State::Held,
                        only_files: None,
                    });
                }
                Err(e) if facts.is_dir && worth_splitting(&e) => {
                    let (moved, left) = self.split(&journal_path, n, &item.path, &held);
                    if moved.is_empty() {
                        let why = plain(&e);
                        let _ = self.ops.append_line(
                            &journal_path,
                            &journal::text(&Line::Failed {
                                n,
                                why: why.clone(),
                            }),
                        );
                        skip(&mut out, why);
                    } else {
                        let bytes = moved
                            .iter()
                            .map(|r| std::fs::metadata(held.join(r)).map_or(0, |m| m.len()))
                            .sum();
                        let mut record = record;
                        record.bytes = bytes;
                        record.files = moved.len() as u64;
                        if left > 0 {
                            skip(&mut out, format!("{left} files inside were in use or locked and stayed; the rest was removed"));
                        }
                        self.log(
                            "removed",
                            &item.path,
                            bytes,
                            if left > 0 {
                                "partly: files in use stayed"
                            } else {
                                ""
                            },
                        );
                        out.held.push(HeldItem {
                            root: root.clone(),
                            n,
                            record,
                            state: State::Held,
                            only_files: Some(moved),
                        });
                    }
                }
                Err(e) => {
                    let why = plain(&e);
                    let _ = self.ops.append_line(
                        &journal_path,
                        &journal::text(&Line::Failed {
                            n,
                            why: why.clone(),
                        }),
                    );
                    skip(&mut out, why);
                }
            }
        }
        progress(items.len(), items.len());
        if !out.held.is_empty() {
            out.id = id;
        }
        out
    }

    /// Move a folder's files one by one (some are in use). Returns the files moved and the
    /// number left behind.
    fn split(&self, journal_path: &Path, n: u32, from: &Path, to: &Path) -> (Vec<String>, usize) {
        if self
            .ops
            .append_line(journal_path, &journal::text(&Line::Split { n }))
            .is_err()
        {
            return (Vec::new(), 0);
        }
        let mut moved = Vec::new();
        let mut left = 0;
        for rel in check::files_inside(from) {
            let rel_text = rel.to_string_lossy().into_owned();
            let target = to.join(&rel);
            let step = target
                .parent()
                .map_or(Ok(()), |p| self.ops.create_dir_all(p))
                .and_then(|()| {
                    self.ops.append_line(
                        journal_path,
                        &journal::text(&Line::File {
                            n,
                            rel: rel_text.clone(),
                        }),
                    )
                });
            if step.is_err() {
                left += 1;
                continue;
            }
            match self.ops.rename(&from.join(&rel), &target) {
                Ok(()) => {
                    let _ = self.ops.append_line(
                        journal_path,
                        &journal::text(&Line::FileHeld {
                            n,
                            rel: rel_text.clone(),
                        }),
                    );
                    moved.push(rel_text);
                }
                Err(_) => left += 1,
            }
        }
        (moved, left)
    }

    /// Every cleanup, newest first.
    #[must_use]
    pub fn cleanups(&self) -> Vec<Cleanup> {
        let mut by_id: BTreeMap<String, Cleanup> = BTreeMap::new();
        for root in self.roots() {
            let Ok(reader) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in reader.flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if let Some((created, items)) = read_part(&root, &id) {
                    let c = by_id.entry(id.clone()).or_insert(Cleanup {
                        id,
                        created,
                        items: Vec::new(),
                    });
                    c.created = c.created.max(created);
                    c.items.extend(items);
                }
            }
        }
        let mut list: Vec<Cleanup> = by_id
            .into_values()
            .filter(|c| !c.items.is_empty())
            .collect();
        list.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        list
    }

    /// Put held items of a cleanup back (all of them, or those `(root, n)` listed). An item whose
    /// place is taken comes back as "name (restored)".
    pub fn restore(&self, id: &str, only: Option<&[(PathBuf, u32)]>) -> Restored {
        let mut out = Restored::default();
        let Some(cleanup) = self.cleanups().into_iter().find(|c| c.id == id) else {
            out.problems.push(Skipped {
                path: PathBuf::from(id),
                why: "there is no such cleanup".into(),
            });
            return out;
        };
        for item in cleanup.items {
            if item.state != State::Held {
                continue;
            }
            if let Some(list) = only {
                if !list
                    .iter()
                    .any(|(r, n)| *n == item.n && same_key(r) == same_key(&item.root))
                {
                    continue;
                }
            }
            let journal_path = item.root.join(id).join("journal.jsonl");
            let original = &item.record.original;
            let problem = |out: &mut Restored, why: String| {
                out.problems.push(Skipped {
                    path: original.clone(),
                    why,
                });
            };
            // The folder it goes back into must be where it was (not a link to somewhere else).
            let parent = original.parent().unwrap_or_else(|| Path::new(""));
            if let Err(e) = self.ops.create_dir_all(parent) {
                problem(
                    &mut out,
                    format!("its folder could not be made again: {}", plain(&e)),
                );
                continue;
            }
            match std::fs::canonicalize(parent) {
                Ok(real) if same_key(&real) == same_key(parent) => {}
                _ => {
                    problem(&mut out, "its folder is now a link or cannot be found; restore it by hand from the holding folder".into());
                    continue;
                }
            }
            if let Some(files) = &item.only_files {
                // Split items: each file goes back to its place inside the original folder.
                let mut back = 0usize;
                for rel in files {
                    let from = item.record.held.join(rel);
                    let to = free_name(&original.join(rel));
                    let step = to
                        .parent()
                        .map_or(Ok(()), |p| self.ops.create_dir_all(p))
                        .and_then(|()| self.ops.rename(&from, &to));
                    match step {
                        Ok(()) => back += 1,
                        Err(e) => problem(
                            &mut out,
                            format!("{rel} could not be restored: {}", plain(&e)),
                        ),
                    }
                }
                if back == files.len() {
                    let _ = self.ops.append_line(
                        &journal_path,
                        &journal::text(&Line::Restored {
                            n: item.n,
                            to: original.clone(),
                        }),
                    );
                    self.log("restored", original, item.record.bytes, "");
                    out.back.push((original.clone(), original.clone()));
                }
                continue;
            }
            match look(&item.record.held) {
                Ok(f) if f.files != item.record.files || f.bytes != item.record.bytes => problem(
                    &mut out,
                    "it changed while it was held (restored anyway)".into(),
                ),
                Ok(_) => {}
                Err(why) => {
                    problem(&mut out, format!("the held copy cannot be used: {why}"));
                    continue;
                }
            }
            let to = free_name(original);
            if self
                .ops
                .append_line(
                    &journal_path,
                    &journal::text(&Line::Restore {
                        n: item.n,
                        to: to.clone(),
                    }),
                )
                .is_err()
            {
                problem(&mut out, "the journal could not be written".into());
                continue;
            }
            match self.ops.rename(&item.record.held, &to) {
                Ok(()) => {
                    let _ = self.ops.append_line(
                        &journal_path,
                        &journal::text(&Line::Restored {
                            n: item.n,
                            to: to.clone(),
                        }),
                    );
                    self.log("restored", &to, item.record.bytes, "");
                    out.back.push((original.clone(), to));
                }
                Err(e) => problem(&mut out, plain(&e)),
            }
        }
        out
    }

    /// Delete a cleanup's held items for good. Only ever inside a holding folder. Returns the
    /// bytes freed.
    pub fn empty(&self, id: &str) -> Result<u64, String> {
        if id.is_empty() || id.contains(['/', '\\', '.']) {
            return Err("not a cleanup id".into());
        }
        let Some(cleanup) = self.cleanups().into_iter().find(|c| c.id == id) else {
            return Err("there is no such cleanup".into());
        };
        let mut freed = 0;
        let roots: Vec<PathBuf> = {
            let mut r: Vec<PathBuf> = cleanup.items.iter().map(|i| i.root.clone()).collect();
            r.sort();
            r.dedup();
            r
        };
        for root in roots {
            let bytes: u64 = cleanup
                .items
                .iter()
                .filter(|i| i.state == State::Held && same_key(&i.root) == same_key(&root))
                .map(|i| i.record.bytes)
                .sum();
            let dir = root.join(id);
            let items = dir.join("items");
            // Never follow a link, never delete outside the holding folder.
            for p in [&root, &dir, &items] {
                match std::fs::symlink_metadata(p) {
                    Ok(m) if check::is_link(&m) => {
                        return Err(format!("{} is a link; nothing was deleted", p.display()))
                    }
                    Ok(_) => {}
                    Err(_) if p == &items => break,
                    Err(e) => return Err(format!("{} cannot be read ({e})", p.display())),
                }
            }
            if std::fs::symlink_metadata(&items).is_err() {
                continue;
            }
            let (real_root, real_items) =
                match (std::fs::canonicalize(&root), std::fs::canonicalize(&items)) {
                    (Ok(r), Ok(i)) => (r, i),
                    _ => {
                        return Err("the holding folder cannot be found; nothing was deleted".into())
                    }
                };
            if !inside(&real_items, &real_root) || same_key(&real_items) == same_key(&real_root) {
                return Err(
                    "the items are not inside the holding folder; nothing was deleted".into(),
                );
            }
            let journal_path = dir.join("journal.jsonl");
            let time = now().0;
            self.ops
                .append_line(&journal_path, &journal::text(&Line::Empty { time }))
                .map_err(|e| format!("the journal could not be written: {}", plain(&e)))?;
            self.ops
                .remove_tree(&items)
                .map_err(|e| format!("some held items could not be deleted: {}", plain(&e)))?;
            let _ = self
                .ops
                .append_line(&journal_path, &journal::text(&Line::Emptied { time }));
            self.log("emptied", &dir, bytes, "");
            freed += bytes;
        }
        Ok(freed)
    }

    /// Empty every cleanup older than `days`. Returns (id, bytes freed) for each one emptied.
    pub fn empty_expired(&self, days: u32, now_secs: i64) -> Vec<(String, u64)> {
        let limit = now_secs - i64::from(days) * DAY;
        self.cleanups()
            .into_iter()
            .filter(|c| c.created <= limit && c.items.iter().any(|i| i.state == State::Held))
            .filter_map(|c| self.empty(&c.id).ok().map(|b| (c.id, b)))
            .collect()
    }
}

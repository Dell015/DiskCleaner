//! Moving an app's folder to another drive (B11, D141; `docs/phases/b11-move-apps/design.md`).
//!
//! **SAFETY-CRITICAL.** The order never changes:
//! 1. check everything (no links, nothing in use, room on the other drive, NTFS, ...);
//! 2. copy the folder to the other drive under a temporary name, and compare every byte;
//! 3. give the copy its real name;
//! 4. move the original (a rename on its own drive) into that drive's holding folder;
//! 5. leave a junction where the original was (apps), so shortcuts and the uninstaller work;
//! 6. wait for the person: "It works" deletes the held original (inside the holding folder);
//!    "Move back" undoes it, at any time.
//!
//! Every step that changes the disk is written to the move's journal first
//! (`<data>\moves\<id>.jsonl`). A copy that is thrown away is first moved into its own drive's
//! holding folder and deleted only there. After a crash, [`Mover::recover`] reads the journal and
//! the disk and finishes safely: forward only when the app already works from the new place,
//! otherwise back to how it was.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::check::{self, inside, look, same_key};
use crate::engine::{is_real_dir, now, plain, Engine};
use crate::ops::{MoveOps, Volume};
use crate::places::{drive_of, Guard, Places};

/// Room needed on the other drive: the folder's size plus this share (10%, D141 Q7).
pub const ROOM_EXTRA_PERCENT: u64 = 10;

/// What to move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRequest {
    /// The app's name (for the record and the messages).
    pub app: String,
    /// Its folder (the registered install folder; checked by the caller, `dc_api::moving`).
    pub source: PathBuf,
    /// The new place (must not exist; on another drive).
    pub dest: PathBuf,
    /// Leave a junction at the old place (apps). `false` for games whose launcher is told the
    /// new place by the person ("Locate game").
    pub link: bool,
}

/// What a move would do, after every check passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Bytes to copy.
    pub bytes: u64,
    /// Files to copy.
    pub files: u64,
    /// Free bytes on the other drive.
    pub free: u64,
    /// The other drive is removable (USB): the app stops working while it is unplugged.
    pub removable: bool,
}

/// Where a move stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MoveState {
    /// Being moved, or Rhizome stopped during the move (see [`Mover::recover`]). `step` is the
    /// last step started.
    Moving {
        /// Last step started.
        step: String,
    },
    /// Done, and waiting for the person to check the app ("It works" or "Move back").
    Waiting,
    /// Checked by the person; the old copy is deleted.
    Done,
    /// Being moved back, or Rhizome stopped during that.
    MovingBack {
        /// Last step started.
        step: String,
    },
    /// Moved back to where it was; the copy on the other drive is deleted.
    MovedBack,
    /// Not moved; everything is where it was.
    Failed {
        /// Why.
        why: String,
    },
}

/// One move, read back from its journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveInfo {
    /// Id (also the journal's name).
    pub id: String,
    /// The app.
    pub app: String,
    /// Where it was.
    pub source: PathBuf,
    /// Where it went.
    pub dest: PathBuf,
    /// A junction was left at the old place.
    pub link: bool,
    /// Bytes.
    pub bytes: u64,
    /// Files.
    pub files: u64,
    /// When it started (Unix seconds).
    pub created: i64,
    /// Where the original waits until "It works".
    pub held: PathBuf,
    /// Where it stands.
    #[serde(flatten)]
    pub state: MoveState,
    /// Things the person should know (a move back that did not finish, a copy that could not be
    /// deleted, ...).
    pub notes: Vec<String>,
}

/// One journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Line {
    /// The move started (written before anything is copied).
    Start {
        time: i64,
        app: String,
        source: PathBuf,
        dest: PathBuf,
        link: bool,
        bytes: u64,
        files: u64,
        /// The temporary name of the copy.
        partial: PathBuf,
        /// Where the original will wait.
        held: PathBuf,
    },
    /// The copy is complete.
    Copied,
    /// The copy matches the original byte for byte.
    Verified,
    /// About to give the copy its real name.
    Name,
    /// Done.
    Named,
    /// About to move the original into the holding folder.
    Hold,
    /// Done.
    Held,
    /// About to make the junction.
    Link,
    /// Done: the app now runs from the new place.
    Linked,
    /// The move ended without moving the app (`why`); everything is where it was.
    Failed { why: String },
    /// About to delete the held original ("It works").
    Confirm,
    /// Done.
    Confirmed,
    /// Moving back started; `from` is "waiting" or "done".
    Back { from: String },
    /// Moving back from "done": the temporary copy at the old place.
    BackCopy { partial: PathBuf },
    /// That copy is complete and matches byte for byte.
    BackVerified,
    /// About to remove the junction.
    Unlink,
    /// Done.
    Unlinked,
    /// About to put the original (held, or the copy back) at the old place.
    Return,
    /// Done.
    Returned,
    /// Moving back stopped; the app still runs from the new place (`why`).
    BackFailed { why: String },
    /// Moved back; the copy on the other drive is (being) deleted.
    MovedBack,
    /// About to move `what` into its drive's holding folder (`to`) and delete it there.
    Discard { what: PathBuf, to: PathBuf },
    /// Done.
    Discarded { what: PathBuf },
    /// It could not be deleted (`why`); it is still there.
    DiscardFailed { what: PathBuf, why: String },
}

fn text(line: &Line) -> String {
    serde_json::to_string(line).unwrap_or_default()
}

fn parse(text: &str) -> Vec<Line> {
    // Same tolerant reading as the removal journal: every readable record is kept.
    const START: &str = "{\"op\":";
    let mut out = Vec::new();
    for line in text.lines() {
        let mut at = 0;
        while let Some(found) = line[at..].find(START) {
            let begin = at + found;
            let mut stream = serde_json::Deserializer::from_str(&line[begin..]).into_iter::<Line>();
            match stream.next() {
                Some(Ok(record)) => {
                    out.push(record);
                    at = begin + stream.byte_offset();
                }
                _ => at = begin + START.len(),
            }
        }
    }
    out
}

/// The steps seen in a journal, and the move they describe.
#[derive(Debug, Clone, Default)]
struct Seen {
    info: Option<MoveInfo>,
    partial: PathBuf,
    back_partial: Option<PathBuf>,
    ops: Vec<&'static str>,
    back_from: Option<String>,
    /// Discards started and not finished: (what, to).
    open_discards: Vec<(PathBuf, PathBuf)>,
}

impl Seen {
    fn has(&self, op: &str) -> bool {
        self.ops.contains(&op)
    }
    /// Steps seen since the last "back" (or since the start).
    fn since_back(&self, op: &str) -> bool {
        let start = self.ops.iter().rposition(|o| *o == "back").unwrap_or(0);
        self.ops[start..].contains(&op)
    }
}

fn op_name(line: &Line) -> &'static str {
    match line {
        Line::Start { .. } => "start",
        Line::Copied => "copied",
        Line::Verified => "verified",
        Line::Name => "name",
        Line::Named => "named",
        Line::Hold => "hold",
        Line::Held => "held",
        Line::Link => "link",
        Line::Linked => "linked",
        Line::Failed { .. } => "failed",
        Line::Confirm => "confirm",
        Line::Confirmed => "confirmed",
        Line::Back { .. } => "back",
        Line::BackCopy { .. } => "back_copy",
        Line::BackVerified => "back_verified",
        Line::Unlink => "unlink",
        Line::Unlinked => "unlinked",
        Line::Return => "return",
        Line::Returned => "returned",
        Line::BackFailed { .. } => "back_failed",
        Line::MovedBack => "moved_back",
        Line::Discard { .. } => "discard",
        Line::Discarded { .. } => "discarded",
        Line::DiscardFailed { .. } => "discard_failed",
    }
}

fn read_seen(id: &str, lines: &[Line]) -> Seen {
    let mut seen = Seen::default();
    for line in lines {
        seen.ops.push(op_name(line));
        let Some(info) = seen.info.as_mut() else {
            if let Line::Start {
                time,
                app,
                source,
                dest,
                link,
                bytes,
                files,
                partial,
                held,
            } = line
            {
                seen.partial = partial.clone();
                seen.info = Some(MoveInfo {
                    id: id.to_owned(),
                    app: app.clone(),
                    source: source.clone(),
                    dest: dest.clone(),
                    link: *link,
                    bytes: *bytes,
                    files: *files,
                    created: *time,
                    held: held.clone(),
                    state: MoveState::Moving {
                        step: "copy".into(),
                    },
                    notes: Vec::new(),
                });
            }
            continue;
        };
        let step = |s: &str| MoveState::Moving { step: s.into() };
        let back = |s: &str| MoveState::MovingBack { step: s.into() };
        match line {
            Line::Start { .. } => {}
            Line::Copied => info.state = step("check"),
            Line::Verified | Line::Name => info.state = step("name"),
            Line::Named | Line::Hold => info.state = step("hold"),
            Line::Held => {
                info.state = if info.link {
                    step("link")
                } else {
                    MoveState::Waiting
                };
            }
            Line::Link => info.state = step("link"),
            Line::Linked => info.state = MoveState::Waiting,
            Line::Failed { why } => info.state = MoveState::Failed { why: why.clone() },
            Line::Confirm => info.state = step("confirm"),
            Line::Confirmed => info.state = MoveState::Done,
            Line::Back { from } => {
                seen.back_from = Some(from.clone());
                info.state = back("start");
            }
            Line::BackCopy { partial } => {
                seen.back_partial = Some(partial.clone());
                info.state = back("copy");
            }
            Line::BackVerified => info.state = back("verified"),
            Line::Unlink | Line::Unlinked => info.state = back("unlink"),
            Line::Return | Line::Returned => info.state = back("return"),
            Line::BackFailed { why } => {
                info.notes
                    .push(format!("Moving back did not finish: {why}"));
                info.state = if seen.back_from.as_deref() == Some("done") {
                    MoveState::Done
                } else {
                    MoveState::Waiting
                };
                seen.back_from = None;
            }
            Line::MovedBack => info.state = MoveState::MovedBack,
            Line::Discard { what, to } => seen.open_discards.push((what.clone(), to.clone())),
            Line::Discarded { what } => seen
                .open_discards
                .retain(|(w, _)| same_key(w) != same_key(what)),
            Line::DiscardFailed { what, why } => {
                seen.open_discards
                    .retain(|(w, _)| same_key(w) != same_key(what));
                info.notes.push(format!(
                    "{} could not be deleted ({why}); delete it by hand.",
                    what.display()
                ));
            }
        }
    }
    seen
}

/// The REPARSE_DATA_BUFFER of a junction to `target` (`D:\x`): tag, sizes, then the name the
/// system follows (`\??\D:\x`) and the name it shows (`D:\x`), each ending in a NUL.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn mount_point_buffer(target: &str) -> io::Result<Vec<u8>> {
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    let subst: Vec<u16> = format!(r"\??\{target}").encode_utf16().collect();
    let print: Vec<u16> = target.encode_utf16().collect();
    let too_long = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path is too long for a link",
        )
    };
    let subst_bytes = u16::try_from(subst.len() * 2).map_err(|_| too_long())?;
    let print_bytes = u16::try_from(print.len() * 2).map_err(|_| too_long())?;
    let path_bytes = (subst.len() + 1 + print.len() + 1) * 2;
    // The four u16 offsets/lengths plus the names.
    let data_len = u16::try_from(8 + path_bytes).map_err(|_| too_long())?;
    let mut buf = Vec::with_capacity(8 + usize::from(data_len));
    buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buf.extend_from_slice(&data_len.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved
    buf.extend_from_slice(&0u16.to_le_bytes()); // substitute name offset
    buf.extend_from_slice(&subst_bytes.to_le_bytes());
    buf.extend_from_slice(&(subst_bytes + 2).to_le_bytes()); // print name offset (after the NUL)
    buf.extend_from_slice(&print_bytes.to_le_bytes());
    for unit in subst.iter().chain(&[0]).chain(&print).chain(&[0]) {
        buf.extend_from_slice(&unit.to_le_bytes());
    }
    if buf.len() > 16 * 1024 {
        return Err(too_long());
    }
    Ok(buf)
}

/// Where a junction's reparse data points (`D:\x`), or `None` if it is not a junction to a
/// drive path (another kind of link, a volume GUID, damaged data).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_mount_point(buf: &[u8]) -> Option<String> {
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    let u32_at =
        |i: usize| -> Option<u32> { Some(u32::from_le_bytes(buf.get(i..i + 4)?.try_into().ok()?)) };
    let u16_at = |i: usize| -> Option<usize> {
        Some(usize::from(u16::from_le_bytes(
            buf.get(i..i + 2)?.try_into().ok()?,
        )))
    };
    if u32_at(0)? != IO_REPARSE_TAG_MOUNT_POINT {
        return None;
    }
    let (off, len) = (u16_at(8)?, u16_at(10)?);
    let start = 16usize.checked_add(off)?;
    let bytes = buf.get(start..start.checked_add(len)?)?;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let name = String::from_utf16(&units).ok()?;
    let path = name.strip_prefix(r"\??\")?;
    drive_of(Path::new(path))?;
    Some(path.to_owned())
}

/// Moves apps. Uses the removal engine's lock, holding folders, protected list and log.
pub struct Mover<'a> {
    e: Engine<'a>,
    ops: &'a dyn MoveOps,
}

/// Compare two files byte for byte.
fn same_bytes(a: &Path, b: &Path) -> io::Result<bool> {
    let (mut fa, mut fb) = (fs::File::open(a)?, fs::File::open(b)?);
    if fa.metadata()?.len() != fb.metadata()?.len() {
        return Ok(false);
    }
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

fn read_full(f: &mut fs::File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Every entry under `root` (relative), folders first in each level; links refuse.
fn entries(root: &Path) -> Result<Vec<(PathBuf, bool)>, String> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        let reader =
            fs::read_dir(&dir).map_err(|e| format!("{} cannot be read ({e})", dir.display()))?;
        let mut here: Vec<_> = reader
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{} cannot be read ({e})", dir.display()))?;
        here.sort();
        for name in here {
            let child = rel.join(&name);
            let meta = fs::symlink_metadata(root.join(&child))
                .map_err(|e| format!("{} cannot be read ({e})", child.display()))?;
            if check::is_link(&meta) {
                return Err(format!(
                    "it contains a link or cloud-only file ({})",
                    root.join(&child).display()
                ));
            }
            out.push((child.clone(), meta.is_dir()));
            if meta.is_dir() {
                stack.push(child);
            }
        }
    }
    Ok(out)
}

fn in_use(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33) if cfg!(windows))
        || e.kind() == io::ErrorKind::ResourceBusy
}

impl<'a> Mover<'a> {
    /// A mover keeping its journals in `data_dir\moves`.
    #[must_use]
    pub fn new(
        data_dir: PathBuf,
        places: &'a dyn Places,
        guard: &'a dyn Guard,
        ops: &'a dyn MoveOps,
    ) -> Self {
        Self {
            e: Engine::new(data_dir, places, guard, ops),
            ops,
        }
    }

    fn journal_path(&self, id: &str) -> PathBuf {
        self.e.data_dir().join("moves").join(format!("{id}.jsonl"))
    }

    fn write(&self, id: &str, line: &Line) -> Result<(), String> {
        self.ops
            .append_line(&self.journal_path(id), &text(line))
            .map_err(|e| format!("the move's journal could not be written: {}", plain(&e)))
    }

    /// Every check that can be made before anything changes. `Ok` is what the move would do.
    pub fn check(&self, req: &MoveRequest) -> Result<Plan, String> {
        let (src, dst) = (&req.source, &req.dest);
        let sd = self
            .ops
            .drive(src)
            .ok_or("its folder is not on a drive with a letter")?;
        let dd = self
            .ops
            .drive(dst)
            .ok_or("the new place is not on a drive with a letter")?;
        if sd == dd {
            return Err("the new place is on the same drive; pick another drive".into());
        }
        if src.parent().and_then(Path::parent).is_none() {
            return Err("a drive's top folder cannot be moved".into());
        }
        if let Some(why) = self.e.guard().refuse(src) {
            return Err(why);
        }
        if self.e.touches_our_folders(src, None) || self.e.touches_our_folders(dst, None) {
            return Err("it is (or holds) Rhizome's own holding or data folder".into());
        }
        let facts = look(src)?;
        if !facts.is_dir {
            return Err("it is not a folder".into());
        }
        // A program that is running keeps its files open: nothing may be in use.
        for rel in check::files_inside(src) {
            let ext = rel
                .extension()
                .map(|x| x.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if ext == "sys" {
                return Err(format!(
                    "it has a driver ({}); drivers stay where Windows installed them",
                    rel.display()
                ));
            }
            if let Err(err) = self.ops.open_alone(&src.join(&rel)) {
                return Err(if in_use(&err) {
                    format!(
                        "{} is in use ({}). Close {} (also in the taskbar's hidden icons) and try again",
                        req.app,
                        rel.display(),
                        req.app
                    )
                } else {
                    format!("{} cannot be read ({})", rel.display(), plain(&err))
                });
            }
        }
        // The new place: a plain full path (no "." or ".." parts), not there yet.
        if dst.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        }) || dst.file_name().is_none()
        {
            return Err("the new place must be a plain full folder path".into());
        }
        if fs::symlink_metadata(dst).is_ok() {
            return Err(format!(
                "{} already exists; pick another place",
                dst.display()
            ));
        }
        let mut near = dst.parent().map(Path::to_path_buf);
        while let Some(p) = near.clone() {
            if fs::symlink_metadata(&p).is_ok() {
                break;
            }
            near = p.parent().map(Path::to_path_buf);
        }
        let near = near.ok_or("the new place's drive cannot be found")?;
        if !is_real_dir(&near) {
            return Err(format!("{} is a link or not a folder", near.display()));
        }
        // A drive's top is protected as itself only; folders are made inside it.
        let drive_top = near.parent().is_none();
        if let Some(why) = (!drive_top).then(|| self.e.guard().refuse(&near)).flatten() {
            return Err(format!("the new place is not allowed: {why}"));
        }
        let vol: Volume = self
            .ops
            .volume(dst)
            .map_err(|e| format!("the other drive cannot be read ({})", plain(&e)))?;
        if vol.network {
            return Err("network drives are not used for moving apps".into());
        }
        if !vol.filesystem.eq_ignore_ascii_case("NTFS") {
            return Err(format!(
                "the other drive is formatted {}; apps can only be moved to an NTFS drive",
                if vol.filesystem.is_empty() {
                    "in an unknown way"
                } else {
                    &vol.filesystem
                }
            ));
        }
        let need = facts
            .bytes
            .saturating_add(facts.bytes / 100 * ROOM_EXTRA_PERCENT);
        if vol.free < need {
            return Err(format!(
                "the other drive has {} bytes free; {} are needed (the app plus 10%)",
                vol.free, need
            ));
        }
        let src_vol = self
            .ops
            .volume(src)
            .map_err(|e| format!("its drive cannot be read ({})", plain(&e)))?;
        if src_vol.network {
            return Err("apps on network drives are not moved".into());
        }
        let hold = self
            .e
            .places()
            .holding_root(src)
            .ok_or("its drive has no holding folder")?;
        if self.ops.drive(&hold).as_ref() != Some(&sd) {
            return Err("its drive's holding folder is on another drive".into());
        }
        self.e
            .places()
            .holding_root(dst)
            .ok_or("the other drive has no holding folder")?;
        Ok(Plan {
            bytes: facts.bytes,
            files: facts.files,
            free: vol.free,
            removable: vol.removable,
        })
    }

    /// Copy `from` (a folder) to the new folder `to`, file by file. `progress` gets bytes done.
    fn copy_tree(
        &self,
        from: &Path,
        to: &Path,
        total: u64,
        progress: &mut dyn FnMut(&str, u64, u64),
    ) -> Result<(), String> {
        let list = entries(from)?;
        self.ops
            .create_dir(to)
            .map_err(|e| format!("the copy could not be started: {}", plain(&e)))?;
        let mut done = 0u64;
        progress("copy", 0, total);
        for (rel, is_dir) in list {
            let (a, b) = (from.join(&rel), to.join(&rel));
            if is_dir {
                self.ops
                    .create_dir(&b)
                    .map_err(|e| format!("{} could not be made: {}", b.display(), plain(&e)))?;
            } else {
                self.ops
                    .copy_file(&a, &b)
                    .map_err(|e| format!("{} could not be copied: {}", rel.display(), plain(&e)))?;
                done += fs::symlink_metadata(&b).map_or(0, |m| m.len());
                progress("copy", done, total);
            }
        }
        Ok(())
    }

    /// Is `copy` the same as `original`: the same entries, and every file the same bytes?
    fn same_tree(
        original: &Path,
        copy: &Path,
        total: u64,
        progress: &mut dyn FnMut(&str, u64, u64),
    ) -> Result<(), String> {
        let a = entries(original)?;
        let b = entries(copy)?;
        let key = |v: &[(PathBuf, bool)]| -> Vec<(String, bool)> {
            let mut k: Vec<_> = v.iter().map(|(p, d)| (same_key(p), *d)).collect();
            k.sort();
            k
        };
        if key(&a) != key(&b) {
            return Err("the copy does not have the same files as the original".into());
        }
        let mut done = 0u64;
        progress("check", 0, total);
        for (rel, is_dir) in &a {
            if *is_dir {
                continue;
            }
            match same_bytes(&original.join(rel), &copy.join(rel)) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(format!("{} differs in the copy", rel.display()));
                }
                Err(e) => return Err(format!("{} could not be compared ({e})", rel.display())),
            }
            done += fs::symlink_metadata(original.join(rel)).map_or(0, |m| m.len());
            progress("check", done, total);
        }
        Ok(())
    }

    /// Move `what` into its own drive's holding folder (`move-<id>\<label>`) and delete it
    /// there: the only way a copy is ever deleted. Written to the journal first.
    fn discard(&self, id: &str, what: &Path, label: &str) -> Result<(), String> {
        if fs::symlink_metadata(what).is_err() {
            return Ok(());
        }
        let root = self
            .e
            .places()
            .holding_root(what)
            .ok_or("its drive has no holding folder")?;
        let dir = root.join(format!("move-{id}"));
        let to = dir.join(label);
        let result = (|| -> Result<(), String> {
            self.e
                .safe_dir(&dir, "its holding folder is a link; nothing was deleted")?;
            if let Err(e) = self.ops.protect_dir(&root) {
                self.e
                    .log("note", &root, 0, &format!("could not protect: {e}"));
            }
            let _ = self
                .ops
                .append_line(&self.e.roots_file(), &root.to_string_lossy());
            if fs::symlink_metadata(&to).is_ok() {
                return Err(format!("{} is already used", to.display()));
            }
            self.write(
                id,
                &Line::Discard {
                    what: what.to_path_buf(),
                    to: to.clone(),
                },
            )?;
            self.ops.rename(what, &to).map_err(|e| plain(&e))?;
            self.delete_in_holding(&dir, &root)
        })();
        match result {
            Ok(()) => {
                self.write(
                    id,
                    &Line::Discarded {
                        what: what.to_path_buf(),
                    },
                )?;
                self.e.log(
                    "discarded",
                    what,
                    0,
                    "a copy no longer needed (moving an app)",
                );
                Ok(())
            }
            Err(why) => {
                let _ = self.write(
                    id,
                    &Line::DiscardFailed {
                        what: what.to_path_buf(),
                        why: why.clone(),
                    },
                );
                Err(why)
            }
        }
    }

    /// Delete `dir`, which must be a real folder inside the holding folder `root`.
    fn delete_in_holding(&self, dir: &Path, root: &Path) -> Result<(), String> {
        if fs::symlink_metadata(dir).is_err() {
            return Ok(());
        }
        if !inside(dir, root) || same_key(dir) == same_key(root) || !is_real_dir(dir) {
            return Err(
                "it is not a real folder inside a holding folder; nothing was deleted".into(),
            );
        }
        self.ops
            .remove_tree(dir)
            .map_err(|e| format!("it could not be deleted: {}", plain(&e)))
    }

    /// Is the old place Rhizome's junction to the new place?
    fn our_link(&self, info: &MoveInfo) -> bool {
        matches!(self.ops.read_link(&info.source), Ok(Some(t)) if same_key(&t) == same_key(&info.dest))
    }

    /// Move an app. `progress` gets ("copy" or "check", bytes done, bytes in all).
    pub fn start(
        &self,
        req: &MoveRequest,
        mut progress: impl FnMut(&str, u64, u64),
    ) -> Result<MoveInfo, String> {
        let _lock = self.e.lock()?;
        let plan = self.check(req)?;
        let (secs, millis) = now();
        let id = format!("{secs:012}-{millis:03}-{}", std::process::id());
        let name = req
            .source
            .file_name()
            .ok_or("its folder has no name")?
            .to_os_string();
        let hold_root = self
            .e
            .places()
            .holding_root(&req.source)
            .ok_or("its drive has no holding folder")?;
        let hold_dir = hold_root.join(format!("move-{id}")).join("original");
        let held = hold_dir.join(&name);
        let dest_parent = req
            .dest
            .parent()
            .ok_or("the new place has no folder above it")?;
        let partial = dest_parent.join(format!(".rhizome-moving-{id}"));
        // Folders that must exist first (each checked to be a real folder, never a link).
        self.e
            .safe_dir(&hold_dir, "its holding folder is a link; nothing was moved")?;
        if let Err(e) = self.ops.protect_dir(&hold_root) {
            self.e
                .log("note", &hold_root, 0, &format!("could not protect: {e}"));
        }
        self.ops
            .append_line(&self.e.roots_file(), &hold_root.to_string_lossy())
            .map_err(|e| format!("Rhizome's data folder cannot be used: {}", plain(&e)))?;
        self.e.safe_dir(
            dest_parent,
            "the new place's folder is a link; nothing was moved",
        )?;
        self.ops
            .create_dir_all(&self.e.data_dir().join("moves"))
            .map_err(|e| format!("Rhizome's data folder cannot be used: {}", plain(&e)))?;
        self.write(
            &id,
            &Line::Start {
                time: secs,
                app: req.app.clone(),
                source: req.source.clone(),
                dest: req.dest.clone(),
                link: req.link,
                bytes: plan.bytes,
                files: plan.files,
                partial: partial.clone(),
                held: held.clone(),
            },
        )?;
        self.e.log(
            "move-start",
            &req.source,
            plan.bytes,
            &format!("to {}", req.dest.display()),
        );
        let fail = |why: String, discard: &[&Path]| -> Result<MoveInfo, String> {
            for d in discard {
                let label = if same_key(d) == same_key(&partial) {
                    "copy"
                } else {
                    "new"
                };
                if let Err(e) = self.discard(&id, d, label) {
                    self.e
                        .log("note", d, 0, &format!("could not delete the copy: {e}"));
                }
            }
            let _ = self.write(&id, &Line::Failed { why: why.clone() });
            self.e.log("move-failed", &req.source, 0, &why);
            Err(why)
        };
        // 1. Copy, then compare every byte.
        if let Err(why) = self.copy_tree(&req.source, &partial, plan.bytes, &mut progress) {
            return fail(format!("the copy failed: {why}"), &[&partial]);
        }
        if let Err(e) = self.write(&id, &Line::Copied) {
            return fail(e, &[&partial]);
        }
        if let Err(why) = Self::same_tree(&req.source, &partial, plan.bytes, &mut progress) {
            return fail(
                format!("the copy is not the same as the original: {why}"),
                &[&partial],
            );
        }
        if let Err(e) = self.write(&id, &Line::Verified) {
            return fail(e, &[&partial]);
        }
        // 2. The copy gets its real name.
        if let Err(e) = self.write(&id, &Line::Name) {
            return fail(e, &[&partial]);
        }
        if let Err(e) = self.ops.rename(&partial, &req.dest) {
            return fail(
                format!("the copy could not be named: {}", plain(&e)),
                &[&partial],
            );
        }
        if let Err(e) = self.write(&id, &Line::Named) {
            return fail(e, &[&req.dest]);
        }
        // 3. The original waits in its drive's holding folder.
        if let Err(e) = self.write(&id, &Line::Hold) {
            return fail(e, &[&req.dest]);
        }
        if let Err(e) = self.ops.rename(&req.source, &held) {
            let why = if in_use(&e) || e.kind() == io::ErrorKind::PermissionDenied {
                format!(
                    "the original could not be moved aside ({}). Close {} and try again; if it is in Program Files, moving needs administrator rights, which Rhizome does not have",
                    plain(&e),
                    req.app
                )
            } else {
                format!("the original could not be moved aside: {}", plain(&e))
            };
            return fail(why, &[&req.dest]);
        }
        if let Err(e) = self.write(&id, &Line::Held) {
            // The journal cannot be written: put everything back as it was.
            return self.undo_hold(&id, req, &held, e);
        }
        // 4. The link, so the app keeps working from the old path.
        if req.link {
            if let Err(e) = self.write(&id, &Line::Link) {
                return self.undo_hold(&id, req, &held, e);
            }
            if let Err(e) = self.ops.make_link(&req.source, &req.dest) {
                return self.undo_hold(
                    &id,
                    req,
                    &held,
                    format!("the link could not be made: {}", plain(&e)),
                );
            }
            if let Err(e) = self.write(&id, &Line::Linked) {
                // The app works from the new place; the journal will say "link" and recovery
                // checks the link on the disk.
                self.e.log("note", &req.source, 0, &e);
            }
        }
        self.e.log(
            "moved",
            &req.source,
            plan.bytes,
            &format!("to {}", req.dest.display()),
        );
        self.info(&id)
            .ok_or_else(|| "the move's journal cannot be read".to_owned())
    }

    /// The original was held but the move cannot go on: put it back, delete the new copy.
    fn undo_hold(
        &self,
        id: &str,
        req: &MoveRequest,
        held: &Path,
        why: String,
    ) -> Result<MoveInfo, String> {
        let _ = self.write(id, &Line::Return);
        match self.ops.rename(held, &req.source) {
            Ok(()) => {
                let _ = self.write(id, &Line::Returned);
                if let Err(e) = self.discard(id, &req.dest, "new") {
                    self.e.log(
                        "note",
                        &req.dest,
                        0,
                        &format!("could not delete the copy: {e}"),
                    );
                }
                let _ = self.write(id, &Line::Failed { why: why.clone() });
                self.e.log("move-failed", &req.source, 0, &why);
                Err(why)
            }
            Err(e) => {
                // Cannot put it back: leave the new copy, and say exactly where things are.
                let more = format!(
                    "{why}. The original could not be put back ({}); it is in {}, and a full copy is at {}",
                    plain(&e),
                    held.display(),
                    req.dest.display()
                );
                let _ = self.write(id, &Line::Failed { why: more.clone() });
                self.e.log("move-failed", &req.source, 0, &more);
                Err(more)
            }
        }
    }

    /// One move as its journal says.
    #[must_use]
    pub fn info(&self, id: &str) -> Option<MoveInfo> {
        let text = fs::read(self.journal_path(id)).ok()?;
        read_seen(id, &parse(&String::from_utf8_lossy(&text))).info
    }

    /// Every move, newest first.
    #[must_use]
    pub fn moves(&self) -> Vec<MoveInfo> {
        let Ok(reader) = fs::read_dir(self.e.data_dir().join("moves")) else {
            return Vec::new();
        };
        let mut out: Vec<MoveInfo> = reader
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(".jsonl")?;
                self.info(id)
            })
            .collect();
        out.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        out
    }

    /// "It works": delete the original waiting in the holding folder.
    pub fn confirm(&self, id: &str) -> Result<MoveInfo, String> {
        let _lock = self.e.lock()?;
        let info = self.info(id).ok_or("there is no such move")?;
        if info.state != MoveState::Waiting {
            return Err("this move is not waiting for a check".into());
        }
        if info.link && !self.our_link(&info) {
            return Err(format!(
                "{} is no longer Rhizome's link to {}; nothing was deleted",
                info.source.display(),
                info.dest.display()
            ));
        }
        if !is_real_dir(&info.dest) {
            return Err(format!(
                "{} is not there; nothing was deleted",
                info.dest.display()
            ));
        }
        self.write(id, &Line::Confirm)?;
        self.finish_confirm(&info)
    }

    fn finish_confirm(&self, info: &MoveInfo) -> Result<MoveInfo, String> {
        let root = self
            .e
            .places()
            .holding_root(&info.source)
            .ok_or("its drive has no holding folder")?;
        let dir = root.join(format!("move-{}", info.id));
        self.delete_in_holding(&dir, &root)?;
        self.write(&info.id, &Line::Confirmed)?;
        self.e.log(
            "move-confirmed",
            &info.source,
            info.bytes,
            "old copy deleted",
        );
        self.info(&info.id)
            .ok_or_else(|| "the move's journal cannot be read".to_owned())
    }

    /// Put the app back where it was, from "waiting" (the original comes back from the
    /// holding folder) or "done" (the app is copied back and checked).
    pub fn move_back(
        &self,
        id: &str,
        mut progress: impl FnMut(&str, u64, u64),
    ) -> Result<MoveInfo, String> {
        let _lock = self.e.lock()?;
        let info = self.info(id).ok_or("there is no such move")?;
        match info.state {
            MoveState::Waiting => self.back_from_waiting(&info),
            MoveState::Done => self.back_from_done(&info, &mut progress),
            _ => Err("this move cannot be moved back now".into()),
        }
    }

    /// The old place must be Rhizome's link (apps) or empty (games), and the new place real.
    fn back_precheck(&self, info: &MoveInfo) -> Result<(), String> {
        if info.link {
            if !self.our_link(info) {
                return Err(format!(
                    "{} is no longer Rhizome's link to {}; nothing was changed",
                    info.source.display(),
                    info.dest.display()
                ));
            }
        } else if fs::symlink_metadata(&info.source).is_ok() {
            return Err(format!(
                "something is at {} now; nothing was changed",
                info.source.display()
            ));
        }
        if !is_real_dir(&info.dest) {
            return Err(format!(
                "{} is not there; nothing was changed",
                info.dest.display()
            ));
        }
        Ok(())
    }

    fn back_from_waiting(&self, info: &MoveInfo) -> Result<MoveInfo, String> {
        self.back_precheck(info)?;
        if !is_real_dir(&info.held) {
            return Err(format!(
                "the original is not in the holding folder ({}); nothing was changed",
                info.held.display()
            ));
        }
        let id = &info.id;
        self.write(
            id,
            &Line::Back {
                from: "waiting".into(),
            },
        )?;
        if info.link {
            self.write(id, &Line::Unlink)?;
            if let Err(e) = self.ops.remove_link(&info.source) {
                let why = format!("the link could not be removed: {}", plain(&e));
                let _ = self.write(id, &Line::BackFailed { why: why.clone() });
                return Err(why);
            }
            self.write(id, &Line::Unlinked)?;
        }
        self.write(id, &Line::Return)?;
        if let Err(e) = self.ops.rename(&info.held, &info.source) {
            let mut why = format!("the original could not be put back: {}", plain(&e));
            if info.link {
                if let Err(e2) = self.ops.make_link(&info.source, &info.dest) {
                    why.push_str(&format!(
                        "; and the link could not be made again ({}): the app is at {} and the original in {}",
                        plain(&e2),
                        info.dest.display(),
                        info.held.display()
                    ));
                }
            }
            let _ = self.write(id, &Line::BackFailed { why: why.clone() });
            return Err(why);
        }
        self.write(id, &Line::Returned)?;
        self.after_back(info)
    }

    fn back_from_done(
        &self,
        info: &MoveInfo,
        progress: &mut dyn FnMut(&str, u64, u64),
    ) -> Result<MoveInfo, String> {
        self.back_precheck(info)?;
        let id = &info.id;
        let parent = info
            .source
            .parent()
            .ok_or("the old place has no folder above it")?;
        if !is_real_dir(parent) {
            return Err(format!(
                "{} is not a real folder; nothing was changed",
                parent.display()
            ));
        }
        let size = check::look(&info.dest)?.bytes;
        let vol = self
            .ops
            .volume(&info.source)
            .map_err(|e| format!("its old drive cannot be read ({})", plain(&e)))?;
        let need = size.saturating_add(size / 100 * ROOM_EXTRA_PERCENT);
        if vol.free < need {
            return Err(format!(
                "its old drive has {} bytes free; {} are needed (the app plus 10%)",
                vol.free, need
            ));
        }
        for rel in check::files_inside(&info.dest) {
            if let Err(e) = self.ops.open_alone(&info.dest.join(&rel)) {
                return Err(if in_use(&e) {
                    format!(
                        "{} is in use ({}); close it and try again",
                        info.app,
                        rel.display()
                    )
                } else {
                    format!("{} cannot be read ({})", rel.display(), plain(&e))
                });
            }
        }
        let partial = parent.join(format!(".rhizome-moving-{id}-back"));
        self.write(
            id,
            &Line::Back {
                from: "done".into(),
            },
        )?;
        self.write(
            id,
            &Line::BackCopy {
                partial: partial.clone(),
            },
        )?;
        let stop = |why: String| -> Result<MoveInfo, String> {
            if let Err(e) = self.discard(id, &partial, "copy-back") {
                self.e.log(
                    "note",
                    &partial,
                    0,
                    &format!("could not delete the copy: {e}"),
                );
            }
            let _ = self.write(id, &Line::BackFailed { why: why.clone() });
            Err(why)
        };
        if let Err(why) = self.copy_tree(&info.dest, &partial, size, progress) {
            return stop(format!("the copy back failed: {why}"));
        }
        if let Err(why) = Self::same_tree(&info.dest, &partial, size, progress) {
            return stop(format!("the copy back is not the same: {why}"));
        }
        if let Err(e) = self.write(id, &Line::BackVerified) {
            return stop(e);
        }
        if info.link {
            self.write(id, &Line::Unlink)?;
            if let Err(e) = self.ops.remove_link(&info.source) {
                return stop(format!("the link could not be removed: {}", plain(&e)));
            }
            self.write(id, &Line::Unlinked)?;
        }
        self.write(id, &Line::Return)?;
        if let Err(e) = self.ops.rename(&partial, &info.source) {
            let mut why = format!("the copy could not be named: {}", plain(&e));
            if info.link {
                if let Err(e2) = self.ops.make_link(&info.source, &info.dest) {
                    why.push_str(&format!(
                        "; and the link could not be made again ({}): the app is at {}",
                        plain(&e2),
                        info.dest.display()
                    ));
                }
            }
            return stop(why);
        }
        self.write(id, &Line::Returned)?;
        self.after_back(info)
    }

    /// The app is back at the old place: delete the copy on the other drive.
    fn after_back(&self, info: &MoveInfo) -> Result<MoveInfo, String> {
        self.write(&info.id, &Line::MovedBack)?;
        if let Err(e) = self.discard(&info.id, &info.dest, "new") {
            self.e.log(
                "note",
                &info.dest,
                0,
                &format!("could not delete the copy: {e}"),
            );
        }
        // The original's empty folder in the holding folder.
        if let Some(root) = self.e.places().holding_root(&info.source) {
            let _ = self.delete_in_holding(&root.join(format!("move-{}", info.id)), &root);
        }
        self.e.log(
            "moved-back",
            &info.source,
            info.bytes,
            &format!("from {}", info.dest.display()),
        );
        self.info(&info.id)
            .ok_or_else(|| "the move's journal cannot be read".to_owned())
    }

    /// Finish a move Rhizome stopped in the middle of (closed, crashed, power cut). Forward only
    /// when the app already runs from the new place; otherwise everything goes back. Returns
    /// what was done, in words.
    pub fn recover(&self, id: &str) -> Result<String, String> {
        let _lock = self.e.lock()?;
        let text = fs::read(self.journal_path(id)).map_err(|_| "there is no such move")?;
        let seen = read_seen(id, &parse(&String::from_utf8_lossy(&text)));
        let info = seen.info.clone().ok_or("the move's journal has no start")?;
        let mut done = Vec::new();
        // Discards that did not finish: finish them (the thing is already marked to go).
        for (what, to) in &seen.open_discards {
            let root = self.e.places().holding_root(to);
            if fs::symlink_metadata(to).is_ok() {
                if let (Some(dir), Some(root)) = (to.parent(), root) {
                    self.delete_in_holding(dir, &root)?;
                }
            } else if fs::symlink_metadata(what).is_ok() {
                let label = to
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "copy".into());
                self.discard(id, what, &label)?;
            }
            let _ = self.write(id, &Line::Discarded { what: what.clone() });
            done.push(format!("finished deleting {}", what.display()));
        }
        let state = self.info(id).map(|i| i.state).unwrap_or(info.state.clone());
        match state {
            MoveState::Moving { .. } if seen.has("confirm") => {
                let _ = self.finish_confirm(&info)?;
                done.push("finished deleting the old copy".into());
            }
            MoveState::Moving { .. } => done.push(self.recover_forward(&seen, &info)?),
            MoveState::MovingBack { .. } => done.push(self.recover_back(&seen, &info)?),
            _ => {}
        }
        Ok(if done.is_empty() {
            "nothing needed finishing".into()
        } else {
            done.join("; ")
        })
    }

    fn recover_forward(&self, seen: &Seen, info: &MoveInfo) -> Result<String, String> {
        let id = &info.id;
        let src_there = fs::symlink_metadata(&info.source).is_ok_and(|m| !check::is_link(&m));
        let held_there = is_real_dir(&info.held);
        if seen.has("hold") && held_there && !src_there {
            // The original was moved aside.
            if !info.link {
                self.write(id, &Line::Held)?;
                return Ok("the move had finished; it waits for your check".into());
            }
            if self.our_link(info) {
                self.write(id, &Line::Linked)?;
                return Ok("the move had finished; it waits for your check".into());
            }
            if fs::symlink_metadata(&info.source).is_ok() {
                return Err(format!(
                    "something unknown is at {}; the original is in {} and the copy at {}",
                    info.source.display(),
                    info.held.display(),
                    info.dest.display()
                ));
            }
            self.write(id, &Line::Return)?;
            self.ops
                .rename(&info.held, &info.source)
                .map_err(|e| format!("the original could not be put back: {}", plain(&e)))?;
            self.write(id, &Line::Returned)?;
        }
        // The original must be in its place before any copy is deleted.
        if !is_real_dir(&info.source) {
            return Err(format!(
                "the app is not at {} and not in the holding folder; nothing was deleted. The copy is at {}",
                info.source.display(),
                info.dest.display()
            ));
        }
        // Delete whatever was copied.
        let _ = self.discard(id, &seen.partial, "copy");
        if seen.has("name") {
            let _ = self.discard(id, &info.dest, "new");
        }
        let why = "Rhizome stopped during the move; the app was left where it was".to_owned();
        self.write(id, &Line::Failed { why: why.clone() })?;
        Ok(why)
    }

    fn recover_back(&self, seen: &Seen, info: &MoveInfo) -> Result<String, String> {
        let id = &info.id;
        let from_done = seen.back_from.as_deref() == Some("done");
        if from_done && !seen.since_back("back_verified") {
            if let Some(p) = &seen.back_partial {
                let _ = self.discard(id, p, "copy-back");
            }
            let why = "Rhizome stopped while copying back; the app still runs from the new place"
                .to_owned();
            self.write(id, &Line::BackFailed { why: why.clone() })?;
            return Ok(why);
        }
        // Go on with moving back (the person asked for it).
        if info.link && self.our_link(info) {
            self.ops
                .remove_link(&info.source)
                .map_err(|e| format!("the link could not be removed: {}", plain(&e)))?;
            self.write(id, &Line::Unlinked)?;
        }
        if fs::symlink_metadata(&info.source).is_err() {
            let from = if from_done {
                seen.back_partial
                    .clone()
                    .ok_or("the copy back is not recorded")?
            } else {
                info.held.clone()
            };
            if !is_real_dir(&from) {
                return Err(format!(
                    "neither the app nor its copy is at the old place, and {} is missing; the app is at {}",
                    from.display(),
                    info.dest.display()
                ));
            }
            self.ops
                .rename(&from, &info.source)
                .map_err(|e| format!("it could not be put back: {}", plain(&e)))?;
            self.write(id, &Line::Returned)?;
        }
        if !is_real_dir(&info.source) {
            return Err(format!("{} is not a real folder", info.source.display()));
        }
        self.after_back(info)?;
        Ok("finished moving it back".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_junction_buffer_reads_back_to_its_target() {
        let buf = mount_point_buffer(r"D:\Apps\Some App").unwrap();
        assert_eq!(
            parse_mount_point(&buf).as_deref(),
            Some(r"D:\Apps\Some App")
        );
        // Tag, then the data length matches what follows the 8-byte header.
        assert_eq!(&buf[..4], &0xA000_0003u32.to_le_bytes());
        let len = u16::from_le_bytes([buf[4], buf[5]]) as usize;
        assert_eq!(buf.len(), 8 + len);
    }

    #[test]
    fn other_links_and_damaged_data_are_not_read_as_ours() {
        let mut buf = mount_point_buffer(r"D:\x").unwrap();
        buf[0] = 0x0C; // IO_REPARSE_TAG_SYMLINK's low byte
        assert_eq!(parse_mount_point(&buf), None);
        assert_eq!(parse_mount_point(&[1, 2, 3]), None);
        let mut short = mount_point_buffer(r"D:\x").unwrap();
        short.truncate(20);
        assert_eq!(parse_mount_point(&short), None);
        // A volume GUID target is not a drive path.
        let guid = mount_point_buffer(r"Volume{00000000-0000-0000-0000-000000000000}\").unwrap();
        assert_eq!(parse_mount_point(&guid), None);
    }

    #[test]
    fn journal_lines_survive_damage_and_give_the_state() {
        let start = Line::Start {
            time: 5,
            app: "App".into(),
            source: PathBuf::from("/a/App"),
            dest: PathBuf::from("/b/App"),
            link: true,
            bytes: 10,
            files: 2,
            partial: PathBuf::from("/b/.p"),
            held: PathBuf::from("/h/App"),
        };
        let all = [
            start,
            Line::Copied,
            Line::Verified,
            Line::Name,
            Line::Named,
            Line::Hold,
            Line::Held,
            Line::Link,
            Line::Linked,
        ];
        let joined: String = all.iter().map(text).collect::<Vec<_>>().join("\n");
        let seen = read_seen("x", &parse(&joined));
        assert_eq!(seen.info.unwrap().state, MoveState::Waiting);
        // Stopped after "hold": still moving (recovery decides from the disk).
        let cut: String = all[..6].iter().map(text).collect::<Vec<_>>().join("");
        let seen = read_seen("x", &parse(&cut));
        assert!(matches!(seen.info.unwrap().state, MoveState::Moving { .. }));
    }
}

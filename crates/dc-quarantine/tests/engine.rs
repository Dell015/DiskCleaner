//! The removal engine on generated folders in a temporary directory. Never on real data.
#![cfg(feature = "destructive-actions")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]
// The tests set up attacks by hand (swapping a folder for a link).
#![allow(clippy::disallowed_methods)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dc_quarantine::{Engine, Group, Guard, Item, Ops, Places, RealOps, State};

/// A link to a folder. On Windows this needs Developer Mode or administrator rights; without
/// them the link parts of a test are skipped (the rest still runs).
fn link_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
}

struct Here(PathBuf);
impl Places for Here {
    fn holding_root(&self, _item: &Path) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// Refuses anything with "protected" in its path; "Program Files" is not your own.
struct NameGuard;
impl Guard for NameGuard {
    fn refuse(&self, path: &Path) -> Option<String> {
        path.to_string_lossy()
            .contains("protected")
            .then(|| "it is protected".to_owned())
    }
    fn refuse_own(&self, path: &Path) -> Option<String> {
        path.to_string_lossy()
            .contains("Program Files")
            .then(|| "it is not one of your own files".to_owned())
    }
}

#[derive(Default)]
struct Faults {
    renames: Mutex<usize>,
    appends: Mutex<usize>,
    /// Fail this rename (1-based) with this error.
    fail_rename: Option<(usize, io::ErrorKind)>,
    /// Fail renames of paths ending with this name.
    fail_name: Option<&'static str>,
    /// Panic right after this rename succeeded (a crash).
    crash_after_rename: Option<usize>,
    /// Fail every append after this many.
    appends_ok: Option<usize>,
    /// Write this append (1-based) torn: only its first half, no line break (a crash or a full
    /// disk in the middle of a write).
    torn_append: Option<usize>,
}

impl Ops for Faults {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let k = {
            let mut c = self.renames.lock().unwrap();
            *c += 1;
            *c
        };
        if let Some((at, kind)) = self.fail_rename {
            if at == k {
                return Err(io::Error::new(kind, "injected"));
            }
        }
        if let Some(name) = self.fail_name {
            if from.to_string_lossy().ends_with(name) {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "in use"));
            }
        }
        RealOps.rename(from, to)?;
        if self.crash_after_rename == Some(k) {
            panic!("simulated crash");
        }
        Ok(())
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        RealOps.create_dir_all(path)
    }
    fn append_line(&self, path: &Path, line: &str) -> io::Result<()> {
        let k = {
            let mut c = self.appends.lock().unwrap();
            *c += 1;
            *c
        };
        if self.appends_ok.is_some_and(|ok| k > ok) {
            return Err(io::Error::other("disk full"));
        }
        if self.torn_append == Some(k) {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            f.write_all(&line.as_bytes()[..line.len() / 2])?;
            return Err(io::Error::other("disk full half-way"));
        }
        RealOps.append_line(path, line)
    }
    fn remove_tree(&self, path: &Path) -> io::Result<()> {
        RealOps.remove_tree(path)
    }
    fn protect_dir(&self, path: &Path) -> io::Result<()> {
        RealOps.protect_dir(path)
    }
}

struct World {
    _dir: tempfile::TempDir,
    disk: PathBuf,
    holding: PathBuf,
    data: PathBuf,
}

fn world() -> World {
    let dir = tempfile::Builder::new()
        .prefix("dc-quarantine")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let disk = base.join("disk");
    let w = |rel: &str, text: &str| {
        let p = disk.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    w("Downloads/setup.exe", "installer bytes");
    w("Downloads/old.zip", "zip");
    w("Cache/a.bin", "aaaa");
    w("Cache/sub/b.bin", "bbbbbb");
    w("Cache/sub/deeper/c.bin", "c");
    w("protected/keep.txt", "keep");
    w("Docs/report.docx", "my work");
    World {
        holding: base.join("holding"),
        data: base.join("data"),
        disk,
        _dir: dir,
    }
}

fn item(path: PathBuf, group: Group) -> Item {
    Item {
        path,
        group,
        title: "test".into(),
        reason: "test".into(),
        newest_at_scan: 0,
    }
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((
                    p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&p).unwrap(),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn removing_moves_into_holding_and_restoring_puts_everything_back() {
    let w = world();
    let before = snapshot(&w.disk);
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let mut steps = 0;
    let out = e.remove(
        &[
            item(w.disk.join("Cache"), Group::Safe),
            item(
                w.disk.join("Downloads/setup.exe"),
                Group::YourCall { understood: true },
            ),
        ],
        |_, _| steps += 1,
    );
    assert!(out.skipped.is_empty(), "{:?}", out.skipped);
    assert_eq!(out.held.len(), 2);
    assert!(steps >= 3);
    assert!(!w.disk.join("Cache").exists() && !w.disk.join("Downloads/setup.exe").exists());
    let cache = &out.held[0];
    assert_eq!(
        (cache.record.files, cache.record.bytes, cache.record.is_dir),
        (3, 11, true)
    );
    assert!(cache.record.held.starts_with(&w.holding));
    let list = e.cleanups();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].held_bytes(), 11 + 15);

    let back = e.restore(&out.id, None);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert_eq!(back.back.len(), 2);
    assert_eq!(
        snapshot(&w.disk),
        before,
        "everything is back, byte for byte"
    );
    assert!(e.cleanups()[0]
        .items
        .iter()
        .all(|i| matches!(i.state, State::Restored { .. })));
    let log = std::fs::read_to_string(w.data.join("cleaning-log.jsonl")).unwrap();
    assert_eq!(log.lines().filter(|l| l.contains("\"removed\"")).count(), 2);
    assert_eq!(
        log.lines().filter(|l| l.contains("\"restored\"")).count(),
        2
    );
}

#[test]
fn consent_protection_and_odd_items_are_skipped_and_left_alone() {
    let w = world();
    let before = snapshot(&w.disk);
    let links = link_dir(&w.disk.join("Docs"), &w.disk.join("link-to-docs"));
    std::fs::create_dir_all(w.disk.join("HasLink")).unwrap();
    let links = links && link_dir(&w.disk.join("Docs"), &w.disk.join("HasLink/inner"));
    if !links {
        eprintln!(
            "links cannot be made here (Windows without Developer Mode): link checks skipped"
        );
        let _ = std::fs::remove_dir(w.disk.join("HasLink"));
    }
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(
        &[
            item(
                w.disk.join("Downloads/old.zip"),
                Group::ProbablySafe { checked: false },
            ),
            item(w.disk.join("Docs"), Group::YourCall { understood: false }),
            item(w.disk.join("protected"), Group::Safe),
            item(w.disk.join("link-to-docs"), Group::Safe),
            item(w.disk.join("HasLink"), Group::Safe),
            item(w.disk.join("link-to-docs/report.docx"), Group::Safe),
            item(w.disk.join("missing"), Group::Safe),
            item(PathBuf::from("/"), Group::Safe),
            item(PathBuf::from("relative/path"), Group::Safe),
            item(w.data.clone(), Group::Safe),
            item(w.holding.parent().unwrap().to_path_buf(), Group::Safe),
        ],
        |_, _| {},
    );
    assert!(out.held.is_empty(), "{:?}", out.held);
    assert_eq!(out.skipped.len(), 11);
    assert!(
        out.skipped[9].why.contains("DiskCleaner's own")
            && out.skipped[10].why.contains("DiskCleaner's own")
    );
    let why = |k: usize| out.skipped[k].why.clone();
    assert!(why(0).contains("not ticked"));
    assert!(why(1).contains("not confirmed"));
    assert!(why(2).contains("protected"));
    if links {
        assert!(why(3).contains("link"));
        assert!(why(4).contains("contains a link"));
        assert!(why(5).contains("link"));
    }
    assert!(why(6).contains("not there"));
    assert!(why(7).contains("full path") || why(7).contains("DiskCleaner's own"));
    assert!(why(8).contains("full path"));
    assert!(out.id.is_empty());
    let mut after = snapshot(&w.disk);
    after.retain(|(p, _)| !p.starts_with("link-to-docs") && !p.starts_with("HasLink"));
    assert_eq!(after, before);
}

#[test]
fn your_own_files_that_changed_after_the_scan_are_skipped() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let mut it = item(w.disk.join("Docs"), Group::YourCall { understood: true });
    it.newest_at_scan = 1_000_000_000; // long before the files were written
    let mut cache = item(w.disk.join("Cache"), Group::Safe);
    cache.newest_at_scan = 1_000_000_000; // suggestions may change (caches do); they still move
    let out = e.remove(&[it, cache], |_, _| {});
    assert_eq!(out.skipped.len(), 1);
    assert!(out.skipped[0].why.contains("changed after the scan"));
    assert_eq!(out.held.len(), 1);
    assert!(w.disk.join("Docs/report.docx").exists());
}

#[test]
fn a_taken_place_restores_as_a_copy_named_restored() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(
        &[
            item(w.disk.join("Downloads/old.zip"), Group::Safe),
            item(w.disk.join("Cache"), Group::Safe),
        ],
        |_, _| {},
    );
    std::fs::write(
        w.disk.join("Downloads/old.zip"),
        "a new file with the same name",
    )
    .unwrap();
    std::fs::create_dir_all(w.disk.join("Cache")).unwrap();
    let back = e.restore(&out.id, None);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert_eq!(
        std::fs::read_to_string(w.disk.join("Downloads/old.zip")).unwrap(),
        "a new file with the same name"
    );
    assert_eq!(
        std::fs::read_to_string(w.disk.join("Downloads/old (restored).zip")).unwrap(),
        "zip"
    );
    assert!(w.disk.join("Cache (restored)/sub/deeper/c.bin").exists());
    // Restoring one item at a time works too (nothing left to do now).
    assert!(e.restore(&out.id, None).back.is_empty());
}

#[test]
fn a_missing_parent_folder_is_made_again_on_restore() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache/sub"), Group::Safe)], |_, _| {});
    let gone = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    assert_eq!(gone.held.len(), 1);
    let back = e.restore(&out.id, None);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert!(w.disk.join("Cache/sub/deeper/c.bin").exists());
}

#[test]
fn emptying_deletes_only_inside_the_holding_folder() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let others = snapshot(&w.disk);
    assert_eq!(e.empty(&out.id).unwrap(), 11);
    assert!(!w.holding.join(&out.id).join("items").exists());
    assert!(
        w.holding.join(&out.id).join("journal.jsonl").exists(),
        "the record stays"
    );
    assert_eq!(
        snapshot(&w.disk),
        others,
        "nothing outside the holding folder changed"
    );
    assert!(e.cleanups()[0]
        .items
        .iter()
        .all(|i| i.state == State::Emptied));
    assert_eq!(e.restore(&out.id, None).back.len(), 0);
    for bad in ["", "..", "../disk", "a/b", "nope"] {
        assert!(e.empty(bad).is_err(), "{bad}");
    }
}

#[test]
fn emptying_refuses_when_the_items_folder_was_swapped_for_a_link() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let items = w.holding.join(&out.id).join("items");
    let moved_away = w.holding.join("elsewhere");
    std::fs::rename(&items, &moved_away).unwrap();
    if !link_dir(&w.disk.join("Docs"), &items) {
        eprintln!("links cannot be made here: test skipped");
        return;
    }
    assert!(e.empty(&out.id).unwrap_err().contains("link"));
    assert!(
        w.disk.join("Docs/report.docx").exists(),
        "the link's target is untouched"
    );
}

#[test]
fn old_cleanups_are_emptied_after_the_retention_time() {
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let created = e.cleanups()[0].created;
    assert!(e.empty_expired(30, created + 29 * 86_400).is_empty());
    assert_eq!(
        e.empty_expired(30, created + 30 * 86_400),
        vec![(out.id.clone(), 11)]
    );
    assert!(
        e.empty_expired(30, created + 99 * 86_400).is_empty(),
        "only once"
    );
}

#[test]
fn a_crash_right_after_a_move_is_read_back_from_the_disk() {
    let w = world();
    let places = Here(w.holding.clone());
    let ops = Faults {
        crash_after_rename: Some(2),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let items = [
        item(w.disk.join("Downloads/setup.exe"), Group::Safe),
        item(w.disk.join("Downloads/old.zip"), Group::Safe),
        // Same depth, so the order stays as given (outer items go first).
        item(w.disk.join("Cache/a.bin"), Group::Safe),
    ];
    let crashed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.remove(&items, |_, _| {})));
    assert!(crashed.is_err());
    // A fresh start reads the journal: the second item had moved (its "moved" line never got
    // written), the third was never touched.
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let list = e.cleanups();
    assert_eq!(list.len(), 1);
    let held: Vec<String> = list[0]
        .items
        .iter()
        .map(|i| {
            i.record
                .original
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(held, ["setup.exe", "old.zip"]);
    assert!(w.disk.join("Cache/a.bin").exists());
    let back = e.restore(&list[0].id, None);
    assert_eq!(back.back.len(), 2);
    assert!(
        w.disk.join("Downloads/old.zip").exists() && w.disk.join("Downloads/setup.exe").exists()
    );
}

#[test]
fn a_full_disk_stops_before_anything_moves() {
    let w = world();
    let before = snapshot(&w.disk);
    let places = Here(w.holding.clone());
    // The start line and the holding-folder list are written; the item's own line is not.
    let ops = Faults {
        appends_ok: Some(2),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    assert!(out.held.is_empty());
    assert!(out.skipped[0].why.contains("journal could not be written"));
    assert_eq!(snapshot(&w.disk), before);
}

#[test]
fn folders_with_files_in_use_move_file_by_file_and_restore() {
    let w = world();
    let places = Here(w.holding.clone());
    // The folder itself cannot move (something inside is open), and one file stays locked.
    let ops = Faults {
        fail_rename: Some((1, io::ErrorKind::PermissionDenied)),
        fail_name: Some("b.bin"),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    assert_eq!(out.held.len(), 1);
    let held = &out.held[0];
    assert_eq!(held.only_files.as_ref().unwrap().len(), 2);
    assert_eq!(held.record.files, 2);
    assert!(out.skipped[0].why.contains("1 files inside were in use"));
    assert!(
        w.disk.join("Cache/sub/b.bin").exists(),
        "the locked file stays"
    );
    assert!(!w.disk.join("Cache/a.bin").exists());
    let e2 = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    assert_eq!(
        e2.cleanups()[0].items[0].only_files.as_ref().unwrap().len(),
        2
    );
    let back = e2.restore(&out.id, None);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert!(w.disk.join("Cache/a.bin").exists() && w.disk.join("Cache/sub/deeper/c.bin").exists());
}

#[test]
fn a_move_that_fails_leaves_the_item_where_it_was() {
    let w = world();
    let places = Here(w.holding.clone());
    let ops = Faults {
        fail_rename: Some((1, io::ErrorKind::CrossesDevices)),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(
        &[item(w.disk.join("Downloads/old.zip"), Group::Safe)],
        |_, _| {},
    );
    assert!(out.held.is_empty());
    assert!(w.disk.join("Downloads/old.zip").exists());
    assert!(e.cleanups().is_empty());
}

#[test]
fn only_the_ops_file_may_call_rename_or_remove() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // `unsafe` lives only in win.rs, and only ops.rs calls into it (D119).
        if name != "win.rs" {
            assert!(!text.contains("unsafe {"), "{name} contains unsafe");
            for call in [
                "MoveFileExW",
                "SetFileInformationByHandle",
                "SetFileSecurityW",
            ] {
                assert!(!text.contains(call), "{name} contains {call}");
            }
        }
        if name != "ops.rs" {
            assert!(!text.contains("win::"), "{name} calls win.rs");
        }
        if name == "ops.rs" {
            continue;
        }
        for banned in [
            "fs::rename",
            "remove_file",
            "remove_dir",
            "Command::",
            "set_permissions",
            "fs::write",
            "File::create",
        ] {
            assert!(
                !text.contains(banned),
                "{} contains {banned}",
                path.display()
            );
        }
    }
}

// ---- Findings of the B9 safety review (2026-11-14) ----

fn names(e: &Engine) -> Vec<String> {
    e.cleanups()
        .iter()
        .flat_map(|c| c.items.iter())
        .filter(|i| i.state == State::Held)
        .map(|i| {
            i.record
                .original
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn a_torn_journal_line_loses_no_other_record() {
    // Finding 1: the "moved" line of the first item is torn (half written, no line break).
    // Before the fix the next item's record was swallowed with it and its files were invisible.
    let w = world();
    let places = Here(w.holding.clone());
    // Appends: 1 start, 2 holding list, 3 hold #1, 4 held #1 (torn), 5 log, 6 hold #2, ...
    let ops = Faults {
        torn_append: Some(4),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(
        &[
            item(w.disk.join("Downloads/setup.exe"), Group::Safe),
            item(w.disk.join("Downloads/old.zip"), Group::Safe),
        ],
        |_, _| {},
    );
    assert_eq!(out.held.len(), 2);
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    assert_eq!(names(&e), ["setup.exe", "old.zip"], "both are listed");
    assert_eq!(e.restore(&out.id, None).back.len(), 2, "and both come back");
    assert!(w.disk.join("Downloads/old.zip").exists());
}

#[test]
fn a_cleanup_with_a_lost_start_time_is_never_emptied_automatically() {
    // Findings 1 and 2: the start record is torn, so the cleanup's age is unknown.
    let w = world();
    let places = Here(w.holding.clone());
    let ops = Faults {
        torn_append: Some(1),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    // The start could not be written, so nothing was moved at all.
    assert!(out.held.is_empty());
    assert!(w.disk.join("Cache/a.bin").exists());

    // A journal damaged later (by hand here) still never expires.
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let journal = w.holding.join(&out.id).join("journal.jsonl");
    let text = std::fs::read_to_string(&journal).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    std::fs::write(&journal, format!("{}{rest}", &first[..first.len() / 2])).unwrap();
    let list = e.cleanups();
    assert_eq!(list[0].created, 0, "unknown");
    assert_eq!(list[0].items.len(), 1, "the item is still listed");
    let far_future = 4_000_000_000;
    assert!(e.expired(30, far_future).is_empty());
    assert!(e.empty_expired(30, far_future).is_empty());
    assert!(w.holding.join(&out.id).join("items/1/Cache/a.bin").exists());
}

#[test]
fn a_clock_that_went_back_or_forward_by_mistake_is_not_trusted_blindly() {
    // Finding 2: a cleanup "made in the future" (the clock was wrong then, or is wrong now).
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let created = e.cleanups()[0].created;
    assert!(e.empty_expired(30, created - 1).is_empty());
    assert!(e.empty_expired(0, created - 86_400).is_empty());
    assert_eq!(e.expired(30, created + 30 * 86_400).len(), 1);
    assert!(w.holding.join(&out.id).join("items").exists());
}

#[test]
fn emptying_refuses_anything_the_record_does_not_describe() {
    // Finding 5: a stray folder in the items folder is never deleted unseen.
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let stray = w.holding.join(&out.id).join("items/99/stray.txt");
    std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
    std::fs::write(&stray, "not in the record").unwrap();
    let err = e.empty(&out.id).unwrap_err();
    assert!(err.contains("not in DiskCleaner's record"), "{err}");
    assert!(stray.exists());
    assert!(w.holding.join(&out.id).join("items/1/Cache/a.bin").exists());
    std::fs::remove_dir_all(stray.parent().unwrap()).unwrap();
    assert_eq!(e.empty(&out.id).unwrap(), 11);
}

#[test]
fn a_file_by_file_restore_that_stops_part_way_can_be_finished() {
    // Finding 4: crash after the first file went back.
    let w = world();
    let places = Here(w.holding.clone());
    let ops = Faults {
        fail_rename: Some((1, io::ErrorKind::PermissionDenied)),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &ops);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    assert_eq!(out.held[0].only_files.as_ref().unwrap().len(), 3);
    let crash = Faults {
        crash_after_rename: Some(1),
        ..Faults::default()
    };
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &crash);
    let crashed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e.restore(&out.id, None)));
    assert!(crashed.is_err());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let item = e.cleanups()[0].items[0].clone();
    assert_eq!(item.state, State::Held);
    assert_eq!(
        item.only_files.as_ref().unwrap().len(),
        2,
        "one file is back"
    );
    assert_eq!(item.record.files, 2);
    let back = e.restore(&out.id, None);
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert!(matches!(
        e.cleanups()[0].items[0].state,
        State::Restored { .. }
    ));
    for f in ["Cache/a.bin", "Cache/sub/b.bin", "Cache/sub/deeper/c.bin"] {
        assert!(w.disk.join(f).exists(), "{f}");
    }
}

#[test]
fn program_folders_are_never_your_own_files_but_findings_there_still_move() {
    // Finding 3, the engine's own last check.
    let w = world();
    let tool = w.disk.join("Program Files/SomeTool");
    std::fs::create_dir_all(&tool).unwrap();
    std::fs::write(tool.join("tool.exe"), "exe").unwrap();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(
        &[item(tool.clone(), Group::YourCall { understood: true })],
        |_, _| {},
    );
    assert!(out.skipped[0].why.contains("not one of your own"));
    assert!(tool.join("tool.exe").exists());
    let out = e.remove(&[item(tool.clone(), Group::Safe)], |_, _| {});
    assert_eq!(out.held.len(), 1, "a finding there is decided by its rule");
}

#[test]
fn leftovers_that_changed_after_the_scan_are_skipped() {
    // Finding 8: leftovers of an app that is in use again.
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let mut left = item(w.disk.join("Cache"), Group::Safe);
    left.reason = "leftover_missing_app".into();
    left.newest_at_scan = 1_000_000_000;
    let out = e.remove(&[left], |_, _| {});
    assert!(out.held.is_empty());
    assert!(out.skipped[0].why.contains("changed after the scan"));
}

#[test]
fn an_item_inside_another_moves_with_it_whatever_the_order() {
    // Finding 9: inner picked before outer.
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(
        &[
            item(w.disk.join("Cache/sub"), Group::Safe),
            item(w.disk.join("Cache"), Group::Safe),
            item(w.disk.join("Cache"), Group::Safe),
        ],
        |_, _| {},
    );
    assert_eq!(out.held.len(), 1);
    assert_eq!(out.held[0].record.files, 3);
    assert!(out.skipped.is_empty());
    let back = e.restore(&out.id, None);
    assert_eq!(back.back.len(), 1);
    assert!(w.disk.join("Cache/sub/deeper/c.bin").exists());
    assert!(!w.disk.join("Cache (restored)").exists());
}

#[test]
fn only_one_removal_restore_or_emptying_runs_at_a_time() {
    // Finding 11.
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(&[item(w.disk.join("Cache"), Group::Safe)], |_, _| {});
    let other = std::fs::OpenOptions::new()
        .write(true)
        .open(w.data.join("cleaning.lock"))
        .unwrap();
    other.lock().unwrap();
    let busy = e.remove(
        &[item(w.disk.join("Downloads/old.zip"), Group::Safe)],
        |_, _| {},
    );
    assert!(busy.skipped[0].why.contains("another removal"));
    assert!(e.restore(&out.id, None).problems[0]
        .why
        .contains("another removal"));
    assert!(e.empty(&out.id).unwrap_err().contains("another removal"));
    assert!(w.disk.join("Downloads/old.zip").exists());
    drop(other);
    assert_eq!(e.restore(&out.id, None).back.len(), 1);
    // Ids are unique per process too.
    assert!(out.id.ends_with(&format!("-{}", std::process::id())));
}

#[test]
fn moving_never_replaces_anything() {
    // Finding 9: the move itself refuses a taken name (not only the free-name check before it).
    let w = world();
    let a = w.disk.join("Downloads/old.zip");
    let b = w.disk.join("Downloads/setup.exe");
    let err = RealOps.rename(&a, &b).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "installer bytes");
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "zip");
}

#[test]
fn restoring_never_makes_folders_inside_a_link() {
    // Finding 9: the missing parent is under a folder that became a link.
    let w = world();
    let places = Here(w.holding.clone());
    let e = Engine::new(w.data.clone(), &places, &NameGuard, &RealOps);
    let out = e.remove(
        &[item(w.disk.join("Cache/sub/deeper"), Group::Safe)],
        |_, _| {},
    );
    std::fs::rename(w.disk.join("Cache"), w.disk.join("Cache-moved")).unwrap();
    let target = w.disk.join("Elsewhere");
    std::fs::create_dir_all(&target).unwrap();
    if !link_dir(&target, &w.disk.join("Cache")) {
        eprintln!("links cannot be made here: test skipped");
        return;
    }
    let back = e.restore(&out.id, None);
    assert!(back.back.is_empty());
    assert!(back.problems[0].why.contains("link"));
    assert!(
        std::fs::read_dir(&target).unwrap().next().is_none(),
        "nothing was made inside the link's target"
    );
}

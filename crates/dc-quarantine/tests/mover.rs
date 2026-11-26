//! Moving an app to another drive (B11): every step, every refusal, injected failures and
//! crashes, on temporary folders only. "Drives" are two folders `c` and `d` in a temp folder.
#![cfg(feature = "destructive-actions")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::type_complexity
)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dc_quarantine::{Guard, MoveOps, MoveRequest, MoveState, Mover, Ops, Places, RealOps, Volume};

/// Each "drive" keeps its holding folder at `<drive>/.hold`.
struct Drives(PathBuf);
impl Drives {
    fn drive_of(&self, p: &Path) -> Option<PathBuf> {
        let rest = p.strip_prefix(&self.0).ok()?;
        let first = rest.components().next()?;
        Some(self.0.join(first.as_os_str()))
    }
}
impl Places for Drives {
    fn holding_root(&self, item: &Path) -> Option<PathBuf> {
        self.drive_of(item).map(|d| d.join(".hold"))
    }
}

struct NoGuard;
impl Guard for NoGuard {
    fn refuse(&self, path: &Path) -> Option<String> {
        path.to_string_lossy()
            .contains("Windows")
            .then(|| "it is protected".to_owned())
    }
    fn refuse_own(&self, _path: &Path) -> Option<String> {
        None
    }
}

/// Real disk operations with injected trouble.
#[derive(Default)]
struct T {
    root: PathBuf,
    copies: Mutex<usize>,
    renames: Mutex<usize>,
    /// Fail the n-th file copy.
    fail_copy: Option<usize>,
    /// Corrupt the n-th file copy (write different bytes).
    corrupt_copy: Option<usize>,
    /// Fail the n-th rename.
    fail_rename: Option<usize>,
    /// Panic right after the n-th rename (a crash).
    crash_after_rename: Option<usize>,
    /// Panic right after making the link (a crash before the journal says so).
    crash_after_link: bool,
    fail_link: bool,
    /// Panic during the n-th file copy.
    crash_on_copy: Option<usize>,
    /// Panic right after removing the link (moving back).
    crash_after_unlink: bool,
    /// Files with this name are "in use".
    busy: Option<&'static str>,
    filesystem: Option<&'static str>,
    free: Option<u64>,
    removable: bool,
}

fn count(m: &Mutex<usize>) -> usize {
    let mut c = m.lock().unwrap();
    *c += 1;
    *c
}

impl Ops for T {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let k = count(&self.renames);
        if self.fail_rename == Some(k) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected"));
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
        RealOps.append_line(path, line)
    }
    fn remove_tree(&self, path: &Path) -> io::Result<()> {
        RealOps.remove_tree(path)
    }
    fn protect_dir(&self, path: &Path) -> io::Result<()> {
        RealOps.protect_dir(path)
    }
}

impl MoveOps for T {
    fn copy_file(&self, from: &Path, to: &Path) -> io::Result<()> {
        let k = count(&self.copies);
        if self.crash_on_copy == Some(k) {
            panic!("simulated crash");
        }
        if self.fail_copy == Some(k) {
            return Err(io::Error::other("disk full"));
        }
        RealOps.copy_file(from, to)?;
        if self.corrupt_copy == Some(k) {
            fs::write(to, b"different bytes, same story").unwrap();
        }
        Ok(())
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        RealOps.create_dir(path)
    }
    fn make_link(&self, link: &Path, target: &Path) -> io::Result<()> {
        if self.fail_link {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected"));
        }
        RealOps.make_link(link, target)?;
        if self.crash_after_link {
            panic!("simulated crash");
        }
        Ok(())
    }
    fn read_link(&self, link: &Path) -> io::Result<Option<PathBuf>> {
        RealOps.read_link(link)
    }
    fn remove_link(&self, link: &Path) -> io::Result<()> {
        RealOps.remove_link(link)?;
        if self.crash_after_unlink {
            panic!("simulated crash");
        }
        Ok(())
    }
    fn open_alone(&self, file: &Path) -> io::Result<()> {
        if self
            .busy
            .is_some_and(|b| file.file_name().is_some_and(|n| n == b))
        {
            return Err(io::Error::new(io::ErrorKind::ResourceBusy, "in use"));
        }
        RealOps.open_alone(file)
    }
    fn volume(&self, _path: &Path) -> io::Result<Volume> {
        Ok(Volume {
            filesystem: self.filesystem.unwrap_or("NTFS").to_owned(),
            free: self.free.unwrap_or(1 << 40),
            total: 1 << 41,
            removable: self.removable,
            network: false,
        })
    }
    fn drive(&self, path: &Path) -> Option<String> {
        Drives(self.root.clone())
            .drive_of(path)
            .map(|d| d.to_string_lossy().into_owned())
    }
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    app: PathBuf,
    dest: PathBuf,
    data: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let app = root.join("c").join("Programs").join("Tool");
    fs::create_dir_all(app.join("bin")).unwrap();
    fs::write(app.join("tool.exe"), vec![7u8; 300_000]).unwrap();
    fs::write(app.join("bin").join("lib.dll"), b"library").unwrap();
    fs::write(app.join("readme.txt"), b"hello").unwrap();
    fs::create_dir_all(app.join("empty")).unwrap();
    fs::create_dir_all(root.join("d")).unwrap();
    let dest = root.join("d").join("Apps").join("Tool");
    let data = root.join("c").join("data");
    World {
        _tmp: tmp,
        root,
        app,
        dest,
        data,
    }
}

fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap().flatten() {
            let rel = e
                .path()
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if e.file_type().unwrap().is_symlink() {
                out.push((rel + " (link)", Vec::new()));
            } else if e.file_type().unwrap().is_dir() {
                out.push((rel + "/", Vec::new()));
                stack.push(e.path());
            } else {
                out.push((rel, fs::read(e.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn req(w: &World, link: bool) -> MoveRequest {
    MoveRequest {
        app: "Tool".into(),
        source: w.app.clone(),
        dest: w.dest.clone(),
        link,
    }
}

fn mover<'a>(w: &World, places: &'a Drives, ops: &'a T) -> Mover<'a> {
    Mover::new(w.data.clone(), places, &NoGuard, ops)
}

fn ops(w: &World) -> T {
    T {
        root: w.root.clone(),
        ..T::default()
    }
}

fn is_link(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

#[test]
fn an_app_moves_keeps_its_path_and_the_old_copy_goes_only_after_it_works() {
    let w = world();
    let before = snapshot(&w.app);
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let mut steps = Vec::new();
    let info = m
        .start(&req(&w, true), |s, _, _| steps.push(s.to_owned()))
        .unwrap();
    assert_eq!(info.state, MoveState::Waiting);
    assert!(steps.contains(&"copy".to_owned()) && steps.contains(&"check".to_owned()));
    // The new place has every byte; the old path is a link to it.
    assert_eq!(snapshot(&w.dest), before);
    assert!(is_link(&w.app));
    assert_eq!(
        snapshot(&w.app),
        before,
        "the app is reachable from its old path"
    );
    // The original waits in the old drive's holding folder.
    assert_eq!(snapshot(&info.held), before);
    assert!(info.held.starts_with(w.root.join("c").join(".hold")));
    // No temporary copy is left.
    assert!(fs::read_dir(w.dest.parent().unwrap()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".rhizome")));
    // It works: the original is deleted, and only it.
    let done = m.confirm(&info.id).unwrap();
    assert_eq!(done.state, MoveState::Done);
    assert!(!info.held.exists());
    assert!(!w
        .root
        .join("c")
        .join(".hold")
        .join(format!("move-{}", info.id))
        .exists());
    assert_eq!(snapshot(&w.dest), before);
    assert_eq!(m.moves().len(), 1);
}

#[test]
fn move_back_before_it_works_puts_the_original_back_and_deletes_the_copy() {
    let w = world();
    let before = snapshot(&w.app);
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let info = m.start(&req(&w, true), |_, _, _| {}).unwrap();
    let back = m.move_back(&info.id, |_, _, _| {}).unwrap();
    assert_eq!(back.state, MoveState::MovedBack);
    assert!(!is_link(&w.app));
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
    assert!(!w
        .root
        .join("d")
        .join(".hold")
        .join(format!("move-{}", info.id))
        .exists());
    assert!(!w
        .root
        .join("c")
        .join(".hold")
        .join(format!("move-{}", info.id))
        .exists());
}

#[test]
fn move_back_after_it_works_copies_it_back_and_checks_it() {
    let w = world();
    let before = snapshot(&w.app);
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let info = m.start(&req(&w, true), |_, _, _| {}).unwrap();
    m.confirm(&info.id).unwrap();
    let back = m.move_back(&info.id, |_, _, _| {}).unwrap();
    assert_eq!(back.state, MoveState::MovedBack);
    assert!(!is_link(&w.app));
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
    assert!(fs::read_dir(w.app.parent().unwrap()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".rhizome")));
}

#[test]
fn a_launcher_game_moves_without_a_link_and_can_move_back() {
    let w = world();
    let before = snapshot(&w.app);
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let info = m.start(&req(&w, false), |_, _, _| {}).unwrap();
    assert_eq!(info.state, MoveState::Waiting);
    assert!(
        fs::symlink_metadata(&w.app).is_err(),
        "no link for launcher games"
    );
    assert_eq!(snapshot(&w.dest), before);
    let back = m.move_back(&info.id, |_, _, _| {}).unwrap();
    assert_eq!(back.state, MoveState::MovedBack);
    assert_eq!(snapshot(&w.app), before);
    // Something put at the old place after the move blocks moving back.
    let info = m.start(&req(&w, false), |_, _, _| {}).unwrap();
    fs::create_dir_all(&w.app).unwrap();
    assert!(m.move_back(&info.id, |_, _, _| {}).is_err());
    assert!(w.dest.exists());
}

#[test]
fn every_refusal_leaves_everything_as_it_was() {
    let cases: Vec<(&str, Box<dyn Fn(&World) -> (MoveRequest, T)>)> = vec![
        (
            "same drive",
            Box::new(|w| {
                let mut r = req(w, true);
                r.dest = w.root.join("c").join("Apps").join("Tool");
                (r, ops(w))
            }),
        ),
        (
            "in use",
            Box::new(|w| {
                (
                    req(w, true),
                    T {
                        busy: Some("lib.dll"),
                        ..ops(w)
                    },
                )
            }),
        ),
        (
            "not NTFS",
            Box::new(|w| {
                (
                    req(w, true),
                    T {
                        filesystem: Some("exFAT"),
                        ..ops(w)
                    },
                )
            }),
        ),
        (
            "too little room",
            Box::new(|w| {
                (
                    req(w, true),
                    T {
                        free: Some(300_000),
                        ..ops(w)
                    },
                )
            }),
        ),
        (
            "already there",
            Box::new(|w| {
                fs::create_dir_all(&w.dest).unwrap();
                (req(w, true), ops(w))
            }),
        ),
        (
            "a link inside",
            Box::new(|w| {
                // A junction on Windows, a symbolic link elsewhere (Rhizome's own maker).
                RealOps
                    .make_link(&w.app.join("link"), &w.root.join("d"))
                    .unwrap();
                (req(w, true), ops(w))
            }),
        ),
        (
            "a driver",
            Box::new(|w| {
                fs::write(w.app.join("bin").join("x.sys"), b"driver").unwrap();
                (req(w, true), ops(w))
            }),
        ),
        (
            "protected",
            Box::new(|w| {
                let win = w.root.join("c").join("Windows");
                fs::create_dir_all(&win).unwrap();
                let mut r = req(w, true);
                r.source = win;
                (r, ops(w))
            }),
        ),
        (
            "a drive top",
            Box::new(|w| {
                let mut r = req(w, true);
                r.source = w.root.join("c");
                (r, ops(w))
            }),
        ),
    ];
    for (name, make) in cases {
        let w = world();
        let (r, ops) = make(&w);
        let before = snapshot(&w.root.join("c"));
        let places = Drives(w.root.clone());
        let m = mover(&w, &places, &ops);
        let err = m.start(&r, |_, _, _| {});
        assert!(err.is_err(), "{name}: {err:?}");
        let mut after = snapshot(&w.root.join("c"));
        // The data folder (the lock file) may appear; nothing else.
        after.retain(|(p, _)| !p.starts_with("data"));
        let mut before = before;
        before.retain(|(p, _)| !p.starts_with("data"));
        assert_eq!(after, before, "{name}");
        if name != "already there" {
            assert!(!w.dest.exists(), "{name}");
        }
    }
}

#[test]
fn a_removable_drive_is_allowed_and_reported() {
    let w = world();
    let ops = T {
        removable: true,
        ..ops(&w)
    };
    let places = Drives(w.root.clone());
    let m = mover(&w, &places, &ops);
    assert!(m.check(&req(&w, true)).unwrap().removable);
}

#[test]
fn a_failed_or_wrong_copy_is_deleted_and_the_app_stays() {
    for (fail, corrupt) in [(Some(2), None), (None, Some(1))] {
        let w = world();
        let before = snapshot(&w.app);
        let ops = T {
            fail_copy: fail,
            corrupt_copy: corrupt,
            ..ops(&w)
        };
        let places = Drives(w.root.clone());
        let m = mover(&w, &places, &ops);
        let err = m.start(&req(&w, true), |_, _, _| {}).unwrap_err();
        assert!(err.contains("copy"), "{err}");
        assert_eq!(snapshot(&w.app), before);
        assert!(!is_link(&w.app));
        let apps = w.root.join("d").join("Apps");
        assert_eq!(fs::read_dir(&apps).unwrap().count(), 0, "no copy left");
        assert!(matches!(m.moves()[0].state, MoveState::Failed { .. }));
    }
}

#[test]
fn when_the_original_cannot_move_aside_the_copy_is_deleted() {
    let w = world();
    let before = snapshot(&w.app);
    // Rename 1 names the copy; rename 2 moves the original aside (fails: "in use").
    let ops = T {
        fail_rename: Some(2),
        ..ops(&w)
    };
    let places = Drives(w.root.clone());
    let m = mover(&w, &places, &ops);
    let err = m.start(&req(&w, true), |_, _, _| {}).unwrap_err();
    assert!(err.contains("Close Tool"), "{err}");
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
}

#[test]
fn when_the_link_cannot_be_made_the_original_comes_back() {
    let w = world();
    let before = snapshot(&w.app);
    let ops = T {
        fail_link: true,
        ..ops(&w)
    };
    let places = Drives(w.root.clone());
    let m = mover(&w, &places, &ops);
    assert!(m.start(&req(&w, true), |_, _, _| {}).is_err());
    assert!(!is_link(&w.app));
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
}

fn crash(w: &World, ops: T) {
    let places = Drives(w.root.clone());
    let r = req(w, true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let m = Mover::new(w.data.clone(), &places, &NoGuard, &ops);
        let _ = m.start(&r, |_, _, _| {});
    }));
    assert!(result.is_err(), "the crash happened");
}

#[test]
fn a_crash_before_the_link_is_undone_by_recovery() {
    // Crash right after the original was moved aside (rename 2).
    let w = world();
    let before = snapshot(&w.app);
    crash(
        &w,
        T {
            crash_after_rename: Some(2),
            ..ops(&w)
        },
    );
    assert!(!w.app.exists(), "the original is aside");
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let id = m.moves()[0].id.clone();
    assert!(matches!(
        m.info(&id).unwrap().state,
        MoveState::Moving { .. }
    ));
    let said = m.recover(&id).unwrap();
    assert!(said.contains("left where it was"), "{said}");
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
    assert!(matches!(
        m.info(&id).unwrap().state,
        MoveState::Failed { .. }
    ));
}

#[test]
fn a_crash_after_the_link_finishes_forward() {
    let w = world();
    let before = snapshot(&w.app);
    crash(
        &w,
        T {
            crash_after_link: true,
            ..ops(&w)
        },
    );
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let id = m.moves()[0].id.clone();
    m.recover(&id).unwrap();
    assert_eq!(m.info(&id).unwrap().state, MoveState::Waiting);
    assert_eq!(snapshot(&w.app), before);
    assert!(is_link(&w.app));
}

#[test]
fn a_crash_after_naming_the_copy_deletes_it() {
    // Rename 1 named the copy, then the crash.
    let w = world();
    let before = snapshot(&w.app);
    crash(
        &w,
        T {
            crash_after_rename: Some(1),
            ..ops(&w)
        },
    );
    assert!(w.dest.exists());
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let id = m.moves()[0].id.clone();
    m.recover(&id).unwrap();
    assert!(!w.dest.exists());
    assert_eq!(snapshot(&w.app), before);
}

#[test]
// Someone else changes the disk in this test (a temp folder only).
#[allow(clippy::disallowed_methods)]
fn it_works_is_refused_when_the_link_was_changed() {
    let w = world();
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let info = m.start(&req(&w, true), |_, _, _| {}).unwrap();
    // Someone replaced the link.
    // (A junction is a folder entry on Windows: removed as a link, never its target.)
    RealOps.remove_link(&w.app).unwrap();
    fs::create_dir_all(&w.app).unwrap();
    assert!(m.confirm(&info.id).is_err());
    assert!(info.held.exists(), "the original is kept");
    assert!(m.move_back(&info.id, |_, _, _| {}).is_err());
}

#[test]
fn one_disk_job_at_a_time() {
    let w = world();
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    fs::create_dir_all(&w.data).unwrap();
    let _held = dc_quarantine_lock(&w.data);
    let m = mover(&w, &places, &ops);
    let err = m.start(&req(&w, true), |_, _, _| {}).unwrap_err();
    assert!(err.contains("running"), "{err}");
    assert!(!w.dest.exists());
}

/// Take the shared lock the way a removal in another window would.
fn dc_quarantine_lock(data: &Path) -> fs::File {
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("cleaning.lock"))
        .unwrap();
    f.try_lock().unwrap();
    f
}

#[test]
fn a_crash_during_the_copy_leaves_no_copy_after_recovery() {
    let w = world();
    let before = snapshot(&w.app);
    crash(
        &w,
        T {
            crash_on_copy: Some(2),
            ..ops(&w)
        },
    );
    let (places, ops) = (Drives(w.root.clone()), ops(&w));
    let m = mover(&w, &places, &ops);
    let id = m.moves()[0].id.clone();
    m.recover(&id).unwrap();
    assert_eq!(snapshot(&w.app), before);
    let apps = w.root.join("d").join("Apps");
    assert_eq!(fs::read_dir(&apps).unwrap().count(), 0, "no copy left");
}

#[test]
fn a_crash_while_moving_back_finishes_moving_back() {
    let w = world();
    let before = snapshot(&w.app);
    let places = Drives(w.root.clone());
    let id = {
        let ops = ops(&w);
        let m = mover(&w, &places, &ops);
        m.start(&req(&w, true), |_, _, _| {}).unwrap().id
    };
    let crashing = T {
        crash_after_unlink: true,
        ..ops(&w)
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let m = Mover::new(w.data.clone(), &places, &NoGuard, &crashing);
        let _ = m.move_back(&id, |_, _, _| {});
    }));
    assert!(result.is_err());
    assert!(
        fs::symlink_metadata(&w.app).is_err(),
        "link gone, original not back yet"
    );
    let ops = ops(&w);
    let m = mover(&w, &places, &ops);
    assert!(matches!(
        m.info(&id).unwrap().state,
        MoveState::MovingBack { .. }
    ));
    m.recover(&id).unwrap();
    assert_eq!(m.info(&id).unwrap().state, MoveState::MovedBack);
    assert_eq!(snapshot(&w.app), before);
    assert!(!w.dest.exists());
}

//! Scanner tests. They only ever read generated fixtures or temporary directories.
#![allow(missing_docs)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dc_core::{Kind, ScanStatus};
use dc_scan::{default_probe, scan, CancelToken, FileProbe, Probe, ScanError, ScanOptions};

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

/// Generate the standard fixture into a fresh temp dir; returns (guard, tree path, manifest).
fn fixture(file_count: u32) -> (tempfile::TempDir, PathBuf, fixture_gen::Manifest) {
    let dir = tmp();
    let manifest = fixture_gen::generate(&fixture_gen::Options {
        out: dir.path().to_path_buf(),
        seed: 99,
        file_count,
    })
    .unwrap();
    let tree = dir.path().join("tree");
    (dir, tree, manifest)
}

/// Every path under `root` with its kind of metadata, for before/after comparisons.
fn listing(root: &Path) -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    fn walk(dir: &Path, out: &mut Vec<(String, u64, Option<std::time::SystemTime>)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let meta = fs::symlink_metadata(&path).unwrap();
            out.push((
                path.to_string_lossy().into_owned(),
                meta.len(),
                meta.modified().ok(),
            ));
            if meta.is_dir() && !meta.file_type().is_symlink() {
                walk(&path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

// Needs file identity: on Windows this runs in dc-winmeta's tests, which have the real probe.
#[cfg(unix)]
#[test]
fn totals_match_the_fixture_manifest() {
    let (_guard, tree, m) = fixture(300);
    let result = scan(&tree, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    assert_eq!(
        result.status,
        ScanStatus::Complete,
        "errors: {:?}",
        result.errors
    );
    let t = result.totals;
    assert_eq!(t.dirs, m.totals.dirs);
    assert_eq!(t.files, m.totals.unique_files);
    assert_eq!(
        t.hardlink_extra_paths,
        m.totals.file_paths - m.totals.unique_files
    );
    assert_eq!(t.links, m.totals.links);
    assert_eq!(t.logical_bytes, m.totals.logical_bytes_unique);
    assert!(t.size_on_disk_bytes > 0);
}

#[test]
fn scanning_does_not_modify_the_tree() {
    let (_guard, tree, _) = fixture(100);
    let before = listing(&tree);
    let _ = scan(&tree, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    assert_eq!(before, listing(&tree));
}

#[test]
fn the_result_is_deterministic_and_round_trips() {
    let (_guard, tree, _) = fixture(100);
    let options = ScanOptions::default();
    let a = scan(&tree, default_probe().as_ref(), &options).unwrap();
    let b = scan(&tree, default_probe().as_ref(), &options).unwrap();
    // Reading a directory can legitimately update its access time, so atime is not compared.
    let strip = |nodes: &[dc_core::Node]| {
        nodes
            .iter()
            .cloned()
            .map(|mut n| {
                n.atime = None;
                n
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(strip(&a.nodes), strip(&b.nodes));
    assert_eq!(a.totals, b.totals);
    assert!(a.tree().is_ok());
}

#[test]
fn links_are_recorded_but_not_followed() {
    let (_guard, tree, m) = fixture(10);
    let result = scan(&tree, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    let links: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Link)
        .collect();
    assert_eq!(links.len() as u64, m.totals.links);
    for link in links {
        assert_eq!(link.logical_size, 0);
        assert_eq!(link.agg.total_logical, 0);
        assert!(link.flags.reparse);
        // A link never has children in the stored tree.
        assert!(result.nodes.iter().all(|n| n.parent != Some(link.id)));
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_is_not_entered() {
    let dir = tmp();
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("big.bin"), vec![1u8; 100_000]).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("to_outside")).unwrap();
    fs::write(root.join("small.txt"), b"hi").unwrap();

    let result = scan(&root, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    assert_eq!(result.totals.files, 1);
    assert_eq!(result.totals.logical_bytes, 2);
    assert_eq!(result.totals.links, 1);
}

#[cfg(unix)]
#[test]
fn a_root_that_is_a_link_is_refused() {
    let dir = tmp();
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let err = scan(&link, default_probe().as_ref(), &ScanOptions::default()).unwrap_err();
    assert!(matches!(err, ScanError::RootIsLink(_)));
}

// Needs file identity: on Windows this runs in dc-winmeta's tests, which have the real probe.
#[cfg(unix)]
#[test]
fn hardlinks_are_counted_once_and_the_first_name_wins() {
    let dir = tmp();
    let root = dir.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.bin"), vec![7u8; 10_000]).unwrap();
    fs::hard_link(root.join("a.bin"), root.join("b.bin")).unwrap();
    fs::hard_link(root.join("a.bin"), root.join("c.bin")).unwrap();

    let result = scan(&root, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    assert_eq!(result.totals.files, 1);
    assert_eq!(result.totals.logical_bytes, 10_000);
    assert_eq!(result.totals.hardlink_extra_paths, 2);
    let counted: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::File && n.counted)
        .collect();
    assert_eq!(counted.len(), 1);
    assert_eq!(counted[0].name, "a.bin");
    assert!(result
        .nodes
        .iter()
        .filter(|n| n.name != "a.bin" && n.kind == Kind::File)
        .all(|n| n.flags.hardlink));
}

#[test]
fn drive_roots_need_permission() {
    #[cfg(unix)]
    let root = Path::new("/");
    #[cfg(windows)]
    let root = Path::new(r"C:\");
    let err = scan(root, default_probe().as_ref(), &ScanOptions::default()).unwrap_err();
    assert!(matches!(err, ScanError::DriveRootRefused(_)));
}

#[test]
fn a_missing_root_and_a_file_root_are_errors() {
    let dir = tmp();
    let p = default_probe();
    let missing = dir.path().join("nope");
    assert!(matches!(
        scan(&missing, p.as_ref(), &ScanOptions::default()),
        Err(ScanError::Root { .. })
    ));
    let file = dir.path().join("f");
    fs::write(&file, b"x").unwrap();
    assert!(matches!(
        scan(&file, p.as_ref(), &ScanOptions::default()),
        Err(ScanError::NotADirectory(_))
    ));
}

/// Wraps the real probe: fails for one name and/or cancels after N files.
struct ScriptedProbe {
    inner: Box<dyn FileProbe>,
    fail_name: Option<&'static str>,
    cancel_after: Option<(usize, CancelToken)>,
    calls: AtomicUsize,
}

impl FileProbe for ScriptedProbe {
    fn probe(&self, path: &Path, meta: &fs::Metadata) -> io::Result<Probe> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some((after, token)) = &self.cancel_after {
            if n >= *after {
                token.cancel();
            }
        }
        if self
            .fail_name
            .is_some_and(|f| path.file_name().is_some_and(|n| n == f))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "scripted denial",
            ));
        }
        self.inner.probe(path, meta)
    }
}

#[test]
fn a_failing_entry_is_reported_and_the_scan_continues() {
    let dir = tmp();
    let root = dir.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("ok.txt"), b"12345").unwrap();
    fs::write(root.join("bad.bin"), b"1234567890").unwrap();
    let probe = ScriptedProbe {
        inner: default_probe(),
        fail_name: Some("bad.bin"),
        cancel_after: None,
        calls: AtomicUsize::new(0),
    };

    let result = scan(&root, &probe, &ScanOptions::default()).unwrap();
    assert_eq!(result.status, ScanStatus::Partial);
    assert_eq!(result.totals.errors, 1);
    assert_eq!(result.errors[0].op, "probe");
    assert_eq!(result.errors[0].kind, "permission_denied");
    // The file is still counted, with a fallback size on disk.
    assert_eq!(result.totals.files, 2);
    assert_eq!(result.totals.logical_bytes, 15);
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp();
    let root = dir.path().join("root");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(root.join("ok.txt"), b"x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read_dir(&locked).is_ok(); // running as root
    let result = scan(&root, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if readable_anyway {
        eprintln!("skipped: permissions are not enforced for this user");
        return;
    }
    assert_eq!(result.status, ScanStatus::Partial);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].op, "read_dir");
    assert_eq!(result.totals.files, 1);
}

#[test]
fn cancelling_stops_the_scan_early_and_marks_it() {
    let (_guard, tree, _) = fixture(2000);
    let cancel = CancelToken::new();
    let probe = ScriptedProbe {
        inner: default_probe(),
        fail_name: None,
        cancel_after: Some((50, cancel.clone())),
        calls: AtomicUsize::new(0),
    };
    let options = ScanOptions {
        cancel,
        ..ScanOptions::default()
    };
    let result = scan(&tree, &probe, &options).unwrap();
    assert_eq!(result.status, ScanStatus::Cancelled);
    assert!(
        result.totals.files < 2000,
        "scanned {} files",
        result.totals.files
    );
    assert!(result.tree().is_ok());
}

#[test]
fn a_scan_cancelled_before_it_starts_returns_an_empty_cancelled_result() {
    let (_guard, tree, _) = fixture(10);
    let cancel = CancelToken::new();
    cancel.cancel();
    let options = ScanOptions {
        cancel,
        ..ScanOptions::default()
    };
    let result = scan(&tree, default_probe().as_ref(), &options).unwrap();
    assert_eq!(result.status, ScanStatus::Cancelled);
    assert_eq!(result.totals.files, 0);
}

#[test]
fn filters_change_what_is_stored_but_not_the_totals() {
    let (_guard, tree, _) = fixture(300);
    let full = scan(&tree, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    let options = ScanOptions {
        max_depth: Some(2),
        min_size: Some(100_000),
        ..ScanOptions::default()
    };
    let small = scan(&tree, default_probe().as_ref(), &options).unwrap();
    assert_eq!(small.totals, full.totals);
    assert!(small.nodes.len() < full.nodes.len());
    assert!(small.nodes.iter().all(|n| n.depth <= 2));
    assert!(small
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::File)
        .all(|n| n.logical_size >= 100_000));
    assert_eq!(small.nodes[0].agg, full.nodes[0].agg);
    assert!(small.tree().is_ok());
}

#[test]
fn progress_is_reported() {
    let (_guard, tree, _) = fixture(500);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let options = ScanOptions {
        progress: Some(Arc::new(move |p| sink.lock().unwrap().push(p.clone()))),
        progress_interval: Duration::from_millis(1),
        ..ScanOptions::default()
    };
    let result = scan(&tree, default_probe().as_ref(), &options).unwrap();
    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty());
    let last = seen.last().unwrap();
    assert!(
        last.files > 0 && last.files <= result.totals.files + result.totals.hardlink_extra_paths
    );
}

#[cfg(unix)]
#[test]
fn sparse_files_use_less_space_on_disk_than_their_length() {
    let (_guard, tree, _) = fixture(10);
    let result = scan(&tree, default_probe().as_ref(), &ScanOptions::default()).unwrap();
    let sparse = result
        .nodes
        .iter()
        .find(|n| n.name == "sparse_64MiB.bin")
        .unwrap();
    if sparse.logical_size == 64 * 1024 * 1024 {
        assert!(sparse.size_on_disk < sparse.logical_size);
        assert!(sparse.flags.sparse);
    }
}

/// A second tripwire next to clippy's `disallowed-methods`: the scanner and core sources must
/// never mention a destructive call.
#[test]
fn scanner_sources_contain_no_destructive_calls() {
    let forbidden = [
        "remove_file",
        "remove_dir",
        "fs::rename",
        "set_permissions",
        "OpenOptions",
        "File::create",
        "fs::write",
        "set_len",
    ];
    for crate_dir in ["src", "../dc-core/src"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(crate_dir);
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let text = fs::read_to_string(&path).unwrap();
            // Only look at code above the unit-test module, if any.
            let code = text.split("#[cfg(test)]").next().unwrap();
            for word in forbidden {
                assert!(!code.contains(word), "{} mentions {word}", path.display());
            }
        }
    }
}

//! A live scan sends every entry once, each folder before its contents, then a summary that
//! agrees with the finished scan. Fixture trees only.

use std::collections::HashMap;

use dc_api::live::{run, LiveEvent, LiveOptions};
use dc_core::ScanStatus;

fn fixture(file_count: u32) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("dc-api-live")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    fixture_gen::generate(&fixture_gen::Options {
        out: dir.path().to_path_buf(),
        seed: 7,
        file_count,
    })
    .unwrap();
    let tree = dir.path().join("tree");
    (dir, tree)
}

#[test]
fn live_events_rebuild_the_same_tree_as_the_finished_scan() {
    let (_guard, tree) = fixture(500);
    let mut events = Vec::new();
    let mut opts = LiveOptions::new(&tree);
    opts.batch_interval = std::time::Duration::from_millis(5);
    let scan = run(opts, |e| events.push(e)).unwrap();

    let LiveEvent::Started { root, .. } = &events[0] else {
        panic!("first event is not Started")
    };
    let sep = std::path::MAIN_SEPARATOR;
    // Rebuild like a view does: folder path -> (size total), counting files as they arrive.
    let mut dirs: HashMap<String, ()> = HashMap::from([(root.clone(), ())]);
    let mut entries = 0usize;
    let mut bytes = 0u64;
    let mut finished = None;
    for e in &events[1..] {
        match e {
            LiveEvent::Batch { dirs: batch } => {
                for d in batch {
                    assert!(
                        dirs.contains_key(&d.dir),
                        "{} arrived before its folder",
                        d.dir
                    );
                    for en in &d.entries {
                        entries += 1;
                        bytes += en.size;
                        if en.kind == 1 {
                            dirs.insert(format!("{}{sep}{}", d.dir, en.name), ());
                        }
                    }
                }
            }
            LiveEvent::Finished(s) => finished = Some(s.clone()),
            LiveEvent::Started { .. } => panic!("started twice"),
            _ => {}
        }
    }
    let summary = finished.expect("a Finished event");
    assert!(matches!(events.last(), Some(LiveEvent::Finished(_))));
    assert_eq!(entries, scan.nodes.len() - 1);
    assert_eq!(summary.status, "complete");
    // Hard links: what was sent minus the uncounted second names is the scan's total.
    let by_path: HashMap<String, u64> = {
        let t = scan.tree().unwrap();
        let paths = dc_api::analysis::full_paths(&t, root);
        t.nodes()
            .iter()
            .map(|n| (paths[n.id as usize].clone(), n.size_on_disk))
            .collect()
    };
    let uncounted: u64 = summary.uncounted.iter().map(|p| by_path[p]).sum();
    assert_eq!(bytes - uncounted, scan.totals.size_on_disk_bytes);
    assert_eq!(summary.bytes_on_disk, scan.totals.size_on_disk_bytes);
    // Every mark points at a path that was sent.
    for m in &summary.marks {
        assert!(by_path.contains_key(&m.path), "{}", m.path);
        if m.mark.risk == "dont_touch" {
            assert!(!m.mark.offered);
        }
    }
}

#[test]
fn a_cancelled_live_scan_finishes_as_cancelled_with_no_checks() {
    let (_guard, tree) = fixture(200);
    let opts = LiveOptions::new(&tree);
    opts.cancel.cancel();
    let mut last = None;
    let scan = run(opts, |e| last = Some(e)).unwrap();
    assert_eq!(scan.status, ScanStatus::Cancelled);
    let Some(LiveEvent::Finished(s)) = last else {
        panic!("no Finished event")
    };
    assert_eq!(s.status, "cancelled");
    assert!(s.marks.is_empty());
}

#[test]
fn a_missing_folder_fails_with_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut events = Vec::new();
    let res = run(LiveOptions::new(dir.path().join("nope")), |e| {
        events.push(e)
    });
    assert!(res.is_err());
    assert!(matches!(events.last(), Some(LiveEvent::Failed { .. })));
}

#[test]
fn events_serialize_with_a_type_tag() {
    let json = serde_json::to_string(&LiveEvent::Analysing).unwrap();
    assert_eq!(json, r#"{"type":"analysing"}"#);
}

#[test]
fn installed_apps_are_read_alongside_and_used_for_the_checks() {
    let (_guard, tree) = fixture(50);
    let mut opts = LiveOptions::new(&tree);
    opts.apps = Some(Box::new(|| {
        Some(dc_winmeta::build_snapshot(&dc_winmeta::FakeSource::new()))
    }));
    let mut last = None;
    run(opts, |e| last = Some(e)).unwrap();
    let Some(LiveEvent::Finished(s)) = last else {
        panic!("no Finished event")
    };
    assert!(s.apps_known);
}

#[test]
fn a_finished_live_scan_can_be_turned_into_a_view_file() {
    let (_guard, tree) = fixture(300);
    let mut summary = None;
    let kept = dc_api::live::run_keep(LiveOptions::new(&tree), |e| {
        if let LiveEvent::Finished(s) = e {
            summary = Some(s);
        }
    })
    .unwrap();
    let summary = summary.expect("finished");
    let view = dc_api::view::build(&kept.scan, kept.apps.as_ref()).unwrap();
    // The view adds one shared top ("This PC") above the scanned folder.
    assert_eq!(view.nodes.name.len(), kept.scan.nodes.len() + 1);
    assert_eq!(view.marks.len(), summary.marks.len());
    let text = serde_json::to_string(&view).unwrap();
    assert!(text.starts_with("{\"schema\":\"dc-view\""));
    // Every error is listed (up to the cap), each with a reason.
    let listed = usize::try_from(summary.errors)
        .unwrap()
        .min(dc_api::live::MAX_UNREADABLE);
    assert_eq!(summary.unreadable.len(), listed);
    assert!(summary.unreadable.iter().all(|u| !u.why.is_empty()));
}

#[cfg(unix)]
#[test]
fn folders_that_cannot_be_read_are_listed_with_a_reason() {
    use std::os::unix::fs::PermissionsExt;
    let (_guard, tree) = fixture(50);
    let locked = tree.join("locked-folder");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("inside.txt"), b"x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::read_dir(&locked).is_ok(); // running as root
    let mut summary = None;
    let _ = run(LiveOptions::new(&tree), |e| {
        if let LiveEvent::Finished(s) = e {
            summary = Some(s);
        }
    });
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let summary = summary.expect("finished");
    if readable_anyway {
        return;
    }
    assert_eq!(summary.status, "partial");
    let hit = summary
        .unreadable
        .iter()
        .find(|u| u.path.ends_with("locked-folder"))
        .expect("the locked folder is listed");
    assert!(hit.why.contains("did not allow"), "{}", hit.why);
}

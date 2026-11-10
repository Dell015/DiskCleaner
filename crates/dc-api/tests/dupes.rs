//! Duplicate finding on small temporary folders. Files are only read; the test checks that
//! nothing changed.

use std::path::Path;

use dc_api::dupes::{find_duplicates, DupEvent, DupOptions};
use dc_api::live::{run, LiveOptions};

const MIB: usize = 1024 * 1024;

fn bytes(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("dc-api-dupes")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    let root = dir.path().join("disk");
    let w = |rel: &str, data: &[u8]| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    let a = bytes(1, 2 * MIB + 7);
    w("Videos/a.bin", &a);
    w("Backup/copy of a.bin", &a);
    w("Backup/old/a again.bin", &a);
    // Same size, different end: told apart by the quick look.
    let mut c = a.clone();
    *c.last_mut().unwrap() ^= 0xFF;
    w("Other/c.bin", &c);
    // Same size, same ends, different middle: only the full read tells it apart.
    let mut d = a.clone();
    d[MIB] ^= 0xFF;
    w("Other/d.bin", &d);
    // A second pair, smaller.
    let e = bytes(9, MIB + 1);
    w("Videos/e.bin", &e);
    w("Downloads/e (1).bin", &e);
    // Below the size limit: never compared.
    w("small1.txt", b"same");
    w("small2.txt", b"same");
    (dir, root)
}

fn scan(root: &Path) -> dc_core::ScanResult {
    run(LiveOptions::new(root), |_| {}).unwrap()
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for entry in walk(root) {
        out.push((
            entry.to_string_lossy().into_owned(),
            std::fs::read(&entry).unwrap(),
        ));
    }
    out.sort();
    out
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn collect(scan: &dc_core::ScanResult, opts: &DupOptions) -> Vec<DupEvent> {
    let mut events = Vec::new();
    find_duplicates(scan, opts, |e| events.push(e)).unwrap();
    events
}

#[test]
fn identical_files_are_found_and_lookalikes_are_not() {
    let (_guard, root) = setup();
    let before = snapshot(&root);
    let result = scan(&root);
    let events = collect(&result, &DupOptions::default());
    let groups: Vec<(u64, Vec<String>)> = events
        .iter()
        .filter_map(|e| match e {
            DupEvent::Group { size, paths } => Some((*size, paths.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 2, "{groups:?}");
    // Biggest first.
    assert_eq!(groups[0].0, (2 * MIB + 7) as u64);
    let names: Vec<&str> = groups[0]
        .1
        .iter()
        .map(|p| p.rsplit(['/', '\\']).next().unwrap())
        .collect();
    assert_eq!(names.len(), 3);
    assert!(
        names.contains(&"a.bin")
            && names.contains(&"copy of a.bin")
            && names.contains(&"a again.bin")
    );
    assert!(!groups
        .iter()
        .flat_map(|g| &g.1)
        .any(|p| p.ends_with("c.bin") || p.ends_with("d.bin")));
    let Some(DupEvent::Started { files, bytes }) = events.first().cloned() else {
        panic!("first event is Started")
    };
    assert_eq!(files, 7);
    let Some(DupEvent::Finished {
        status,
        groups: n,
        wasted,
        unreadable,
    }) = events.last().cloned()
    else {
        panic!("last event is Finished")
    };
    assert_eq!((status, n, unreadable), ("complete", 2, 0));
    assert_eq!(wasted, 2 * (2 * MIB + 7) as u64 + (MIB + 1) as u64);
    let last_done = events
        .iter()
        .filter_map(|e| match e {
            DupEvent::Progress { done, .. } => Some(*done),
            _ => None,
        })
        .next_back()
        .unwrap();
    assert_eq!(last_done, bytes, "progress ends at the total");
    assert_eq!(snapshot(&root), before, "nothing changed");
}

#[test]
fn a_folder_can_be_checked_on_its_own() {
    let (_guard, root) = setup();
    let result = scan(&root);
    let videos = dc_api::live::display_path(
        &std::fs::canonicalize(root.join("Videos"))
            .unwrap()
            .to_string_lossy(),
    );
    let opts = DupOptions {
        within: Some(videos),
        ..DupOptions::default()
    };
    let events = collect(&result, &opts);
    // Inside Videos, a.bin and e.bin have no twin.
    assert!(!events.iter().any(|e| matches!(e, DupEvent::Group { .. })));
}

#[test]
fn a_cancelled_search_stops_and_says_so() {
    let (_guard, root) = setup();
    let result = scan(&root);
    let opts = DupOptions::default();
    opts.cancel.cancel();
    let events = collect(&result, &opts);
    assert!(matches!(
        events.last(),
        Some(DupEvent::Finished {
            status: "cancelled",
            ..
        })
    ));
    assert!(!events.iter().any(|e| matches!(e, DupEvent::Group { .. })));
}

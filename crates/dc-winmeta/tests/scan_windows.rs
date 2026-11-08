//! End-to-end scans on Windows with the real probe. These are the checks that need file
//! identity (hardlinks), so they live here: `dc-scan` itself cannot depend on this crate.
#![allow(missing_docs)]
#![cfg(windows)]

use std::fs;

use dc_core::{Kind, ScanStatus};
use dc_scan::{scan, ScanOptions};
use dc_winmeta::WindowsProbe;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

#[test]
fn totals_match_the_fixture_manifest() {
    let dir = tmp();
    let m = fixture_gen::generate(&fixture_gen::Options {
        out: dir.path().to_path_buf(),
        seed: 99,
        file_count: 300,
    })
    .unwrap();
    let result = scan(
        &dir.path().join("tree"),
        &WindowsProbe,
        &ScanOptions::default(),
    )
    .unwrap();
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
fn hardlinks_are_counted_once_and_the_first_name_wins() {
    let dir = tmp();
    let root = dir.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.bin"), vec![7u8; 10_000]).unwrap();
    fs::hard_link(root.join("a.bin"), root.join("b.bin")).unwrap();
    fs::hard_link(root.join("a.bin"), root.join("c.bin")).unwrap();

    let result = scan(&root, &WindowsProbe, &ScanOptions::default()).unwrap();
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
}

#[test]
fn size_on_disk_is_the_real_allocation_not_an_estimate() {
    let dir = tmp();
    let root = dir.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("small.bin"), vec![1u8; 5_000]).unwrap();
    let result = scan(&root, &WindowsProbe, &ScanOptions::default()).unwrap();
    let file = result.nodes.iter().find(|n| n.name == "small.bin").unwrap();
    assert_eq!(file.logical_size, 5_000);
    // On NTFS this is a whole number of clusters (usually 8192); never smaller than the data.
    assert!(file.size_on_disk >= 5_000 && file.size_on_disk.is_multiple_of(512));
}

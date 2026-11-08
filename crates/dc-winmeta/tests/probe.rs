//! Windows-only checks of the probe against real (temporary) files.
#![allow(missing_docs)]
#![cfg(windows)]

use std::fs;

use dc_scan::FileProbe;
use dc_winmeta::WindowsProbe;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

#[test]
fn identity_link_count_and_allocation_are_reported() {
    let dir = tmp();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    fs::write(&a, vec![1u8; 10_000]).unwrap();
    fs::hard_link(&a, &b).unwrap();
    let probe = WindowsProbe;
    let pa = probe.probe(&a, &fs::symlink_metadata(&a).unwrap()).unwrap();
    let pb = probe.probe(&b, &fs::symlink_metadata(&b).unwrap()).unwrap();
    assert_eq!(pa.file_id, pb.file_id);
    assert!(pa.file_id.is_some());
    assert_eq!(pa.link_count, 2);
    let allocated = pa.size_on_disk.unwrap();
    assert!(
        allocated >= 10_000 && allocated % 512 == 0,
        "allocated {allocated}"
    );
}

#[test]
fn different_files_have_different_ids() {
    let dir = tmp();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fs::write(&a, b"x").unwrap();
    fs::write(&b, b"y").unwrap();
    let probe = WindowsProbe;
    let pa = probe.probe(&a, &fs::symlink_metadata(&a).unwrap()).unwrap();
    let pb = probe.probe(&b, &fs::symlink_metadata(&b).unwrap()).unwrap();
    assert_ne!(pa.file_id, pb.file_id);
    assert_eq!(pa.link_count, 1);
}

#[test]
fn a_missing_file_is_an_error_not_a_panic() {
    let dir = tmp();
    let real = dir.path().join("real.txt");
    fs::write(&real, b"x").unwrap();
    let meta = fs::symlink_metadata(&real).unwrap();
    let never_created = dir.path().join("never_created.txt");
    assert!(WindowsProbe.probe(&never_created, &meta).is_err());
}

#[test]
fn long_paths_work() {
    let dir = tmp();
    let mut path = dir.path().to_path_buf();
    for _ in 0..6 {
        path.push("d".repeat(60));
    }
    // `\\?\` paths are needed to create this on older setups; std adds it automatically.
    fs::create_dir_all(&path).unwrap();
    let file = path.join("f.txt");
    fs::write(&file, b"hello").unwrap();
    assert!(file.as_os_str().len() > 260);
    let meta = fs::symlink_metadata(&file).unwrap();
    assert!(WindowsProbe.probe(&file, &meta).is_ok());
}

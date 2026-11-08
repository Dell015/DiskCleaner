#![allow(missing_docs)]
mod common;

use std::fs;
use std::path::Path;

/// Walk the tree without following links; count dirs, file paths and bytes.
fn walk(dir: &Path, t: &mut (u64, u64, u64, u64)) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let meta = fs::symlink_metadata(&p).unwrap();
        if meta.file_type().is_symlink() {
            t.3 += 1;
        } else if meta.is_dir() {
            t.0 += 1;
            walk(&p, t);
        } else {
            t.1 += 1;
            t.2 += meta.len();
        }
    }
}

#[test]
fn manifest_totals_match_what_is_on_disk() {
    let (out, m) = common::run("totals", 11, 300);
    let mut t = (0, 0, 0, 0);
    walk(&out.join("tree"), &mut t);
    assert_eq!(m.totals.dirs, t.0);
    assert_eq!(m.totals.file_paths, t.1);
    assert_eq!(m.totals.links, t.3);
    // Skipped sparse files are recorded as 0 bytes, created ones as logical size; both match disk.
    assert_eq!(m.totals.logical_bytes_all_paths, t.2);
    let top_dirs: u64 = m.top_level.iter().map(|x| x.dirs).sum();
    let top_files: u64 = m.top_level.iter().map(|x| x.file_paths).sum();
    assert_eq!(top_dirs, m.totals.dirs);
    assert_eq!(top_files, m.totals.file_paths);
    assert!(m.totals.unique_files < m.totals.file_paths); // one hardlink pair
}

#[test]
fn default_file_count_generates_quickly() {
    let start = std::time::Instant::now();
    let (_, m) = common::run(
        "default",
        fixture_gen::DEFAULT_SEED,
        fixture_gen::DEFAULT_FILE_COUNT,
    );
    assert!(start.elapsed().as_secs() < 60);
    assert!(m.totals.file_paths > 10_000);
}

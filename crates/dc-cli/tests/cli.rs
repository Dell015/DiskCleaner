//! End-to-end tests of the `dc` binary against generated fixtures.
#![allow(missing_docs)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn dc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dc"))
        .args(args)
        .output()
        .unwrap()
}

fn fixture() -> (tempfile::TempDir, PathBuf, fixture_gen::Manifest) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let manifest = fixture_gen::generate(&fixture_gen::Options {
        out: dir.path().join("fx"),
        seed: 5,
        file_count: 200,
    })
    .unwrap();
    let tree = dir.path().join("fx").join("tree");
    (dir, tree, manifest)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn scan_then_top_works_and_matches_the_manifest() {
    let (dir, tree, m) = fixture();
    let out = dir.path().join("scan.json");
    let result = dc(&[
        "scan",
        tree.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains(&format!("({} bytes)", m.totals.logical_bytes_unique)));

    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(json["totals"]["files"], m.totals.unique_files);
    assert_eq!(
        json["totals"]["logical_bytes"],
        m.totals.logical_bytes_unique
    );
    assert_eq!(json["status"], "complete");

    for flag in ["--files", "--dirs", "--ext", "--category", "--oldest"] {
        let top = dc(&["top", out.to_str().unwrap(), flag, "--limit", "3"]);
        assert_eq!(top.status.code(), Some(0), "{flag}: {}", text(&top.stderr));
        assert!(text(&top.stdout).lines().count() >= 2, "{flag}");
    }
}

#[test]
fn scan_never_overwrites_an_existing_file() {
    let (dir, tree, _) = fixture();
    let out = dir.path().join("scan.json");
    std::fs::write(&out, b"precious").unwrap();
    let again = dc(&[
        "scan",
        tree.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(std::fs::read(&out).unwrap(), b"precious");
}

#[test]
fn filters_and_size_parsing() {
    let (dir, tree, _) = fixture();
    let out = dir.path().join("small.json");
    let r = dc(&[
        "scan",
        tree.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--max-depth",
        "1",
        "--min-size",
        "64KB",
    ]);
    assert_eq!(r.status.code(), Some(0), "{}", text(&r.stderr));
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(json["options"]["min_size"], 65536);
    let bad = dc(&[
        "scan",
        tree.to_str().unwrap(),
        "--out",
        dir.path().join("x.json").to_str().unwrap(),
        "--min-size",
        "ten",
    ]);
    assert_ne!(bad.status.code(), Some(0));
}

#[test]
fn a_drive_root_is_refused() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let out = dir.path().join("root.json");
    #[cfg(unix)]
    let root = "/";
    #[cfg(windows)]
    let root = r"C:\";
    let r = dc(&["scan", root, "--out", out.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(text(&r.stderr).contains("drive root"));
    assert!(!out.exists());
}

#[test]
fn top_needs_a_report_flag_and_a_valid_file() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let junk = dir.path().join("junk.json");
    std::fs::write(&junk, b"not json").unwrap();
    assert_ne!(dc(&["top", junk.to_str().unwrap()]).status.code(), Some(0));
    assert_eq!(
        dc(&["top", junk.to_str().unwrap(), "--files"])
            .status
            .code(),
        Some(1)
    );
}

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

fn sample_snapshot_file(dir: &std::path::Path) -> PathBuf {
    use dc_winmeta::{build_snapshot, FakeSource, KnownFolder, RawUninstallEntry, RegistrySource};

    let mut fake = FakeSource::new().with_existing(r"C:\Program Files\Good");
    fake.folders = vec![KnownFolder {
        name: "ProgramFiles".into(),
        path: Some(r"C:\Program Files".into()),
    }];
    let entry = |key: &str, name: &str| RawUninstallEntry {
        key: key.into(),
        source: Some(RegistrySource::User),
        display_name: Some(name.into()),
        ..RawUninstallEntry::default()
    };
    let mut good = entry("Good", "Good App");
    good.install_location = Some(r"C:\Program Files\Good".into());
    good.publisher = Some("Good Inc".into());
    let mut gone = entry("Gone", "Gone App");
    gone.install_location = Some(r"C:\Program Files\Gone".into());
    let mut patch = entry("KB1", "Patch For Good");
    patch.parent_key_name = Some("Good".into());
    fake.entries = vec![good, gone, patch];

    let path = dir.join("apps.json");
    std::fs::write(
        &path,
        serde_json::to_string(&build_snapshot(&fake)).unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn apps_shows_a_saved_snapshot_and_hides_updates_by_default() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = sample_snapshot_file(dir.path());
    let file = file.to_str().unwrap();

    let out = dc(&["apps", "--from", file]);
    let shown = text(&out.stdout);
    assert!(out.status.success(), "{shown}");
    assert!(
        shown.contains("Good App") && shown.contains("Good Inc"),
        "{shown}"
    );
    assert!(
        shown.contains("Gone App") && shown.contains("STALE"),
        "{shown}"
    );
    assert!(!shown.contains("Patch For Good"), "{shown}");
    assert!(shown.contains("2 apps shown, 1 hidden"), "{shown}");

    let all = text(&dc(&["apps", "--from", file, "--include-hidden"]).stdout);
    assert!(all.contains("Patch For Good"), "{all}");
}

#[test]
fn apps_rejects_a_file_that_is_not_a_snapshot() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "{\"nope\": 1}").unwrap();
    let out = dc(&["apps", "--from", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("not a DiskCleaner apps snapshot"));
}

#[cfg(not(windows))]
#[test]
fn apps_without_a_file_explains_it_needs_windows() {
    let out = dc(&["apps"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("only works on Windows"));
}

#[test]
fn scan_with_an_apps_snapshot_labels_the_app_folder() {
    use dc_winmeta::{build_snapshot, FakeSource, RawUninstallEntry, RegistrySource};

    let (dir, tree, _manifest) = fixture();
    // Pick a real directory inside the fixture and pretend an app is installed there.
    let app_dir = std::fs::read_dir(&tree)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.is_dir())
        .unwrap();
    let app_dir = std::path::absolute(app_dir).unwrap();

    // Only Windows paths are recognised as install folders when reading registry text, so set
    // the folder directly: this test is about labelling, not about registry parsing.
    let mut fake = FakeSource::new();
    fake.entries = vec![RawUninstallEntry {
        key: "demo".into(),
        source: Some(RegistrySource::User),
        display_name: Some("Demo App".into()),
        ..RawUninstallEntry::default()
    }];
    let mut apps = build_snapshot(&fake);
    apps.apps[0].install_folder = Some(app_dir.display().to_string());
    let snapshot = dir.path().join("apps.json");
    std::fs::write(&snapshot, serde_json::to_string(&apps).unwrap()).unwrap();

    let out = dir.path().join("scan.json");
    let result = dc(&[
        "scan",
        tree.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--apps",
        snapshot.to_str().unwrap(),
    ]);
    let shown = text(&result.stdout);
    assert!(result.status.success(), "{shown}{}", text(&result.stderr));
    assert!(shown.contains("apps      1 app folders found"), "{shown}");

    let top = text(&dc(&["top", out.to_str().unwrap(), "--dirs", "--limit", "500"]).stdout);
    assert!(top.contains("[Demo App]"), "{top}");
}

#[test]
fn apps_shows_signatures_when_the_snapshot_has_them() {
    use dc_winmeta::{
        build_snapshot_with, BuildOptions, FakeSource, KnownFolder, RawUninstallEntry,
        RegistrySource, SignatureStatus,
    };

    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let mut fake = FakeSource::new()
        .with_existing(r"C:\Program Files\Good")
        .with_existing(r"C:\Program Files\Good\good.exe");
    fake.folders = vec![KnownFolder {
        name: "ProgramFiles".into(),
        path: Some(r"C:\Program Files".into()),
    }];
    fake.entries = vec![RawUninstallEntry {
        key: "Good".into(),
        source: Some(RegistrySource::User),
        display_name: Some("Good App".into()),
        install_location: Some(r"C:\Program Files\Good".into()),
        display_icon: Some(r"C:\Program Files\Good\good.exe,0".into()),
        ..RawUninstallEntry::default()
    }];
    fake.signatures.insert(
        r"C:\Program Files\Good\good.exe".into(),
        SignatureStatus::Signed {
            signer: Some("Good Inc".into()),
        },
    );
    let snapshot = build_snapshot_with(
        &fake,
        &BuildOptions {
            verify_signatures: true,
        },
    );
    let file = dir.path().join("apps.json");
    std::fs::write(&file, serde_json::to_string(&snapshot).unwrap()).unwrap();

    let shown = text(&dc(&["apps", "--from", file.to_str().unwrap()]).stdout);
    assert!(
        shown.contains("SIGNATURE") && shown.contains("signed: Good Inc"),
        "{shown}"
    );
    assert!(shown.contains("1 signed"), "{shown}");
}

#[test]
fn protected_reports_protected_and_unprotected_paths() {
    let out = dc(&[
        "protected",
        r"C:\Windows\System32",
        r"C:\Users\bob\Downloads\x",
    ]);
    assert!(out.status.success());
    let shown = text(&out.stdout);
    assert!(
        shown.contains("PROTECTED") && shown.contains("windows_dir"),
        "{shown}"
    );
    assert!(
        shown.contains("not protected") && shown.contains("does NOT mean safe"),
        "{shown}"
    );
}

#[test]
fn protected_fails_closed_on_odd_paths_and_lists_the_table() {
    let out = dc(&["protected", "relative/path"]);
    assert!(text(&out.stdout).contains("PROTECTED"));
    let out = dc(&["protected", "--list"]);
    assert!(text(&out.stdout).contains("windows_dir"));
    assert!(!dc(&["protected"]).status.success());
}

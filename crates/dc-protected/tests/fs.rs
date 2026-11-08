//! The real resolver, on temporary directories only.

use dc_protected::{FsResolver, PathResolver, Resolution};

#[test]
fn existing_path_resolves_and_missing_tail_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let missing = dir.path().join("nope").join("deeper");
    match FsResolver.resolve(&missing.to_string_lossy()) {
        Resolution::Resolved(text) => {
            let expected = real.join("nope").join("deeper");
            assert_eq!(
                text.replace(r"\\?\", "").to_lowercase(),
                expected
                    .to_string_lossy()
                    .replace(r"\\?\", "")
                    .to_lowercase()
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn dotdot_is_resolved_by_the_os() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("a")).unwrap();
    let tricky = dir.path().join("a").join("..").join("a");
    assert!(matches!(
        FsResolver.resolve(&tricky.to_string_lossy()),
        Resolution::Resolved(_)
    ));
}

#[cfg(unix)]
#[test]
fn symlink_is_resolved_to_its_target() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("target")).unwrap();
    let link = dir.path().join("link");
    if std::os::unix::fs::symlink(dir.path().join("target"), &link).is_err() {
        eprintln!("skipped: cannot create symlinks here");
        return;
    }
    let real = std::fs::canonicalize(dir.path().join("target")).unwrap();
    assert_eq!(
        FsResolver.resolve(&link.to_string_lossy()),
        Resolution::Resolved(real.to_string_lossy().into_owned())
    );
}

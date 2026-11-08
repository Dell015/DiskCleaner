#![allow(missing_docs)]
mod common;

use fixture_gen::{generate, validate_out, GenError, Options};

#[test]
fn refuses_non_empty_directory_and_leaves_it_untouched() {
    let out = common::out_dir("guard-nonempty");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), b"keep").unwrap();
    let err = generate(&Options {
        out: out.clone(),
        seed: 1,
        file_count: 5,
    })
    .unwrap_err();
    assert!(matches!(err, GenError::OutDirNotEmpty(_)));
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
}

#[test]
fn refuses_a_file_path() {
    let out = common::out_dir("guard-file");
    std::fs::create_dir_all(&out).unwrap();
    let file = out.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    assert!(matches!(
        validate_out(&file),
        Err(GenError::OutDirNotADirectory(_))
    ));
}

#[test]
fn refuses_parent_refs_and_empty_paths() {
    assert!(matches!(
        validate_out(std::path::Path::new("")),
        Err(GenError::OutDirUnresolvable(_))
    ));
    assert!(matches!(
        validate_out(&common::out_dir("x").join("..").join("y")),
        Err(GenError::OutDirHasParentRef(_))
    ));
}

#[test]
fn refuses_a_root_without_writing() {
    #[cfg(unix)]
    let root = std::path::Path::new("/");
    #[cfg(windows)]
    let root = std::path::Path::new(r"C:\");
    assert!(matches!(validate_out(root), Err(GenError::OutDirIsRoot(_))));
}

#[cfg(unix)]
#[test]
fn refuses_a_symlink_as_output() {
    let base = common::out_dir("guard-link");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir(base.join("real")).unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    assert!(matches!(
        validate_out(&base.join("link")),
        Err(GenError::OutDirIsLink(_))
    ));
}

#[test]
fn accepts_empty_existing_and_missing_directories() {
    let empty = common::out_dir("guard-empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(validate_out(&empty).is_ok());
    assert!(validate_out(&common::out_dir("guard-missing").join("a").join("b")).is_ok());
}

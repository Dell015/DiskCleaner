//! Tripwire: `dc-winmeta` may read the system but must never write to it, delete anything or
//! start programs. This fails the build if such calls appear in the crate's sources.
#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 19] = [
    "RegSetValue",
    "RegCreateKey",
    "RegDelete",
    "RegSaveKey",
    "RegReplaceKey",
    "RegRestoreKey",
    "RegLoadKey",
    "RegUnLoadKey",
    "RegCopyTree",
    "Command::new",
    "ShellExecute",
    "CreateProcess",
    "WinExec",
    "remove_file",
    "remove_dir",
    "fs::rename",
    "fs::write",
    "File::create",
    "OpenOptions",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn sources_never_write_delete_or_launch() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() >= 6, "expected to find the crate sources");
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for word in FORBIDDEN {
            assert!(!text.contains(word), "{} contains {word}", file.display());
        }
    }
}

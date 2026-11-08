#![allow(dead_code)]
//! Shared helpers. Tests only ever write under Cargo's per-target temp directory.

use std::path::PathBuf;

use fixture_gen::{generate, Manifest, Options};

/// A fresh, unique, empty-to-be output directory under `CARGO_TARGET_TMPDIR`.
pub fn out_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("fg-{name}-{}", std::process::id()))
}

pub fn run(name: &str, seed: u64, file_count: u32) -> (PathBuf, Manifest) {
    let out = out_dir(name);
    let opts = Options {
        out: out.clone(),
        seed,
        file_count,
    };
    let manifest = generate(&opts).expect("generation should succeed");
    (out, manifest)
}

/// Every path under `root` (relative, `/`-separated, sorted) with its size; links not followed.
pub fn snapshot(root: &std::path::Path) -> Vec<(String, u64, Option<Vec<u8>>)> {
    fn walk(
        dir: &std::path::Path,
        root: &std::path::Path,
        out: &mut Vec<(String, u64, Option<Vec<u8>>)>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                out.push((rel, 0, None));
            } else if meta.is_dir() {
                out.push((rel, 0, None));
                walk(&path, root, out);
            } else {
                // Hash only small files fully; large (sparse) ones by size.
                let hash = (meta.len() < 4 * 1024 * 1024).then(|| {
                    blake3::hash(&std::fs::read(&path).unwrap())
                        .as_bytes()
                        .to_vec()
                });
                out.push((rel, meta.len(), hash));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

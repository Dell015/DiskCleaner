//! Tripwire: the rules engine only reads data.
#![allow(missing_docs)]

#[test]
fn no_destructive_calls_in_source() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let banned = [
        "remove_file",
        "remove_dir",
        "fs::write",
        "File::create",
        "OpenOptions",
        "fs::rename",
        "fs::copy",
        "create_dir",
        "set_permissions",
        "Command::",
        "unsafe ",
        "std::fs",
    ];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for word in banned {
            assert!(!text.contains(word), "{} contains {word}", path.display());
        }
    }
}

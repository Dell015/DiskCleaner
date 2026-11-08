#![allow(missing_docs)]
mod common;

use std::fs;

#[test]
fn layout_contains_the_expected_categories_and_edge_cases() {
    let (out, m) = common::run("contents", 7, 600);
    let tree = out.join("tree");
    let names: Vec<_> = m.categories.iter().map(|c| c.name.as_str()).collect();
    for want in [
        "steam",
        "chrome",
        "brave",
        "hoyoverse",
        "appdata",
        "temp",
        "leftover",
        "system32",
        "edge",
        "edge_deep",
        "edge_many_files",
        "edge_zero",
        "edge_unicode",
        "edge_long",
        "links",
    ] {
        assert!(names.contains(&want), "missing category {want}");
    }

    // Deep nesting: 64 levels below edge_deep.
    let mut deep = tree.join("edge_deep");
    for i in 0..64 {
        deep.push(format!("d{i:02}"));
    }
    assert!(deep.join("bottom.txt").is_file());

    // Many files: exactly file_count, sizes 0..=512.
    let many: Vec<_> = fs::read_dir(tree.join("edge_many_files"))
        .unwrap()
        .collect();
    assert_eq!(many.len(), 600);
    for e in many {
        assert!(e.unwrap().metadata().unwrap().len() <= 512);
    }
    let cat = m
        .categories
        .iter()
        .find(|c| c.name == "edge_many_files")
        .unwrap();
    assert_eq!(cat.count, Some(600));

    // Zero-byte files.
    let zeros = fs::read_dir(tree.join("edge_zero"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.metadata()
                .map(|m| m.is_file() && m.len() == 0)
                .unwrap_or(false)
        })
        .count();
    assert!(zeros >= 10);
    assert!(tree.join("edge_zero/empty_dir").is_dir());

    // Long name.
    let long = fs::read_dir(tree.join("edge_long")).unwrap().count();
    assert_eq!(long, 1);
}

#[test]
fn fake_executables_are_not_real_pe_files_and_text_is_marked() {
    let (out, _) = common::run("exe", 3, 10);
    let exe = fs::read(out.join("tree/Windows/System32/kernel32.dll")).unwrap();
    assert_eq!(&exe[..4], b"DCFX");
    assert_ne!(&exe[..2], b"MZ");
    let txt =
        fs::read_to_string(out.join("tree/Users/TestUser/AppData/Local/UninstalledApp/config.ini"))
            .unwrap();
    assert!(txt.starts_with("DiskCleaner fixture - dummy data\n"));
}

#[test]
fn mtimes_are_in_the_documented_range() {
    let (out, _) = common::run("mtime", 5, 10);
    let t = fs::metadata(out.join("tree/Windows/System32/ntdll.dll"))
        .unwrap()
        .modified()
        .unwrap();
    let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!((1_420_070_400..1_735_689_600).contains(&secs));
}

#[test]
fn hardlinks_share_data_and_special_entries_are_listed() {
    let (out, m) = common::run("special", 9, 10);
    let a = fs::read(out.join("tree/links/hardlink_a.bin")).unwrap();
    let b = fs::read(out.join("tree/links/hardlink_b.bin")).unwrap();
    assert_eq!(a, b);
    for kind in [
        "hardlink",
        "sparse",
        "junction",
        "symlink_file",
        "symlink_dir",
    ] {
        assert!(
            m.special.iter().any(|s| s.kind == kind),
            "missing special {kind}"
        );
    }
    for s in &m.special {
        assert_eq!(s.reason.is_some(), format!("{:?}", s.status) == "Skipped");
    }
}

//! Tests for the shortcut reader, using hand-built `.lnk` bytes and temp folders.
#![allow(missing_docs)]

use std::fs;

use dc_winmeta::{collect_shortcuts, parse_lnk};
use proptest::prelude::*;

const HEADER_SIZE: u32 = 0x4C;
const LNK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
const HAS_LINK_INFO: u32 = 0x02;
const HAS_WORKING_DIR: u32 = 0x10;
const HAS_ARGUMENTS: u32 = 0x20;
const IS_UNICODE: u32 = 0x80;

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// A minimal, valid shortcut: no ID list, LinkInfo with an ANSI base path and suffix,
/// Unicode string data with a working directory and arguments.
fn build_lnk(base: &str, suffix: &str, working_dir: &str, args: &str) -> Vec<u8> {
    let mut info = Vec::new();
    let base_offset = 28u32;
    let suffix_offset = base_offset + u32::try_from(base.len()).unwrap() + 1;
    let size = suffix_offset + u32::try_from(suffix.len()).unwrap() + 1;
    for value in [size, 0x1C, 1, 0x1C, base_offset, 0, suffix_offset] {
        info.extend_from_slice(&value.to_le_bytes());
    }
    info.extend_from_slice(base.as_bytes());
    info.push(0);
    info.extend_from_slice(suffix.as_bytes());
    info.push(0);

    let mut data = Vec::new();
    data.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    data.extend_from_slice(&LNK_CLSID);
    let flags = HAS_LINK_INFO | HAS_WORKING_DIR | HAS_ARGUMENTS | IS_UNICODE;
    data.extend_from_slice(&flags.to_le_bytes());
    data.resize(usize::try_from(HEADER_SIZE).unwrap(), 0);
    data.extend_from_slice(&info);
    for text in [working_dir, args] {
        data.extend_from_slice(&u16::try_from(text.chars().count()).unwrap().to_le_bytes());
        data.extend_from_slice(&utf16(text));
    }
    data
}

#[test]
fn a_valid_shortcut_yields_target_arguments_and_working_dir() {
    let data = build_lnk(
        r"C:\Program Files\App",
        "app.exe",
        r"C:\Program Files\App",
        "--fast",
    );
    let link = parse_lnk(&data).unwrap();
    assert_eq!(
        link.target.as_deref(),
        Some(r"C:\Program Files\App\app.exe")
    );
    assert_eq!(link.working_dir.as_deref(), Some(r"C:\Program Files\App"));
    assert_eq!(link.arguments.as_deref(), Some("--fast"));
}

#[test]
fn a_base_path_ending_in_a_separator_is_not_doubled() {
    let data = build_lnk(r"C:\App\", "app.exe", "", "");
    assert_eq!(
        parse_lnk(&data).unwrap().target.as_deref(),
        Some(r"C:\App\app.exe")
    );
}

#[test]
fn wrong_headers_are_rejected() {
    assert_eq!(parse_lnk(b""), None);
    assert_eq!(parse_lnk(b"not a shortcut at all"), None);
    let mut data = build_lnk("C:\\A", "b.exe", "", "");
    data[4] ^= 0xFF; // damage the class id
    assert_eq!(parse_lnk(&data), None);
}

#[test]
fn every_truncation_of_a_valid_shortcut_is_safe() {
    let data = build_lnk(r"C:\Program Files\App", "app.exe", r"C:\W", "-x");
    for cut in 0..data.len() {
        let _ = parse_lnk(&data[..cut]);
    }
}

#[test]
fn collecting_reads_nested_shortcuts_and_counts_broken_ones() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("Programs").join("Vendor");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("App.lnk"),
        build_lnk(r"C:\Program Files\App", "app.exe", "", ""),
    )
    .unwrap();
    fs::write(dir.path().join("Broken.lnk"), b"junk").unwrap();
    fs::write(dir.path().join("readme.txt"), b"ignored").unwrap();

    let report = collect_shortcuts(&[dir.path().to_path_buf(), dir.path().join("missing")]);
    assert_eq!(report.items.len(), 1);
    let item = &report.items[0];
    assert_eq!(item.name, "App");
    assert_eq!(
        item.target.as_deref(),
        Some(r"C:\Program Files\App\app.exe")
    );
    assert_eq!(report.problems.len(), 1);
    assert!(report.problems[0].message.contains("1 were not valid"));
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = parse_lnk(&data);
    }

    #[test]
    fn mutated_valid_shortcuts_never_panic(index in 0usize..200, value in any::<u8>()) {
        let mut data = build_lnk(r"C:\Program Files\App", "app.exe", r"C:\W", "-x");
        let at = index % data.len();
        data[at] = value;
        let _ = parse_lnk(&data);
    }
}

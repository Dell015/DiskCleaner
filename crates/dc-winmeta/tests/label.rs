//! Labelling scanned directories with the apps that own them (portable: uses Windows-style
//! path text and an in-memory tree, no real disk).
#![allow(missing_docs)]

use std::path::Path;

use dc_core::{Kind, Node, Tree};
use dc_winmeta::{app_labels, build_snapshot, FakeSource, RawUninstallEntry, RegistrySource};

fn snapshot() -> dc_winmeta::AppSnapshot {
    let entry = |key: &str, name: &str, folder: &str| RawUninstallEntry {
        key: key.into(),
        source: Some(RegistrySource::Machine64),
        display_name: Some(name.into()),
        install_location: Some(folder.into()),
        ..RawUninstallEntry::default()
    };
    let mut patch = entry("KB", "Patch", r"C:\Program Files\Alpha");
    patch.parent_key_name = Some("Alpha".into());
    let mut fake = FakeSource::new()
        .with_existing(r"C:\Program Files\Alpha")
        .with_existing(r"C:\Program Files\Beta")
        .with_existing(r"C:\Program Files\Beta\Inner");
    fake.entries = vec![
        entry("alpha", "Alpha App", r"C:\Program Files\Alpha"),
        entry("beta", "Beta App", r"C:\Program Files\Beta"),
        entry("beta2", "Beta Helper", r"C:\Program Files\Beta"),
        entry("inner", "Beta Inner", r"C:\Program Files\Beta\Inner"),
        patch,
    ];
    build_snapshot(&fake)
}

fn tree() -> Tree {
    let mut tree = Tree::with_root(Node::new(r"C:\Program Files", Kind::Dir));
    for name in ["Alpha", "Beta", "Gamma"] {
        tree.push(0, Node::new(name, Kind::Dir)).unwrap();
    }
    tree.push(2, Node::new("Inner", Kind::Dir)).unwrap();
    tree.push(1, Node::new("file.txt", Kind::File)).unwrap();
    tree
}

#[test]
fn install_folders_are_labelled_and_shared_folders_get_every_app() {
    let labels = app_labels(&tree(), Path::new(r"C:\Program Files"), &snapshot());
    let pairs: Vec<(u32, &str)> = labels.iter().map(|l| (l.node, l.app.as_str())).collect();
    assert_eq!(
        pairs,
        [
            (1, "Alpha App"),
            (2, "Beta App"),
            (2, "Beta Helper"),
            (4, "Beta Inner")
        ]
    );
}

#[test]
fn patches_and_unmatched_folders_are_not_labelled() {
    let labels = app_labels(&tree(), Path::new(r"c:\program files"), &snapshot());
    assert!(labels.iter().all(|l| l.app != "Patch"));
    assert!(labels.iter().all(|l| l.node != 3), "Gamma has no app");
}

#[test]
fn a_scan_root_inside_an_app_folder_labels_the_root_with_the_closest_app() {
    let mut tree = Tree::with_root(Node::new(r"C:\Program Files\Beta\Inner\bin", Kind::Dir));
    tree.push(0, Node::new("sub", Kind::Dir)).unwrap();
    let labels = app_labels(
        &tree,
        Path::new(r"\\?\C:\Program Files\Beta\Inner\bin"),
        &snapshot(),
    );
    assert_eq!(labels.len(), 1);
    assert_eq!((labels[0].node, labels[0].app.as_str()), (0, "Beta Inner"));
}

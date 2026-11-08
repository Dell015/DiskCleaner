//! Helpers shared by the attribution tests.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use dc_core::{Kind, Node, ScanResult, ScanStatus, StoredOptions, Totals, Tree, SCHEMA_VERSION};
use dc_winmeta::{build_snapshot, AppSnapshot, FakeSource, RawUninstallEntry, RegistrySource};

pub const ROOT: &str = r"C:\";

pub fn scan(files: &[(&str, u64)]) -> ScanResult {
    let mut tree = Tree::with_root(Node::new(ROOT, Kind::Dir));
    let mut dirs: BTreeMap<String, u32> = BTreeMap::new();
    for (path, size) in files {
        let parts: Vec<&str> = path.split('\\').collect();
        let mut parent = 0u32;
        let mut so_far = String::new();
        for (i, part) in parts.iter().enumerate() {
            so_far.push('\\');
            so_far.push_str(part);
            if i + 1 == parts.len() {
                let mut file = Node::new(*part, Kind::File);
                file.set_extension_from_name();
                file.logical_size = *size;
                file.size_on_disk = *size;
                parent = tree.push(parent, file).unwrap();
            } else if let Some(id) = dirs.get(&so_far) {
                parent = *id;
            } else {
                parent = tree.push(parent, Node::new(*part, Kind::Dir)).unwrap();
                dirs.insert(so_far.clone(), parent);
            }
        }
        let _ = parent;
    }
    tree.aggregate();
    let total = tree.root().unwrap().agg;
    ScanResult {
        schema_version: SCHEMA_VERSION,
        tool_version: "test".into(),
        root: ROOT.into(),
        started_unix: 0,
        finished_unix: 0,
        status: ScanStatus::Complete,
        options: StoredOptions::default(),
        totals: Totals {
            files: total.file_count,
            logical_bytes: total.total_logical,
            size_on_disk_bytes: total.total_on_disk,
            ..Totals::default()
        },
        errors: Vec::new(),
        nodes: tree.into_nodes(),
        app_labels: Vec::new(),
    }
}

/// (name, publisher, install folder)
pub fn apps(list: &[(&str, &str, Option<&str>)]) -> AppSnapshot {
    let mut fake = FakeSource::new();
    fake.entries = list
        .iter()
        .enumerate()
        .map(|(i, (name, publisher, _))| RawUninstallEntry {
            key: format!("key{i}"),
            source: Some(RegistrySource::Machine64),
            display_name: Some((*name).into()),
            publisher: (!publisher.is_empty()).then(|| (*publisher).into()),
            ..RawUninstallEntry::default()
        })
        .collect();
    let mut snapshot = build_snapshot(&fake);
    for (name, _, folder) in list {
        if let Some(app) = snapshot.apps.iter_mut().find(|a| a.name == *name) {
            app.install_folder = folder.map(str::to_owned);
        }
    }
    snapshot
}

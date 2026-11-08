//! Builds small scans for candidate tests.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use dc_core::{
    Kind, Node, ScanResult, ScanStatus, StoredOptions, Timestamp, Totals, Tree, SCHEMA_VERSION,
};

pub const NOW: i64 = 1_800_000_000;
pub const DAY: i64 = 86_400;

pub struct F {
    pub path: &'static str,
    pub size: u64,
    pub age_days: i64,
    pub special: &'static str, // "", "link", "cloud"
}

pub fn f(path: &'static str, size: u64, age_days: i64) -> F {
    F {
        path,
        size,
        age_days,
        special: "",
    }
}

pub fn scan(files: &[F]) -> ScanResult {
    let mut tree = Tree::with_root(Node::new(r"C:\", Kind::Dir));
    let mut dirs: BTreeMap<String, u32> = BTreeMap::new();
    for file in files {
        let parts: Vec<&str> = file.path.split('\\').collect();
        let mut parent = 0u32;
        let mut so_far = String::new();
        for (i, part) in parts.iter().enumerate() {
            so_far.push('\\');
            so_far.push_str(part);
            if i + 1 == parts.len() {
                let mut node = Node::new(*part, Kind::File);
                node.set_extension_from_name();
                node.logical_size = file.size;
                node.size_on_disk = file.size;
                node.mtime = Some(Timestamp {
                    secs: NOW - file.age_days * DAY,
                    nanos: 0,
                });
                node.flags.reparse = file.special == "link";
                node.flags.cloud = file.special == "cloud";
                let _ = tree.push(parent, node);
            } else if let Some(id) = dirs.get(&so_far) {
                parent = *id;
            } else if let Ok(id) = tree.push(parent, Node::new(*part, Kind::Dir)) {
                dirs.insert(so_far.clone(), id);
                parent = id;
            }
        }
    }
    tree.aggregate();
    let total = tree.root().map(|r| r.agg).unwrap_or_default();
    ScanResult {
        schema_version: SCHEMA_VERSION,
        tool_version: "test".into(),
        root: r"C:\".into(),
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

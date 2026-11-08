//! Builds small scans and app snapshots for the leftover tests.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use dc_attrib::{attribute, AttribResult};
use dc_core::SCHEMA_VERSION;
use dc_core::{Kind, Node, ScanResult, ScanStatus, StoredOptions, Timestamp, Totals, Tree};
use dc_protected::FakeResolver;
use dc_rules::Classifier;
use dc_winmeta::{build_snapshot, AppSnapshot, FakeSource, RawUninstallEntry, RegistrySource};

pub const NOW: i64 = 1_800_000_000;
pub const DAY: i64 = 86_400;

/// A file: path below `C:\`, size, age in days, and "" / "link".
pub struct F {
    pub path: &'static str,
    pub size: u64,
    pub age: i64,
    pub special: &'static str,
}

pub fn f(path: &'static str, age: i64) -> F {
    F {
        path,
        size: 1000,
        age,
        special: "",
    }
}

pub fn link(path: &'static str, age: i64) -> F {
    F {
        path,
        size: 0,
        age,
        special: "link",
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
                    secs: NOW - file.age * DAY,
                    nanos: 0,
                });
                node.flags.reparse = file.special == "link";
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
        finished_unix: NOW,
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

/// One registered app: name, publisher, install folder, stale?
pub struct A {
    pub name: &'static str,
    pub publisher: &'static str,
    pub folder: Option<&'static str>,
    pub stale: bool,
}

pub fn app(name: &'static str, folder: &'static str, stale: bool) -> A {
    A {
        name,
        publisher: "",
        folder: Some(folder),
        stale,
    }
}

pub fn snapshot(list: &[A]) -> AppSnapshot {
    let mut fake = FakeSource::new();
    fake.entries = list
        .iter()
        .enumerate()
        .map(|(i, a)| RawUninstallEntry {
            key: format!("key{i}"),
            source: Some(RegistrySource::Machine64),
            display_name: Some(a.name.into()),
            publisher: (!a.publisher.is_empty()).then(|| a.publisher.into()),
            ..RawUninstallEntry::default()
        })
        .collect();
    let mut snap = build_snapshot(&fake);
    for a in list {
        if let Some(found) = snap.apps.iter_mut().find(|x| x.name == a.name) {
            found.install_folder = a.folder.map(str::to_owned);
            found.flags.stale_registration = a.stale;
            found.flags.no_install_folder = a.folder.is_none();
        }
    }
    snap
}

pub fn attribution(scan: &ScanResult, snap: &AppSnapshot) -> AttribResult {
    attribute(scan, snap).expect("attribution works on a good scan")
}

pub fn run(files: &[F], apps: &[A]) -> dc_leftovers::Leftovers {
    run_with(files, apps, &Classifier::builtin(), &FakeResolver::new())
}

pub fn run_with(
    files: &[F],
    apps: &[A],
    classifier: &Classifier,
    resolver: &FakeResolver,
) -> dc_leftovers::Leftovers {
    let scan = scan(files);
    let snap = snapshot(apps);
    let attrib = attribution(&scan, &snap);
    dc_leftovers::find_leftovers(&scan, &snap, &attrib, classifier, resolver)
        .expect("a good scan can be searched")
}

pub fn find<'a>(
    found: &'a dc_leftovers::Leftovers,
    path: &str,
) -> Option<&'a dc_leftovers::Finding> {
    found
        .findings
        .iter()
        .find(|f| f.candidate.path.eq_ignore_ascii_case(path))
}

//! Speed check on a big synthetic scan. Run by hand: cargo test -p dc-rules --release --test perf -- --ignored --nocapture
#![allow(missing_docs)]

mod common;

use std::time::Instant;

use dc_core::{Kind, Node, ScanResult, ScanStatus, StoredOptions, Totals, Tree, SCHEMA_VERSION};
use dc_protected::FakeResolver;
use dc_rules::{find_candidates, Classifier};

fn big_scan() -> ScanResult {
    let mut tree = Tree::with_root(Node::new(r"C:\Users\Randel", Kind::Dir));
    let add_dir = |t: &mut Tree, p: u32, n: &str| t.push(p, Node::new(n, Kind::Dir)).unwrap();
    let local = {
        let a = add_dir(&mut tree, 0, "AppData");
        add_dir(&mut tree, a, "Local")
    };
    // A uv-like cache: deep, every file a hardlink (blocks everything).
    let uv = add_dir(&mut tree, local, "uv");
    let cache = add_dir(&mut tree, uv, "cache");
    for i in 0..300 {
        let mut d = add_dir(&mut tree, cache, &format!("pkg{i}"));
        for depth in 0..8 {
            d = add_dir(&mut tree, d, &format!("sub{depth}"));
            for j in 0..10 {
                let mut n = Node::new(format!("f{j}.py"), Kind::File);
                n.logical_size = 100;
                n.size_on_disk = 4096;
                n.flags.hardlink = true;
                let _ = tree.push(d, n);
            }
        }
    }
    // A big ordinary tree that no rule cares about.
    let docs = add_dir(&mut tree, 0, "Documents");
    for i in 0..2000 {
        let p = add_dir(&mut tree, docs, &format!("proj{i}"));
        for k in 0..10 {
            let q = add_dir(&mut tree, p, &format!("dir{k}"));
            for j in 0..50 {
                let mut n = Node::new(format!("file{j}.txt"), Kind::File);
                n.logical_size = 1000;
                n.size_on_disk = 4096;
                let _ = tree.push(q, n);
            }
        }
    }
    tree.aggregate();
    ScanResult {
        schema_version: SCHEMA_VERSION,
        tool_version: "test".into(),
        root: r"C:\Users\Randel".into(),
        started_unix: 0,
        finished_unix: 0,
        status: ScanStatus::Complete,
        options: StoredOptions::default(),
        totals: Totals::default(),
        errors: Vec::new(),
        nodes: tree.into_nodes(),
        app_labels: Vec::new(),
    }
}

#[test]
#[ignore]
fn find_candidates_speed() {
    let scan = big_scan();
    println!("nodes: {}", scan.nodes.len());
    let c = Classifier::builtin();
    let t = Instant::now();
    let found = find_candidates(&scan, &c, &FakeResolver::new(), 1_800_000_000).unwrap();
    println!(
        "find_candidates: {:?} items={} blocked={}",
        t.elapsed(),
        found.items.len(),
        found.blocked.len()
    );
}

#[test]
#[ignore]
fn write_big_scan() {
    let Ok(path) = std::env::var("DC_PERF_OUT") else {
        return;
    };
    let scan = big_scan();
    std::fs::write(path, serde_json::to_vec(&scan).unwrap()).unwrap();
}

#[test]
#[ignore]
fn phases() {
    let Ok(path) = std::env::var("DC_PERF_IN") else {
        return;
    };
    let t = Instant::now();
    let scan: ScanResult =
        serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
            .unwrap();
    println!("json load:  {:?}", t.elapsed());
    let t = Instant::now();
    let _tree = scan.tree().unwrap();
    println!("tree build: {:?}", t.elapsed());
    let c = Classifier::builtin();
    let t = Instant::now();
    let found = find_candidates(&scan, &c, &FakeResolver::new(), 1_800_000_000).unwrap();
    println!("candidates: {:?} (includes one tree build)", t.elapsed());
    let t = Instant::now();
    let _ = dc_rules::verify(found, &c, &dc_protected::FsResolver);
    println!("verify:     {:?}", t.elapsed());
}

#[test]
#[ignore]
fn pieces() {
    let c = Classifier::builtin();
    let path = std::env::var("DC_PERF_PATH")
        .unwrap_or_else(|_| r"C:\Users\Randel\Documents\proj1234\dir7\file33.txt".into());
    let path = path.as_str();
    let n = 200_000;
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(c.protected().check(path, &FakeResolver::new()));
    }
    println!("protected.check: {:?}/call", t.elapsed() / n);
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(dc_protected::normalize(path).ok());
    }
    println!("normalize:       {:?}/call", t.elapsed() / n);
    let facts = dc_rules::NodeFacts::default();
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(c.classify(path, &facts, &FakeResolver::new(), 0));
    }
    println!("classify total:  {:?}/call", t.elapsed() / n);
}

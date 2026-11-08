//! Benchmarks on a synthetic one-million-node tree. Run with `cargo bench -p dc-core`.
//! Not part of `cargo test`. This measures; it does not drive any optimization yet.
#![allow(missing_docs, clippy::unwrap_used)]

use criterion::{criterion_group, criterion_main, Criterion};
use dc_core::{query, Kind, Node, Tree};

const NODES: usize = 1_000_000;

/// A deterministic tree: every 10th node is a directory, parents spread over earlier dirs.
fn synthetic(n: usize) -> Tree {
    let mut tree = Tree::with_root(Node::new("root", Kind::Dir));
    let mut dirs: Vec<u32> = vec![0];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for i in 0..n {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let parent = dirs[(state >> 33) as usize % dirs.len()];
        let is_dir = i % 10 == 0;
        let mut node = Node::new(
            format!("n{i}.dat"),
            if is_dir { Kind::Dir } else { Kind::File },
        );
        if !is_dir {
            node.logical_size = state >> 44;
            node.size_on_disk = node.logical_size.next_multiple_of(4096);
            node.set_extension_from_name();
        }
        let id = tree.push(parent, node).unwrap();
        if is_dir {
            dirs.push(id);
        }
    }
    tree
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("tree_1m");
    group.sample_size(10);
    group.bench_function("build", |b| b.iter(|| synthetic(NODES)));
    let mut tree = synthetic(NODES);
    group.bench_function("aggregate", |b| b.iter(|| tree.aggregate()));
    tree.aggregate();
    group.bench_function("top_files_20", |b| b.iter(|| query::top_files(&tree, 20)));
    group.bench_function("top_dirs_20", |b| b.iter(|| query::top_dirs(&tree, 20)));
    group.bench_function("by_extension", |b| b.iter(|| query::by_extension(&tree)));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);

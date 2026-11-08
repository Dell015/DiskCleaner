//! Property tests: aggregation invariants hold for arbitrary trees.
#![allow(missing_docs)]

use dc_core::{Kind, Node, Tree};
use proptest::prelude::*;

/// Build a tree from (parent_pick, is_dir, size, counted) tuples. A parent pick is reduced to
/// an existing directory, so any input is a valid tree.
fn build(spec: &[(usize, bool, u32, bool)]) -> Tree {
    let mut tree = Tree::with_root(Node::new("root", Kind::Dir));
    let mut dirs: Vec<u32> = vec![0];
    for (i, &(pick, is_dir, size, counted)) in spec.iter().enumerate() {
        let parent = dirs[pick % dirs.len()];
        let mut node = Node::new(format!("n{i}"), if is_dir { Kind::Dir } else { Kind::File });
        node.logical_size = u64::from(size);
        node.size_on_disk = u64::from(size).next_multiple_of(4096);
        node.counted = counted || is_dir;
        let id = tree.push(parent, node).unwrap();
        if is_dir {
            dirs.push(id);
        }
    }
    tree.aggregate();
    tree
}

fn spec() -> impl Strategy<Value = Vec<(usize, bool, u32, bool)>> {
    prop::collection::vec(
        (
            any::<usize>(),
            any::<bool>(),
            0u32..1_000_000,
            any::<bool>(),
        ),
        0..200,
    )
}

proptest! {
    #[test]
    fn parent_total_is_sum_of_counted_children(spec in spec()) {
        let tree = build(&spec);
        for node in tree.nodes() {
            if node.kind != Kind::Dir { continue; }
            let sum: u64 = tree.nodes().iter()
                .filter(|c| c.parent == Some(node.id) && c.counted)
                .map(|c| c.agg.total_logical)
                .sum();
            prop_assert_eq!(node.agg.total_logical, sum);
        }
    }

    #[test]
    fn root_file_count_matches_counted_files(spec in spec()) {
        let tree = build(&spec);
        let expect = tree.nodes().iter().filter(|n| n.kind == Kind::File && n.counted).count() as u64;
        prop_assert_eq!(tree.root().unwrap().agg.file_count, expect);
    }

    #[test]
    fn ids_and_parents_are_valid_and_pruning_keeps_validity(spec in spec(), cut in 0u64..500_000) {
        let tree = build(&spec);
        prop_assert!(Tree::from_nodes(tree.nodes().to_vec()).is_ok());
        let small = tree.pruned(|n| n.kind == Kind::Dir || n.logical_size >= cut);
        prop_assert!(Tree::from_nodes(small.nodes().to_vec()).is_ok());
        prop_assert_eq!(small.root().unwrap().agg, tree.root().unwrap().agg);
    }

    #[test]
    fn aggregating_twice_changes_nothing(spec in spec()) {
        let mut tree = build(&spec);
        let before = tree.clone();
        tree.aggregate();
        prop_assert_eq!(tree, before);
    }
}

//! Read-only questions about a [`Tree`]: largest items, totals by type, oldest files.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::node::{Category, Kind, Node, NodeId};
use crate::tree::Tree;

/// Totals for one extension or category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeStat {
    /// Extension (`"mp4"`), category name, or `""` for files without an extension.
    pub name: String,
    /// Counted files.
    pub files: u64,
    /// Sum of logical sizes.
    pub logical: u64,
    /// Sum of sizes on disk.
    pub on_disk: u64,
}

fn counted_files(tree: &Tree) -> impl Iterator<Item = &Node> {
    tree.nodes()
        .iter()
        .filter(|n| n.kind == Kind::File && n.counted)
}

/// The `n` largest counted files by logical size (ties broken by id, so output is stable).
#[must_use]
pub fn top_files(tree: &Tree, n: usize) -> Vec<NodeId> {
    let mut files: Vec<&Node> = counted_files(tree).collect();
    files.sort_by(|a, b| b.logical_size.cmp(&a.logical_size).then(a.id.cmp(&b.id)));
    files.into_iter().take(n).map(|f| f.id).collect()
}

/// The `n` largest directories (the root excluded) by total logical size.
#[must_use]
pub fn top_dirs(tree: &Tree, n: usize) -> Vec<NodeId> {
    let mut dirs: Vec<&Node> = tree
        .nodes()
        .iter()
        .filter(|d| d.kind == Kind::Dir && d.parent.is_some())
        .collect();
    dirs.sort_by(|a, b| {
        b.agg
            .total_logical
            .cmp(&a.agg.total_logical)
            .then(a.id.cmp(&b.id))
    });
    dirs.into_iter().take(n).map(|d| d.id).collect()
}

/// Totals per extension, largest first.
#[must_use]
pub fn by_extension(tree: &Tree) -> Vec<TypeStat> {
    stats(counted_files(tree).map(|f| (f.ext.clone().unwrap_or_default(), f)))
}

/// Totals per category, largest first.
#[must_use]
pub fn by_category(tree: &Tree) -> Vec<TypeStat> {
    stats(counted_files(tree).map(|f| {
        let category = f.category.unwrap_or(Category::Other);
        (format!("{category:?}").to_lowercase(), f)
    }))
}

fn stats<'a>(items: impl Iterator<Item = (String, &'a Node)>) -> Vec<TypeStat> {
    let mut map: BTreeMap<String, TypeStat> = BTreeMap::new();
    for (name, file) in items {
        let slot = map.entry(name.clone()).or_insert_with(|| TypeStat {
            name,
            files: 0,
            logical: 0,
            on_disk: 0,
        });
        slot.files += 1;
        slot.logical = slot.logical.saturating_add(file.logical_size);
        slot.on_disk = slot.on_disk.saturating_add(file.size_on_disk);
    }
    let mut out: Vec<TypeStat> = map.into_values().collect();
    out.sort_by(|a, b| b.logical.cmp(&a.logical).then_with(|| a.name.cmp(&b.name)));
    out
}

/// The `n` counted files with the oldest modification time (files without one are skipped).
#[must_use]
pub fn oldest_files(tree: &Tree, n: usize) -> Vec<NodeId> {
    let mut files: Vec<&Node> = counted_files(tree).filter(|f| f.mtime.is_some()).collect();
    files.sort_by(|a, b| a.mtime.cmp(&b.mtime).then(a.id.cmp(&b.id)));
    files.into_iter().take(n).map(|f| f.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Timestamp;

    fn tree() -> Tree {
        let mut tree = Tree::with_root(Node::new("root", Kind::Dir));
        let d = tree.push(0, Node::new("d", Kind::Dir)).unwrap();
        for (parent, name, size, secs) in [
            (0, "a.mp4", 900, 50),
            (d, "b.mp4", 100, 10),
            (d, "c.txt", 300, 30),
            (0, "noext", 5, 20),
        ] {
            let mut f = Node::new(name, Kind::File);
            f.logical_size = size;
            f.size_on_disk = size;
            f.mtime = Some(Timestamp { secs, nanos: 0 });
            f.set_extension_from_name();
            tree.push(parent, f).unwrap();
        }
        tree.aggregate();
        tree
    }

    #[test]
    fn largest_files_and_dirs() {
        let t = tree();
        let names: Vec<_> = top_files(&t, 2)
            .iter()
            .map(|&i| t.get(i).unwrap().name.clone())
            .collect();
        assert_eq!(names, ["a.mp4", "c.txt"]);
        assert_eq!(top_dirs(&t, 5), vec![1]);
    }

    #[test]
    fn totals_by_extension_and_category() {
        let t = tree();
        let ext = by_extension(&t);
        assert_eq!(
            ext[0],
            TypeStat {
                name: "mp4".into(),
                files: 2,
                logical: 1000,
                on_disk: 1000
            }
        );
        assert!(ext.iter().any(|s| s.name.is_empty() && s.files == 1));
        let cat = by_category(&t);
        assert_eq!(cat[0].name, "video");
    }

    #[test]
    fn oldest_first() {
        let t = tree();
        let names: Vec<_> = oldest_files(&t, 2)
            .iter()
            .map(|&i| t.get(i).unwrap().name.clone())
            .collect();
        assert_eq!(names, ["b.mp4", "noext"]);
    }
}

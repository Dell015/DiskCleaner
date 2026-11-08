//! The arena tree and its aggregation.

use crate::error::CoreError;
use crate::node::{Kind, Node, NodeId};

/// An arena: nodes live in a `Vec`, and relationships are indices.
///
/// Invariant: a node's parent always has a smaller index, so children are always after their
/// parent. Aggregation relies on it and [`Tree::from_nodes`] checks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    /// A tree holding only the root (a directory).
    #[must_use]
    pub fn with_root(mut root: Node) -> Tree {
        root.id = 0;
        root.parent = None;
        root.depth = 0;
        Tree { nodes: vec![root] }
    }

    /// Rebuild a tree from stored nodes, validating the invariants.
    pub fn from_nodes(nodes: Vec<Node>) -> Result<Tree, CoreError> {
        let invalid = |why: String| CoreError::InvalidTree { reason: why };
        if nodes.is_empty() {
            return Err(invalid("no root node".to_owned()));
        }
        for (index, node) in nodes.iter().enumerate() {
            if usize::try_from(node.id).ok() != Some(index) {
                return Err(invalid(format!("node at {index} has id {}", node.id)));
            }
            match (index, node.parent) {
                (0, None) => {}
                (0, Some(_)) => return Err(invalid("root has a parent".to_owned())),
                (_, None) => return Err(invalid(format!("node {index} has no parent"))),
                (_, Some(parent)) => {
                    let parent_index = usize::try_from(parent).ok();
                    if parent_index.is_none_or(|p| p >= index) {
                        return Err(invalid(format!("node {index} has bad parent {parent}")));
                    }
                }
            }
        }
        Ok(Tree { nodes })
    }

    /// Add `node` under `parent`. The parent must exist and be a directory.
    pub fn push(&mut self, parent: NodeId, mut node: Node) -> Result<NodeId, CoreError> {
        let parent_node = self.get(parent).ok_or_else(|| CoreError::InvalidTree {
            reason: format!("no parent {parent}"),
        })?;
        if parent_node.kind != Kind::Dir {
            return Err(CoreError::InvalidTree {
                reason: format!("parent {parent} is not a dir"),
            });
        }
        let id = NodeId::try_from(self.nodes.len()).map_err(|_| CoreError::InvalidTree {
            reason: "too many nodes".to_owned(),
        })?;
        node.id = id;
        node.parent = Some(parent);
        node.depth = parent_node.depth.saturating_add(1);
        self.nodes.push(node);
        Ok(id)
    }

    /// All nodes, root first.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Consume the tree, returning its nodes.
    #[must_use]
    pub fn into_nodes(self) -> Vec<Node> {
        self.nodes
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always `false`: a tree has a root.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node with this id.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(usize::try_from(id).ok()?)
    }

    /// The root node.
    #[must_use]
    pub fn root(&self) -> Option<&Node> {
        self.nodes.first()
    }

    /// Mutable access to a node (used by the scanner while building).
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(usize::try_from(id).ok()?)
    }

    /// A display path: names joined with `/`, starting below the root.
    #[must_use]
    pub fn relative_path(&self, id: NodeId) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut current = self.get(id);
        while let Some(node) = current {
            match node.parent {
                Some(parent) => {
                    parts.push(&node.name);
                    current = self.get(parent);
                }
                None => break,
            }
        }
        parts.reverse();
        parts.join("/")
    }

    /// Recompute every node's totals, bottom-up. Duplicate hardlink paths (`counted == false`)
    /// contribute nothing to their parents.
    pub fn aggregate(&mut self) {
        for node in &mut self.nodes {
            node.agg = crate::node::Aggregate::default();
            if node.kind == Kind::File && node.counted {
                node.agg.total_logical = node.logical_size;
                node.agg.total_on_disk = node.size_on_disk;
                node.agg.file_count = 1;
            }
        }
        for index in (1..self.nodes.len()).rev() {
            let Some(child) = self.nodes.get(index) else {
                continue;
            };
            if !child.counted {
                continue;
            }
            let (agg, kind, parent) = (child.agg, child.kind, child.parent);
            let Some(parent) = parent.and_then(|p| self.get_mut(p)) else {
                continue;
            };
            parent.agg.total_logical = parent.agg.total_logical.saturating_add(agg.total_logical);
            parent.agg.total_on_disk = parent.agg.total_on_disk.saturating_add(agg.total_on_disk);
            parent.agg.file_count += agg.file_count;
            parent.agg.dir_count += agg.dir_count + u64::from(kind == Kind::Dir);
            parent.agg.link_count += agg.link_count + u64::from(kind == Kind::Link);
            parent.agg.child_count += 1;
        }
    }

    /// A smaller tree with only the nodes `keep` accepts (and whose parents were kept).
    ///
    /// Totals are copied as they are, so a directory still reports the size of everything that
    /// was scanned below it even when its children were dropped. The root is always kept.
    #[must_use]
    pub fn pruned(&self, keep: impl Fn(&Node) -> bool) -> Tree {
        let mut new_id: Vec<Option<NodeId>> = vec![None; self.nodes.len()];
        let mut out: Vec<Node> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let parent_new = match node.parent {
                None => None,
                Some(p) => match usize::try_from(p).ok().and_then(|p| new_id.get(p)).copied() {
                    Some(Some(mapped)) => Some(mapped),
                    _ => continue,
                },
            };
            if index != 0 && !keep(node) {
                continue;
            }
            let Ok(id) = NodeId::try_from(out.len()) else {
                break;
            };
            let mut copy = node.clone();
            copy.id = id;
            copy.parent = parent_new;
            out.push(copy);
            if let Some(slot) = new_id.get_mut(index) {
                *slot = Some(id);
            }
        }
        Tree { nodes: out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> Node {
        let mut node = Node::new(name, Kind::File);
        node.logical_size = size;
        node.size_on_disk = size.next_multiple_of(4096);
        node
    }

    fn sample() -> Tree {
        let mut tree = Tree::with_root(Node::new("root", Kind::Dir));
        let a = tree.push(0, Node::new("a", Kind::Dir)).unwrap();
        tree.push(a, file("x.bin", 100)).unwrap();
        tree.push(a, file("y.bin", 5000)).unwrap();
        tree.push(0, file("z.txt", 7)).unwrap();
        tree.push(0, Node::new("lnk", Kind::Link)).unwrap();
        tree.aggregate();
        tree
    }

    #[test]
    fn aggregates_sum_children() {
        let tree = sample();
        let root = tree.root().unwrap();
        assert_eq!(root.agg.total_logical, 5107);
        assert_eq!(root.agg.total_on_disk, 4096 + 8192 + 4096);
        assert_eq!(root.agg.file_count, 3);
        assert_eq!(root.agg.dir_count, 1);
        assert_eq!(root.agg.link_count, 1);
        assert_eq!(root.agg.child_count, 3);
        assert_eq!(tree.get(1).unwrap().agg.total_logical, 5100);
    }

    #[test]
    fn duplicate_hardlinks_are_not_counted() {
        let mut tree = sample();
        if let Some(node) = tree.get_mut(2) {
            node.counted = false;
            node.flags.hardlink = true;
        }
        tree.aggregate();
        assert_eq!(tree.root().unwrap().agg.total_logical, 5007);
        assert_eq!(tree.root().unwrap().agg.file_count, 2);
    }

    #[test]
    fn pruning_keeps_totals() {
        let tree = sample();
        let small = tree.pruned(|n| n.kind != Kind::File || n.logical_size >= 1000);
        assert_eq!(small.len(), tree.len() - 2);
        assert_eq!(small.root().unwrap().agg.total_logical, 5107);
        assert!(Tree::from_nodes(small.into_nodes()).is_ok());
    }

    #[test]
    fn pruning_a_directory_drops_its_children() {
        let tree = sample();
        let small = tree.pruned(|n| n.name != "a");
        assert!(small
            .nodes()
            .iter()
            .all(|n| n.name != "x.bin" && n.name != "y.bin"));
        assert_eq!(small.root().unwrap().agg.total_logical, 5107);
    }

    #[test]
    fn push_rejects_a_missing_or_non_directory_parent() {
        let mut tree = sample();
        assert!(tree.push(99, file("q", 1)).is_err());
        assert!(tree.push(2, file("q", 1)).is_err());
    }

    #[test]
    fn from_nodes_rejects_broken_trees() {
        assert!(Tree::from_nodes(vec![]).is_err());
        let mut nodes = sample().into_nodes();
        nodes[1].parent = Some(3);
        assert!(Tree::from_nodes(nodes).is_err());
    }

    #[test]
    fn relative_paths() {
        let tree = sample();
        assert_eq!(tree.relative_path(3), "a/y.bin");
        assert_eq!(tree.relative_path(0), "");
    }
}

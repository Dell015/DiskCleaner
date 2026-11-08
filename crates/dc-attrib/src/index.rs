//! A path index over the scan: the full comparison path of every node.

use std::collections::HashMap;

use dc_core::{Kind, Node, NodeId, Tree};
use dc_winmeta::path_key;

/// Paths of all nodes (lower case, backslashes) and a reverse lookup.
pub struct Index<'a> {
    pub tree: &'a Tree,
    root_display: String,
    keys: Vec<String>,
    by_key: HashMap<String, NodeId>,
    children: Vec<Vec<NodeId>>,
}

fn strip_verbatim(text: &str) -> &str {
    text.strip_prefix(r"\\?\").unwrap_or(text)
}

impl<'a> Index<'a> {
    /// Build the index. Nodes whose parent comes later in the list get no path and therefore
    /// match nothing (fail closed).
    pub fn new(tree: &'a Tree, root: &str) -> Index<'a> {
        let root_display = strip_verbatim(root.trim()).replace('/', "\\");
        let mut keys: Vec<String> = Vec::with_capacity(tree.len());
        let mut by_key = HashMap::with_capacity(tree.len());
        let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); tree.len()];
        for node in tree.nodes() {
            if let Some(slot) = node.parent.and_then(|p| children.get_mut(p as usize)) {
                slot.push(node.id);
            }
            let key = match node.parent {
                None => path_key(&root_display),
                Some(parent) => match keys.get(parent as usize) {
                    Some(parent_key) if !parent_key.is_empty() => {
                        let base = parent_key.trim_end_matches('\\');
                        format!("{base}\\{}", node.name.to_lowercase())
                    }
                    _ => String::new(),
                },
            };
            if !key.is_empty() {
                by_key.entry(key.clone()).or_insert(node.id);
            }
            keys.push(key);
        }
        Index {
            tree,
            root_display,
            keys,
            by_key,
            children,
        }
    }

    /// Direct children of a node.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.children.get(id as usize).map_or(&[], Vec::as_slice)
    }

    /// Comparison path of a node (empty if unknown).
    pub fn key(&self, id: NodeId) -> &str {
        self.keys.get(id as usize).map_or("", String::as_str)
    }

    /// Find a node by path text.
    pub fn find(&self, path: &str) -> Option<NodeId> {
        self.by_key
            .get(&path_key(strip_verbatim(path.trim())))
            .copied()
    }

    /// Display path (original case) of a node.
    pub fn display(&self, id: NodeId) -> String {
        let relative = self.tree.relative_path(id).replace('/', "\\");
        if relative.is_empty() {
            self.root_display.clone()
        } else {
            format!("{}\\{relative}", self.root_display.trim_end_matches('\\'))
        }
    }

    /// Directories in the scan.
    pub fn dirs(&self) -> impl Iterator<Item = &Node> {
        self.tree.nodes().iter().filter(|n| n.kind == Kind::Dir)
    }
}

/// What lies directly above a candidate folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// `...\AppData\Local|Roaming|LocalLow` of a user.
    UserData,
    /// `X:\ProgramData`.
    ProgramData,
    /// `X:\Program Files` or `X:\Program Files (x86)`.
    ProgramFiles,
    /// `X:\Program Files\Common Files` (and the x86 one): parts shared by programs.
    CommonFiles,
}

/// Classify a parent path key (for example `c:\users\bob\appdata\local`).
pub fn container_kind(parent_key: &str) -> Option<Container> {
    let rest = parent_key.get(2..)?; // drop "c:"
    match rest {
        r"\programdata" => return Some(Container::ProgramData),
        r"\program files" | r"\program files (x86)" => return Some(Container::ProgramFiles),
        r"\program files\common files" | r"\program files (x86)\common files" => {
            return Some(Container::CommonFiles)
        }
        _ => {}
    }
    let parts: Vec<&str> = rest.split('\\').collect();
    match parts.as_slice() {
        ["", "users", _user, "appdata", "local" | "roaming" | "locallow"] => {
            Some(Container::UserData)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containers_are_recognised_exactly() {
        assert_eq!(
            container_kind(r"c:\users\bob\appdata\local"),
            Some(Container::UserData)
        );
        assert_eq!(
            container_kind(r"d:\program files (x86)"),
            Some(Container::ProgramFiles)
        );
        assert_eq!(
            container_kind(r"c:\programdata"),
            Some(Container::ProgramData)
        );
        assert_eq!(container_kind(r"c:\users\bob\appdata\local\temp"), None);
        assert_eq!(container_kind(r"c:\users\bob"), None);
        assert_eq!(container_kind(""), None);
    }
}

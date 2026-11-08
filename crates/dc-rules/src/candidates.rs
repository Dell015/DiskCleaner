//! Walk a scan and list what the rules call `Safe` or `ProbablySafe`.
//!
//! Nothing here changes anything; a candidate is only a suggestion.

use std::collections::BTreeMap;

use dc_core::{CoreError, Kind, Node, NodeId, Risk, ScanResult, Tree};
use dc_protected::{FakeResolver, PathResolver};

use crate::classify::{Classification, Classifier, NodeFacts};

/// Sizes of a set of items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sizes {
    /// Sum of file lengths.
    pub logical: u64,
    /// Sum of space on disk.
    pub on_disk: u64,
    /// Number of files.
    pub files: u64,
}

impl Sizes {
    fn add(&mut self, other: Sizes) {
        self.logical = self.logical.saturating_add(other.logical);
        self.on_disk = self.on_disk.saturating_add(other.on_disk);
        self.files = self.files.saturating_add(other.files);
    }
}

/// Something the rules suggest for cleanup. Not an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The folder or file (for old files grouped under a folder, that folder).
    pub node: NodeId,
    /// Full path.
    pub path: String,
    /// `Safe` or `ProbablySafe`.
    pub risk: Risk,
    /// Id of the rule behind it.
    pub rule: String,
    /// Stable reason id.
    pub reason: String,
    /// Sentence for people.
    pub summary: String,
    /// What would be freed.
    pub sizes: Sizes,
    /// True when the candidate is only some of the files inside the folder (the old ones, or
    /// the ones left after a blocked part was set aside), not the whole folder.
    pub partial: bool,
}

/// A folder a rule matched but that must not be offered, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    /// The folder.
    pub path: String,
    /// The rule that matched.
    pub rule: String,
    /// What inside it stops the offer.
    pub because: String,
}

/// The result of looking through a scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    /// Candidates, biggest first.
    pub items: Vec<Candidate>,
    /// Matches that were refused because of what they contain.
    pub blocked: Vec<Blocked>,
    /// Totals of the `Safe` candidates.
    pub safe: Sizes,
    /// Totals of the `ProbablySafe` candidates.
    pub probably_safe: Sizes,
}

fn strip_verbatim(text: &str) -> &str {
    text.strip_prefix(r"\\?\").unwrap_or(text)
}

fn join(parent: &str, name: &str) -> String {
    format!("{}\\{name}", parent.trim_end_matches('\\'))
}

fn facts_of(node: &Node) -> NodeFacts {
    NodeFacts {
        is_dir: node.kind == Kind::Dir,
        mtime_secs: node.mtime.map(|t| t.secs),
        reparse: node.flags.reparse,
        cloud: node.flags.cloud,
        hardlink: node.flags.hardlink,
    }
}

/// The first thing inside `dir` that makes it unfit to offer, if any.
fn blocker_inside(
    tree: &Tree,
    children: &[Vec<NodeId>],
    dir: NodeId,
    dir_path: &str,
    classifier: &Classifier,
) -> Option<String> {
    let mut stack: Vec<(NodeId, String)> = vec![(dir, dir_path.to_owned())];
    while let Some((id, path)) = stack.pop() {
        for &child in children.get(id as usize).map_or(&[][..], Vec::as_slice) {
            let Some(node) = tree.get(child) else {
                continue;
            };
            let child_path = join(&path, &node.name);
            let why = if node.flags.reparse {
                Some("a link or junction")
            } else if node.flags.cloud {
                Some("a cloud placeholder file")
            } else if node.flags.hardlink {
                Some("a file with more than one name")
            } else if classifier
                .protected()
                .check(&child_path, &FakeResolver::new())
                .protected
            {
                Some("something protected")
            } else {
                None
            };
            if let Some(why) = why {
                return Some(format!("{why}: {child_path}"));
            }
            if node.kind == Kind::Dir {
                stack.push((child, child_path));
            }
        }
    }
    None
}

fn child_lists(tree: &Tree) -> Vec<Vec<NodeId>> {
    let mut children: Vec<Vec<NodeId>> = vec![Vec::new(); tree.len()];
    for node in tree.nodes() {
        if let Some(slot) = node.parent.and_then(|p| children.get_mut(p as usize)) {
            slot.push(node.id);
        }
    }
    children
}

fn pick_rule<'a>(
    classifier: &'a Classifier,
    c: &Classification,
) -> Option<&'a crate::pack::CompiledRule> {
    c.rules
        .iter()
        .filter_map(|id| classifier.rules().iter().find(|r| &r.rule.id == id))
        .find(|r| r.rule.risk == c.risk)
}

/// Look through a scan. `now` is the current time (seconds since the Unix epoch).
pub fn find_candidates(
    scan: &ScanResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
    now: i64,
) -> Result<Candidates, CoreError> {
    let tree = scan.tree()?;
    let children = child_lists(&tree);
    let root_text = strip_verbatim(scan.root.trim()).replace('/', "\\");

    let mut out = Candidates::default();
    // Old files grouped by (group folder, rule): sizes and the rule's details.
    let mut groups: BTreeMap<(NodeId, String), Candidate> = BTreeMap::new();
    let mut stack: Vec<(NodeId, String)> = tree
        .root()
        .map(|r| (r.id, root_text.clone()))
        .into_iter()
        .collect();

    while let Some((id, path)) = stack.pop() {
        let Some(node) = tree.get(id) else { continue };
        let facts = facts_of(node);
        let result = classifier.classify(&path, &facts, resolver, now);
        let offered = result.risk != Risk::DontTouch;
        if offered {
            if let Some(rule) = pick_rule(classifier, &result) {
                if facts.is_dir {
                    match blocker_inside(&tree, &children, id, &path, classifier) {
                        None => {
                            out.items.push(Candidate {
                                node: id,
                                path: path.clone(),
                                risk: result.risk,
                                rule: rule.rule.id.clone(),
                                reason: rule.rule.reason.clone(),
                                summary: rule.rule.summary.clone(),
                                sizes: Sizes {
                                    logical: node.agg.total_logical,
                                    on_disk: node.agg.total_on_disk,
                                    files: node.agg.file_count,
                                },
                                partial: false,
                            });
                            continue; // everything below is part of this item
                        }
                        Some(because) => out.blocked.push(Blocked {
                            path: path.clone(),
                            rule: rule.rule.id.clone(),
                            because,
                        }),
                    }
                } else if node.counted {
                    // Group old files under the folder their rule names.
                    let depth_of_pattern = rule.pattern.len();
                    let parts = path
                        .trim_start_matches(|c| c != '\\')
                        .split('\\')
                        .filter(|p| !p.is_empty())
                        .count();
                    let mut group = id;
                    for _ in 0..parts.saturating_sub(depth_of_pattern) {
                        if let Some(parent) = tree.get(group).and_then(|n| n.parent) {
                            group = parent;
                        }
                    }
                    let group_path = if group == id {
                        path.clone()
                    } else {
                        let mut p = path.clone();
                        for _ in 0..parts.saturating_sub(depth_of_pattern) {
                            if let Some(cut) = p.rfind('\\') {
                                p.truncate(cut);
                            }
                        }
                        p
                    };
                    let entry = groups
                        .entry((group, rule.rule.id.clone()))
                        .or_insert_with(|| Candidate {
                            node: group,
                            path: group_path,
                            risk: result.risk,
                            rule: rule.rule.id.clone(),
                            reason: rule.rule.reason.clone(),
                            summary: rule.rule.summary.clone(),
                            sizes: Sizes::default(),
                            partial: group != id || rule.rule.min_age_days.is_some(),
                        });
                    entry.sizes.add(Sizes {
                        logical: node.logical_size,
                        on_disk: node.size_on_disk,
                        files: 1,
                    });
                    continue;
                }
            }
        }
        if facts.is_dir {
            for &child in children.get(id as usize).map_or(&[][..], Vec::as_slice) {
                if let Some(c) = tree.get(child) {
                    stack.push((child, join(&path, &c.name)));
                }
            }
        }
    }

    out.items.extend(groups.into_values());
    out.items.sort_by(|a, b| {
        b.sizes
            .on_disk
            .cmp(&a.sizes.on_disk)
            .then(a.path.cmp(&b.path))
    });
    out.blocked.sort_by(|a, b| a.path.cmp(&b.path));
    for item in &out.items {
        match item.risk {
            Risk::Safe => out.safe.add(item.sizes),
            _ => out.probably_safe.add(item.sizes),
        }
    }
    Ok(out)
}

/// What the engine says about one path of the scan, step by step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The path as found in the scan.
    pub path: String,
    /// Whether it is a folder.
    pub is_dir: bool,
    /// The classification.
    pub classification: Classification,
    /// For a folder that a rule would offer: what inside stops it, if anything.
    pub blocker: Option<String>,
}

/// Explain one path of the scan (looked up in the scan, not on disk).
pub fn explain(
    scan: &ScanResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
    now: i64,
    wanted: &str,
) -> Result<Option<Explanation>, CoreError> {
    let tree = scan.tree()?;
    let children = child_lists(&tree);
    let root_text = strip_verbatim(scan.root.trim()).replace('/', "\\");
    let key = |p: &str| {
        strip_verbatim(p.trim())
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let wanted_key = key(wanted);
    let mut stack: Vec<(NodeId, String)> = tree
        .root()
        .map(|r| (r.id, root_text.clone()))
        .into_iter()
        .collect();
    while let Some((id, path)) = stack.pop() {
        let Some(node) = tree.get(id) else { continue };
        if key(&path) == wanted_key {
            let facts = facts_of(node);
            let classification = classifier.classify(&path, &facts, resolver, now);
            let blocker = (facts.is_dir && classification.risk != Risk::DontTouch)
                .then(|| blocker_inside(&tree, &children, id, &path, classifier))
                .flatten();
            return Ok(Some(Explanation {
                path,
                is_dir: facts.is_dir,
                classification,
                blocker,
            }));
        }
        if node.kind == Kind::Dir && wanted_key.starts_with(&key(&path)) {
            for &child in children.get(id as usize).map_or(&[][..], Vec::as_slice) {
                if let Some(c) = tree.get(child) {
                    stack.push((child, join(&path, &c.name)));
                }
            }
        }
    }
    Ok(None)
}

/// Check the candidates once more against the real file system: a candidate whose real
/// location differs from its path (a link somewhere above it), that cannot be resolved, or
/// whose real location is protected, is moved to `blocked`. Totals are recomputed.
pub fn verify(
    candidates: Candidates,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
) -> Candidates {
    use dc_protected::{normalize, Resolution};

    let mut out = Candidates {
        blocked: candidates.blocked,
        ..Candidates::default()
    };
    for item in candidates.items {
        let problem: Option<String> = match resolver.resolve(&item.path) {
            Resolution::NotFound => None,
            Resolution::Failed(why) => Some(format!("could not be resolved ({why})")),
            Resolution::Resolved(real) => {
                let same = match (normalize(&item.path), normalize(&real)) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                };
                if !same {
                    Some(format!(
                        "the real location is different ({real}): a link is in the way"
                    ))
                } else if classifier.protected().check(&item.path, resolver).protected {
                    Some("the real location is protected".to_owned())
                } else {
                    None
                }
            }
        };
        match problem {
            None => out.items.push(item),
            Some(because) => out.blocked.push(Blocked {
                path: item.path,
                rule: item.rule,
                because,
            }),
        }
    }
    for item in &out.items {
        match item.risk {
            Risk::Safe => out.safe.add(item.sizes),
            _ => out.probably_safe.add(item.sizes),
        }
    }
    out.blocked.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

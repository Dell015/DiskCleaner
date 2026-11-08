//! Walk a scan and list what the rules call `Safe` or `ProbablySafe`.
//!
//! Nothing here changes anything; a candidate is only a suggestion.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, VecDeque};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use dc_core::{CoreError, Kind, Node, NodeId, Risk, ScanResult, Tree};
use dc_protected::{FakeResolver, PathResolver};

use crate::classify::{Classification, Classifier, NodeFacts};

/// Sizes of a set of items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sizes {
    /// Sum of file lengths.
    pub logical: u64,
    /// Sum of space on disk.
    pub on_disk: u64,
    /// Number of files.
    pub files: u64,
}

impl Sizes {
    /// Add another set of sizes (saturating).
    pub fn add(&mut self, other: Sizes) {
        self.logical = self.logical.saturating_add(other.logical);
        self.on_disk = self.on_disk.saturating_add(other.on_disk);
        self.files = self.files.saturating_add(other.files);
    }
}

/// Something the rules suggest for cleanup. Not an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// How much work the search did (for finding slow steps; never part of a comparison).
#[derive(Debug, Clone, Copy, Default)]
pub struct Work {
    /// Items classified one by one.
    pub classified: u64,
    /// Time spent classifying them, in microseconds.
    pub classify_micros: u64,
    /// Items looked at while checking inside offered folders.
    pub inspected: u64,
    /// Time spent on that, in microseconds.
    pub inspect_micros: u64,
}

impl PartialEq for Work {
    fn eq(&self, _other: &Work) -> bool {
        true
    }
}

impl Eq for Work {}

/// The result of looking through a scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    /// How much work it took.
    pub work: Work,
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
    inspected: &mut u64,
) -> Option<String> {
    let mut stack: Vec<(NodeId, String)> = vec![(dir, dir_path.to_owned())];
    while let Some((id, path)) = stack.pop() {
        for &child in children.get(id as usize).map_or(&[][..], Vec::as_slice) {
            let Some(node) = tree.get(child) else {
                continue;
            };
            *inspected += 1;
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

/// The children of every node, indexed by node id.
pub fn child_lists(tree: &Tree) -> Vec<Vec<NodeId>> {
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

/// Keep only the outermost refusal of each rule: a refused folder inside another refused
/// folder of the same rule says nothing new. Input must be sorted by path.
fn topmost_blocked(sorted: Vec<Blocked>) -> Vec<Blocked> {
    let mut kept: Vec<Blocked> = Vec::new();
    let mut open: Vec<(String, String)> = Vec::new(); // (rule, lowercase path + "\\")
    for item in sorted {
        let lower = item.path.to_lowercase();
        let inside = open
            .iter()
            .any(|(rule, prefix)| *rule == item.rule && lower.starts_with(prefix.as_str()));
        if !inside {
            open.push((
                item.rule.clone(),
                format!("{}\\", lower.trim_end_matches('\\')),
            ));
            kept.push(item);
        }
    }
    kept
}

/// Everything one part of the walk found.
#[derive(Default)]
struct Partial {
    items: Vec<Candidate>,
    blocked: Vec<Blocked>,
    /// Old files grouped by (group folder, rule).
    groups: BTreeMap<(NodeId, String), Candidate>,
    work: Work,
}

impl Partial {
    /// Combine two parts. The result does not depend on which part comes first.
    fn merge(mut self, other: Partial) -> Partial {
        self.items.extend(other.items);
        self.blocked.extend(other.blocked);
        for (key, group) in other.groups {
            match self.groups.entry(key) {
                Entry::Vacant(slot) => {
                    slot.insert(group);
                }
                Entry::Occupied(mut slot) => {
                    let mine = slot.get_mut();
                    mine.sizes.add(group.sizes);
                    mine.partial |= group.partial;
                    // The more cautious label wins, whichever part came first.
                    mine.risk = Risk::escalate_all([mine.risk, group.risk]);
                }
            }
        }
        self.work.classified += other.work.classified;
        self.work.classify_micros += other.work.classify_micros;
        self.work.inspected += other.work.inspected;
        self.work.inspect_micros += other.work.inspect_micros;
        self
    }
}

/// What every step of the walk needs to know.
struct Ctx<'a> {
    tree: &'a Tree,
    children: &'a [Vec<NodeId>],
    classifier: &'a Classifier,
    resolver: &'a (dyn PathResolver + Sync),
    now: i64,
}

fn micros(since: std::time::Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Look at one item. Returns true if the walk should continue into its children.
fn visit(ctx: &Ctx<'_>, id: NodeId, path: &str, part: &mut Partial) -> bool {
    let Some(node) = ctx.tree.get(id) else {
        return false;
    };
    let facts = facts_of(node);
    let started = std::time::Instant::now();
    let result = ctx.classifier.classify(path, &facts, ctx.resolver, ctx.now);
    part.work.classified += 1;
    part.work.classify_micros += micros(started);
    if result.risk != Risk::DontTouch {
        if let Some(rule) = pick_rule(ctx.classifier, &result) {
            if facts.is_dir {
                let started = std::time::Instant::now();
                let inside = blocker_inside(
                    ctx.tree,
                    ctx.children,
                    id,
                    path,
                    ctx.classifier,
                    &mut part.work.inspected,
                );
                part.work.inspect_micros += micros(started);
                match inside {
                    None => {
                        part.items.push(Candidate {
                            node: id,
                            path: path.to_owned(),
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
                        return false; // everything below is part of this item
                    }
                    Some(because) => part.blocked.push(Blocked {
                        path: path.to_owned(),
                        rule: rule.rule.id.clone(),
                        because,
                    }),
                }
            } else if node.counted {
                // Group old files under the folder their rule names.
                let up = path
                    .trim_start_matches(|c| c != '\\')
                    .split('\\')
                    .filter(|p| !p.is_empty())
                    .count()
                    .saturating_sub(rule.pattern.len());
                let mut group = id;
                let mut group_path = path.to_owned();
                for _ in 0..up {
                    if let Some(parent) = ctx.tree.get(group).and_then(|n| n.parent) {
                        group = parent;
                    }
                    if let Some(cut) = group_path.rfind('\\') {
                        group_path.truncate(cut);
                    }
                }
                let entry = part
                    .groups
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
                entry.risk = Risk::escalate_all([entry.risk, result.risk]);
                entry.sizes.add(Sizes {
                    logical: node.logical_size,
                    on_disk: node.size_on_disk,
                    files: 1,
                });
                return false;
            }
        }
    }
    facts.is_dir
}

/// Walk one subtree depth-first into `part`.
fn walk(ctx: &Ctx<'_>, start: (NodeId, String), part: &mut Partial) {
    let mut stack = vec![start];
    while let Some((id, path)) = stack.pop() {
        if visit(ctx, id, &path, part) {
            push_children(ctx, id, &path, &mut |c| stack.push(c));
        }
    }
}

fn push_children(ctx: &Ctx<'_>, id: NodeId, path: &str, push: &mut dyn FnMut((NodeId, String))) {
    for &child in ctx.children.get(id as usize).map_or(&[][..], Vec::as_slice) {
        if let Some(c) = ctx.tree.get(child) {
            push((child, join(path, &c.name)));
        }
    }
}

/// The first few thousand items are walked in one go, breadth first, to get many separate
/// subtrees; those are then walked in parallel.
const SPLIT_AT: usize = 4096;

/// What inside `dir` (a folder of the scan, at `dir_path`) makes it unfit to offer: a link, a
/// cloud placeholder, a file with more than one name, or something protected. `None` if
/// nothing does. Used by B4b candidates and B5 leftovers, so both apply the same test.
pub fn folder_blocker(
    tree: &Tree,
    children: &[Vec<NodeId>],
    dir: NodeId,
    dir_path: &str,
    classifier: &Classifier,
) -> Option<String> {
    blocker_inside(tree, children, dir, dir_path, classifier, &mut 0)
}

/// Look through a scan. `now` is the current time (seconds since the Unix epoch).
///
/// Separate subtrees are searched on several threads; the answer does not depend on how the
/// work was divided (the result is sorted, and merging is order-independent).
pub fn find_candidates(
    scan: &ScanResult,
    classifier: &Classifier,
    resolver: &(dyn PathResolver + Sync),
    now: i64,
) -> Result<Candidates, CoreError> {
    find_candidates_split(scan, classifier, resolver, now, SPLIT_AT)
}

/// Like [`find_candidates`], with the point at which the work is divided chosen by the caller:
/// `1` divides at once (as much parallel work as possible), `usize::MAX` never divides
/// (a plain single-threaded walk). All values give the same answer; tests rely on that.
#[doc(hidden)]
pub fn find_candidates_split(
    scan: &ScanResult,
    classifier: &Classifier,
    resolver: &(dyn PathResolver + Sync),
    now: i64,
    split_at: usize,
) -> Result<Candidates, CoreError> {
    let tree = scan.tree()?;
    let children = child_lists(&tree);
    let root_text = strip_verbatim(scan.root.trim()).replace('/', "\\");
    let ctx = Ctx {
        tree: &tree,
        children: &children,
        classifier,
        resolver,
        now,
    };

    let mut first = Partial::default();
    let mut queue: VecDeque<(NodeId, String)> = tree
        .root()
        .map(|r| (r.id, root_text.clone()))
        .into_iter()
        .collect();
    while queue.len() < split_at {
        let Some((id, path)) = queue.pop_front() else {
            break;
        };
        if visit(&ctx, id, &path, &mut first) {
            push_children(&ctx, id, &path, &mut |c| queue.push_back(c));
        }
    }
    let rest: Vec<(NodeId, String)> = queue.into_iter().collect();
    let found = rest
        .into_par_iter()
        .map(|start| {
            let mut part = Partial::default();
            walk(&ctx, start, &mut part);
            part
        })
        .reduce(Partial::default, Partial::merge);
    let part = first.merge(found);

    let mut out = Candidates {
        work: part.work,
        items: part.items,
        blocked: part.blocked,
        ..Candidates::default()
    };
    out.items.extend(part.groups.into_values());
    out.items.sort_by(|a, b| {
        b.sizes
            .on_disk
            .cmp(&a.sizes.on_disk)
            .then(a.path.cmp(&b.path))
    });
    out.blocked.sort_by(|a, b| a.path.cmp(&b.path));
    out.blocked = topmost_blocked(std::mem::take(&mut out.blocked));
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
                .then(|| blocker_inside(&tree, &children, id, &path, classifier, &mut 0))
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
        work: candidates.work,
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

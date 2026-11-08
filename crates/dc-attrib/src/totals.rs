//! Exclusive totals: every byte of the scan lands in exactly one bucket.

use std::collections::{BTreeMap, HashMap};

use dc_core::{Kind, NodeId};

use crate::index::Index;
use crate::model::{Assignment, OwnerId, OwnerTotals, Sizes, UnattributedDir};

pub struct Totals {
    pub per_owner: Vec<OwnerTotals>,
    pub shared: Sizes,
    pub unattributed: Sizes,
    pub top_unattributed: Vec<UnattributedDir>,
}

fn sizes_of(index: &Index<'_>, id: NodeId) -> Sizes {
    index.tree.get(id).map_or_else(Sizes::default, |n| Sizes {
        logical: n.agg.total_logical,
        on_disk: n.agg.total_on_disk,
        files: n.agg.file_count,
    })
}

fn sub(a: Sizes, b: Sizes) -> Sizes {
    Sizes {
        logical: a.logical.saturating_sub(b.logical),
        on_disk: a.on_disk.saturating_sub(b.on_disk),
        files: a.files.saturating_sub(b.files),
    }
}

fn add(a: &mut Sizes, b: Sizes) {
    a.logical = a.logical.saturating_add(b.logical);
    a.on_disk = a.on_disk.saturating_add(b.on_disk);
    a.files = a.files.saturating_add(b.files);
}

pub fn compute(index: &Index<'_>, assignments: &[Assignment], top_n: usize) -> Totals {
    let nodes = index.tree.nodes();
    let explicit: HashMap<NodeId, &Assignment> = assignments.iter().map(|a| (a.node, a)).collect();

    // Nearest explicit ancestor-or-self of every node (parents come before children).
    let mut nearest: Vec<Option<NodeId>> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let own = explicit.contains_key(&node.id).then_some(node.id);
        let inherited = node
            .parent
            .and_then(|p| nearest.get(p as usize).copied().flatten());
        nearest.push(own.or(inherited));
    }
    let nearest_of = |id: NodeId| nearest.get(id as usize).copied().flatten();

    // Sum of the totals of the nearest explicit descendants, for every node (bottom-up).
    let mut explicit_below: Vec<Sizes> = vec![Sizes::default(); nodes.len()];
    for node in nodes.iter().rev() {
        let Some(parent) = node.parent else { continue };
        let contribution = if explicit.contains_key(&node.id) {
            sizes_of(index, node.id)
        } else {
            explicit_below
                .get(node.id as usize)
                .copied()
                .unwrap_or_default()
        };
        if let Some(slot) = explicit_below.get_mut(parent as usize) {
            add(slot, contribution);
        }
    }

    let mut own: BTreeMap<OwnerId, Sizes> = BTreeMap::new();
    let mut footprint: BTreeMap<OwnerId, u64> = BTreeMap::new();
    let mut folders: BTreeMap<OwnerId, u32> = BTreeMap::new();
    let mut shared = Sizes::default();
    let mut unattributed = Sizes::default();

    // Explicit nodes keep their total minus what deeper explicit nodes own.
    for assignment in assignments {
        let id = assignment.node;
        let credit = sub(
            sizes_of(index, id),
            explicit_below.get(id as usize).copied().unwrap_or_default(),
        );
        match assignment.owner {
            Some(owner) => {
                add(own.entry(owner).or_default(), credit);
                *folders.entry(owner).or_insert(0) += 1;
                let parent_owner = index
                    .tree
                    .get(id)
                    .and_then(|n| n.parent)
                    .and_then(nearest_of)
                    .and_then(|p| explicit.get(&p))
                    .and_then(|a| a.owner);
                if parent_owner != Some(owner) {
                    let slot = footprint.entry(owner).or_insert(0);
                    *slot = slot.saturating_add(sizes_of(index, id).on_disk);
                }
            }
            None => add(&mut shared, credit),
        }
    }
    // The part of the scan above all explicit nodes (including the root) is unattributed.
    if let Some(root) = nodes.first() {
        if !explicit.contains_key(&root.id) {
            add(
                &mut unattributed,
                sub(
                    sizes_of(index, root.id),
                    explicit_below
                        .get(root.id as usize)
                        .copied()
                        .unwrap_or_default(),
                ),
            );
        }
    }
    // Unowned parts nested inside explicit nodes are not unattributed: they inherit the owner.
    // Unattributed folders further down (children of unattributed folders) are already included
    // in the root figure above because they have no explicit node.

    // Biggest unowned folders. Skip a folder when one unowned child holds >= 80% of it.
    let mut unowned: HashMap<NodeId, u64> = HashMap::new();
    for node in nodes.iter().filter(|n| n.kind == Kind::Dir && n.depth >= 1) {
        if nearest_of(node.id).is_none() {
            let below = explicit_below
                .get(node.id as usize)
                .map_or(0, |s| s.on_disk);
            unowned.insert(node.id, node.agg.total_on_disk.saturating_sub(below));
        }
    }
    let mut child_max: HashMap<NodeId, u64> = HashMap::new();
    for (&id, &value) in &unowned {
        if let Some(parent) = index.tree.get(id).and_then(|n| n.parent) {
            let slot = child_max.entry(parent).or_insert(0);
            *slot = (*slot).max(value);
        }
    }
    let mut top: Vec<(NodeId, u64)> = unowned
        .iter()
        .filter(|(id, value)| {
            **value > 0
                && child_max.get(id).copied().unwrap_or(0).saturating_mul(5)
                    < value.saturating_mul(4)
        })
        .map(|(id, value)| (*id, *value))
        .collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    // Show the outermost unowned folders only; a folder is never listed again through a
    // descendant (parents are at least as big as children, so they come first).
    let mut kept: Vec<(NodeId, u64)> = Vec::new();
    let mut kept_ids: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    for (id, value) in top {
        let mut ancestor = index.tree.get(id).and_then(|n| n.parent);
        let mut covered = false;
        while let Some(a) = ancestor {
            if kept_ids.contains(&a) {
                covered = true;
                break;
            }
            ancestor = index.tree.get(a).and_then(|n| n.parent);
        }
        if !covered {
            kept_ids.insert(id);
            kept.push((id, value));
            if kept.len() >= top_n {
                break;
            }
        }
    }
    let top = kept;

    let per_owner = own
        .into_iter()
        .map(|(owner, sizes)| OwnerTotals {
            owner,
            own: sizes,
            footprint_on_disk: footprint.get(&owner).copied().unwrap_or(0),
            folders: folders.get(&owner).copied().unwrap_or(0),
        })
        .collect();
    Totals {
        per_owner,
        shared,
        unattributed,
        top_unattributed: top
            .into_iter()
            .map(|(node, on_disk)| UnattributedDir {
                node,
                path: index.display(node),
                on_disk,
            })
            .collect(),
    }
}

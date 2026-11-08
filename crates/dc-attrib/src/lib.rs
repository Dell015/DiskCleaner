//! Owner attribution: who does this folder belong to, and why do we think so?
//!
//! **Attribution is information, never permission.** Nothing here changes a `Risk`; a folder
//! with a confident owner is still `DontTouch` until the rules engine says otherwise. When
//! evidence is thin or conflicting the result gets *less* certain, never more.
//!
//! This crate only reads the scan and the app snapshot. It never touches the disk.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod index;
mod model;
mod names;
mod resolve;
mod rules;
mod totals;

pub use model::{
    Assignment, AttribResult, Confidence, Evidence, EvidenceKind, Owner, OwnerId, OwnerKind,
    OwnerTotals, ProgramFinding, Sizes, UnattributedDir, SCHEMA_VERSION,
};
pub use names::{normalize, same_name};

use dc_core::{CoreError, NodeId, ScanResult};
use dc_winmeta::{AppSnapshot, FakeSource, SystemInfoSource};

/// How many of the biggest unowned folders are kept.
pub const TOP_UNATTRIBUTED: usize = 50;

/// What we know about one path, for `--explain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The folder or file that was asked about.
    pub path: String,
    /// The node that carries the assignment (the folder itself or its nearest owned ancestor).
    pub assigned_node: Option<NodeId>,
    /// True if the assignment is inherited from an ancestor.
    pub inherited: bool,
    /// The assignment, if any.
    pub assignment: Option<Assignment>,
    /// Display path of the node that carries the assignment.
    pub assigned_path: Option<String>,
}

/// Work out ownership for a scan, without looking inside programs.
pub fn attribute(scan: &ScanResult, snapshot: &AppSnapshot) -> Result<AttribResult, CoreError> {
    attribute_with(scan, snapshot, &FakeSource::new())
}

/// Work out ownership for a scan. `source` is asked for the version resource of programs found
/// inside folders (read-only; programs are never run).
pub fn attribute_with(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    source: &dyn SystemInfoSource,
) -> Result<AttribResult, CoreError> {
    let tree = scan.tree()?;
    let index = index::Index::new(&tree, &scan.root);
    let gathered = rules::gather(&index, snapshot, source);
    let assignments = resolve::resolve(&gathered.claims);
    let totals = totals::compute(&index, &assignments, TOP_UNATTRIBUTED);
    Ok(AttribResult {
        schema_version: SCHEMA_VERSION,
        scan_root: scan.root.clone(),
        scan_total_on_disk: tree.root().map_or(0, |r| r.agg.total_on_disk),
        owners: gathered.owners,
        assignments,
        totals: totals.per_owner,
        shared: totals.shared,
        unattributed: totals.unattributed,
        top_unattributed: totals.top_unattributed,
        programs: gathered.programs,
    })
}

/// Explain the ownership of one path inside the scan (looked up in the scan, not on disk).
pub fn explain(
    scan: &ScanResult,
    result: &AttribResult,
    path: &str,
) -> Result<Option<Explanation>, CoreError> {
    let tree = scan.tree()?;
    let index = index::Index::new(&tree, &scan.root);
    let Some(mut node) = index.find(path) else {
        return Ok(None);
    };
    let by_node: std::collections::HashMap<NodeId, &Assignment> =
        result.assignments.iter().map(|a| (a.node, a)).collect();
    let start = node;
    loop {
        if let Some(assignment) = by_node.get(&node) {
            return Ok(Some(Explanation {
                path: index.display(start),
                assigned_node: Some(node),
                inherited: node != start,
                assignment: Some((*assignment).clone()),
                assigned_path: Some(index.display(node)),
            }));
        }
        match tree.get(node).and_then(|n| n.parent) {
            Some(parent) => node = parent,
            None => break,
        }
    }
    Ok(Some(Explanation {
        path: index.display(start),
        assigned_node: None,
        inherited: false,
        assignment: None,
        assigned_path: None,
    }))
}

/// Is the path protected by the built-in list? (Lexical check only.)
pub fn protected_marker(path: &str) -> bool {
    dc_protected::ProtectedList::builtin()
        .check(path, &dc_protected::FakeResolver::new())
        .protected
}

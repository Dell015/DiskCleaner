//! Leftover detection: finds data left behind by uninstalled apps.
//!
//! **A finding is information, never permission.** Nothing here deletes, moves or changes
//! anything, and the registry is never touched. A folder is only called a leftover when there
//! is positive evidence (a registration whose program folder is gone, a backup-style name, a
//! well-known app that is not installed, programs nobody registered); a folder that merely has
//! no owner is "unowned", not a leftover. Findings are never `Safe`: the best they can be is
//! `ProbablySafe`, and anything that may hold personal data, or that is only weakly supported,
//! is `DontTouch`. The protected list is consulted first and cannot be overridden.
//!
//! The crate reads the scan, the app snapshot and the attribution. It never touches the disk
//! except through the [`PathResolver`] given to it (read-only).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod analyze;
pub mod facts;
pub mod known;
mod model;

pub use known::{KnownApp, KnownApps};
pub use model::{
    Confidence, Evidence, EvidenceKind, Explanation, Finding, Kind, Leftovers, Refused, StaleApp,
    Standing, SCHEMA_VERSION,
};

use dc_attrib::AttribResult;
use dc_core::{CoreError, Risk, ScanResult};
use dc_protected::{normalize as normalize_path, PathResolver, Resolution};
use dc_rules::Classifier;
use dc_winmeta::AppSnapshot;

/// Look for leftovers, with the built-in list of well-known apps.
pub fn find_leftovers(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    attribution: &AttribResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
) -> Result<Leftovers, CoreError> {
    find_leftovers_with(
        scan,
        snapshot,
        attribution,
        classifier,
        resolver,
        &KnownApps::builtin(),
    )
}

/// Look for leftovers with a given list of well-known apps (use [`KnownApps::none`] for none).
pub fn find_leftovers_with(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    attribution: &AttribResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
    known: &KnownApps,
) -> Result<Leftovers, CoreError> {
    Ok(analyze::analyze(scan, snapshot, attribution, classifier, resolver, known)?.leftovers)
}

/// Second look at the real file system: a finding whose real location differs from its path
/// (a link somewhere above it), that cannot be resolved, or whose real location is protected is
/// moved to `refused`. Totals are recomputed.
pub fn verify(found: Leftovers, classifier: &Classifier, resolver: &dyn PathResolver) -> Leftovers {
    let findings = found.findings;
    let mut out = Leftovers {
        findings: Vec::new(),
        refused: found.refused,
        stale_apps: found.stale_apps,
        scan_root: found.scan_root,
        ..found
    };
    for finding in findings {
        let path = finding.candidate.path.clone();
        let problem: Option<String> = match resolver.resolve(&path) {
            Resolution::NotFound => None,
            Resolution::Failed(why) => Some(format!("it could not be resolved ({why})")),
            Resolution::Resolved(real) => {
                let same = match (normalize_path(&path), normalize_path(&real)) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                };
                if !same {
                    Some(format!(
                        "its real location is different ({real}): a link is in the way"
                    ))
                } else if classifier.protected().check(&path, resolver).protected {
                    Some("its real location is protected".to_owned())
                } else {
                    None
                }
            }
        };
        match problem {
            None => out.findings.push(finding),
            Some(because) => out.refused.push(Refused {
                path,
                kind: finding.kind,
                because,
            }),
        }
    }
    out.refused.sort_by(|a, b| a.path.cmp(&b.path));
    let (offered, reported_only) = analyze::totals(&out.findings);
    out.offered = offered;
    out.reported_only = reported_only;
    out
}

/// Combine the results of several scans (for example `C:\Users\me`, `C:\ProgramData` and
/// `C:\Program Files`, scanned separately). The same folder found twice is kept once, and the
/// order and totals are recomputed, so the answer does not depend on the order of the parts.
#[must_use]
pub fn merge(parts: Vec<Leftovers>) -> Leftovers {
    let mut roots: Vec<String> = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    let mut refused: Vec<Refused> = Vec::new();
    let mut stale: Vec<StaleApp> = Vec::new();
    let mut examined: u32 = 0;
    for part in parts {
        roots.push(part.scan_root);
        findings.extend(part.findings);
        refused.extend(part.refused);
        stale.extend(part.stale_apps);
        examined = examined.saturating_add(part.examined);
    }
    roots.sort();
    findings.sort_by(|a, b| {
        a.candidate
            .risk
            .cmp(&b.candidate.risk)
            .then(b.candidate.sizes.on_disk.cmp(&a.candidate.sizes.on_disk))
            .then(a.candidate.path.cmp(&b.candidate.path))
    });
    findings.dedup_by(|a, b| key(&a.candidate.path) == key(&b.candidate.path));
    refused.sort_by(|a, b| a.path.cmp(&b.path));
    refused.dedup_by(|a, b| key(&a.path) == key(&b.path));
    stale.sort_by(|a, b| {
        (a.name.as_str(), a.registered_folder.as_str())
            .cmp(&(b.name.as_str(), b.registered_folder.as_str()))
    });
    stale.dedup();
    let (offered, reported_only) = analyze::totals(&findings);
    Leftovers {
        schema_version: SCHEMA_VERSION,
        scan_root: roots.join("; "),
        findings,
        refused,
        stale_apps: stale,
        offered,
        reported_only,
        examined,
    }
}

fn key(path: &str) -> String {
    path.trim()
        .strip_prefix(r"\\?\")
        .unwrap_or(path.trim())
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Say what happened to one path of the scan (looked up in the scan, not on disk).
pub fn explain(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    attribution: &AttribResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
    wanted: &str,
) -> Result<Explanation, CoreError> {
    let known = KnownApps::builtin();
    let analysis = analyze::analyze(scan, snapshot, attribution, classifier, resolver, &known)?;
    let wanted_key = key(wanted);
    let found = &analysis.leftovers;
    let mut notes: Vec<String> = Vec::new();

    if let Some(f) = found
        .findings
        .iter()
        .find(|f| key(&f.candidate.path) == wanted_key)
    {
        notes.push(format!(
            "main kind: {}; score {} of 100 gives {:?} confidence",
            f.kind.id(),
            f.points,
            f.confidence
        ));
        for e in &f.evidence {
            notes.push(format!("evidence (+{}): {}", e.points, e.detail));
        }
        if !f.personal_data.is_empty() {
            notes.push(format!(
                "may hold personal data: {}",
                f.personal_data.join(", ")
            ));
        }
        if f.report_only_app {
            notes.push(
                "the well-known-app list marks this app as one that can hold games or wanted data"
                    .to_owned(),
            );
        }
        notes.push(format!("final risk: {}", f.candidate.risk.as_str()));
        return Ok(Explanation {
            path: f.candidate.path.clone(),
            standing: Standing::Finding,
            finding: Some(f.clone()),
            refused: None,
            notes,
        });
    }
    if let Some(r) = found.refused.iter().find(|r| key(&r.path) == wanted_key) {
        notes.push(format!("refused: {}", r.because));
        return Ok(Explanation {
            path: r.path.clone(),
            standing: Standing::Refused,
            finding: None,
            refused: Some(r.clone()),
            notes,
        });
    }
    if let Some((path, _)) = analysis.examined.iter().find(|(p, _)| *p == wanted_key) {
        notes.push("looked at, but no evidence that an app left it behind".to_owned());
        notes.push("a folder without an owner is only 'unowned', not a leftover".to_owned());
        return Ok(Explanation {
            path: path.clone(),
            standing: Standing::NoEvidence,
            finding: None,
            refused: None,
            notes,
        });
    }
    // Not examined: is it in the scan at all?
    let tree = scan.tree()?;
    let children = dc_rules::child_lists(&tree);
    let root_key = key(&analysis.ctx_root);
    let in_scan = wanted_key == root_key || {
        let rest = wanted_key
            .strip_prefix(&format!("{root_key}\\"))
            .unwrap_or("");
        let mut node = tree.root().map(|r| r.id);
        for part in rest.split('\\').filter(|p| !p.is_empty()) {
            node = node.and_then(|n| {
                children
                    .get(n as usize)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .copied()
                    .find(|&c| tree.get(c).is_some_and(|x| x.name.to_lowercase() == part))
            });
        }
        !rest.is_empty() && node.is_some()
    };
    if in_scan {
        notes.push(
            "only folders directly inside AppData (Local, Roaming, LocalLow), ProgramData, Program Files or Common Files are looked at"
                .to_owned(),
        );
        Ok(Explanation {
            path: wanted.to_owned(),
            standing: Standing::NotExamined,
            finding: None,
            refused: None,
            notes,
        })
    } else {
        notes.push("this path is not in the scan".to_owned());
        Ok(Explanation {
            path: String::new(),
            standing: Standing::NotInScan,
            finding: None,
            refused: None,
            notes,
        })
    }
}

/// True if no finding is `Safe` (always the case; used by tests as a guard).
#[must_use]
pub fn never_safe(found: &Leftovers) -> bool {
    found
        .findings
        .iter()
        .all(|f| f.candidate.risk != Risk::Safe)
}

//! The search itself: which folders show evidence of a removed app, and how strong it is.

use std::collections::{HashMap, HashSet};

use dc_attrib::{
    container_kind, normalize, Assignment, AttribResult, Confidence as AttribConfidence, Container,
    EvidenceKind as AttribEvidence, Owner, OwnerId, OwnerKind,
};
use dc_core::{CoreError, Kind as NodeKind, NodeId, Risk, ScanResult, Tree};
use dc_protected::PathResolver;
use dc_rules::{child_lists, folder_blocker, Candidate, Classifier, Sizes};
use dc_winmeta::{path_key, AppSnapshot, InstalledApp};

use crate::facts::{self, personal, Facts};
use crate::known::KnownApps;
use crate::model::{
    Confidence, Evidence, EvidenceKind, Finding, Kind, Leftovers, Refused, StaleApp, SCHEMA_VERSION,
};

/// Points for each kind of evidence (see the requirements, R3).
const STALE_REGISTRATION: u32 = 60;
const NAME_MATCH: u32 = 20;
const BACKUP_NAME: u32 = 50;
const KNOWN_APP: u32 = 60;
const UNREGISTERED_PROGRAMS: u32 = 30;
const UNTOUCHED_180: u32 = 20;
const UNTOUCHED_365: u32 = 30;
/// A change within this many days caps the confidence at Low.
const RECENT_DAYS: i64 = 30;
/// Highest score that is still Low.
const LOW_CAP: u32 = Confidence::MEDIUM_AT - 1;
const DAY: i64 = 86_400;

fn strip_verbatim(text: &str) -> &str {
    text.strip_prefix(r"\\?\").unwrap_or(text)
}

/// A folder directly inside a container.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Child {
    pub node: NodeId,
    pub parent: NodeId,
    pub container: Container,
}

/// Everything the search needs to look things up.
pub(crate) struct Ctx<'a> {
    pub tree: Tree,
    pub children: Vec<Vec<NodeId>>,
    pub root_text: String,
    pub facts: Facts,
    pub now: i64,
    pub owners: HashMap<OwnerId, &'a Owner>,
    pub assignments: HashMap<NodeId, &'a Assignment>,
    pub stale: HashMap<OwnerId, &'a InstalledApp>,
    pub installed_names: Vec<String>,
    pub known: &'a KnownApps,
    pub classifier: &'a Classifier,
    pub resolver: &'a dyn PathResolver,
}

/// What the search found. `examined` lists every folder that was looked at.
pub(crate) struct Analysis {
    pub leftovers: Leftovers,
    pub examined: Vec<(String, NodeId)>,
    pub ctx_root: String,
}

pub(crate) fn display_path(tree: &Tree, root_text: &str, id: NodeId) -> String {
    let relative = tree.relative_path(id).replace('/', "\\");
    if relative.is_empty() {
        root_text.to_owned()
    } else {
        format!("{}\\{relative}", root_text.trim_end_matches('\\'))
    }
}

/// Is the app something that can serve as evidence (R5)?
fn valid_app(app: &InstalledApp) -> bool {
    let f = &app.flags;
    !f.update_or_patch && !f.system_component && !f.hidden_entry && !app.name.trim().is_empty()
}

/// Is an install folder in a place where programs are installed?
fn in_program_area(key: &str) -> bool {
    let Some((parent, _)) = key.rsplit_once('\\') else {
        return false;
    };
    matches!(
        container_kind(parent),
        Some(Container::ProgramFiles | Container::UserData)
    ) || parent.ends_with(r"\appdata\local\programs")
}

/// Registered apps that count as evidence and whose program folder is gone.
fn stale_apps(snapshot: &AppSnapshot) -> Vec<&InstalledApp> {
    let keyed: Vec<(&InstalledApp, String)> = snapshot
        .apps
        .iter()
        .filter(|a| valid_app(a))
        .filter_map(|a| a.install_folder.as_deref().map(|f| (a, path_key(f))))
        .collect();
    keyed
        .iter()
        .filter(|(app, key)| {
            app.flags.stale_registration
                && in_program_area(key)
                && !keyed.iter().any(|(other, other_key)| {
                    !std::ptr::eq(*other, *app)
                        && key.starts_with(&format!("{}\\", other_key.trim_end_matches('\\')))
                })
        })
        .map(|(app, _)| *app)
        .collect()
}

/// Is a path possibly on the way to a container? Used to keep the walk small.
fn on_path_to_container(key: &str) -> bool {
    let parts: Vec<&str> = key.split('\\').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [_] | [_, "users"] | [_, "users", _] | [_, "users", _, "appdata"] => true,
        [_, "users", _, "appdata", "local" | "roaming" | "locallow"] => true,
        [_, "programdata"] => true,
        [_, pf] | [_, pf, "common files"] => pf.starts_with("program files"),
        _ => false,
    }
}

/// All folders directly inside a container, found by a short walk from the scan root.
fn container_children(tree: &Tree, children: &[Vec<NodeId>], root_text: &str) -> Vec<Child> {
    let mut found = Vec::new();
    let Some(root) = tree.root() else {
        return found;
    };
    let mut stack: Vec<(NodeId, String)> = vec![(root.id, path_key(root_text))];
    while let Some((id, key)) = stack.pop() {
        let container = container_kind(&key);
        for &child in children.get(id as usize).map_or(&[][..], Vec::as_slice) {
            let Some(node) = tree.get(child) else {
                continue;
            };
            if node.kind != NodeKind::Dir {
                continue;
            }
            if let Some(container) = container {
                found.push(Child {
                    node: child,
                    parent: id,
                    container,
                });
            }
            let child_key = format!(
                "{}\\{}",
                key.trim_end_matches('\\'),
                node.name.to_lowercase()
            );
            if on_path_to_container(&child_key) {
                stack.push((child, child_key));
            }
        }
    }
    found.sort_by_key(|c| c.node);
    found
}

/// `Some(base)` for names like `base.old`, `base_bak`, `base (backup)`.
pub(crate) fn backup_base(lower_name: &str) -> Option<String> {
    const SUFFIXES: [&str; 10] = [
        ".old", ".bak", ".backup", "_old", "-old", "_bak", "-bak", "_backup", "-backup", " (old)",
    ];
    let mut candidates: Vec<&str> = SUFFIXES.to_vec();
    candidates.push(" (backup)");
    for suffix in candidates {
        if let Some(base) = lower_name.strip_suffix(suffix) {
            let base = base.trim();
            if base.chars().count() >= 3 {
                return Some(base.to_owned());
            }
        }
    }
    None
}

fn is_microsoft(text: &str) -> bool {
    let n = normalize(text);
    n == "windows" || n == "microsoft" || n.starts_with("microsoft ") || n.starts_with("windows ")
}

/// Folders that belong to Windows or Microsoft and are never leftovers.
fn is_os_area(lower_name: &str) -> bool {
    lower_name.starts_with("windows")
        || lower_name.starts_with("microsoft")
        || matches!(
            lower_name,
            "common files"
                | "dotnet"
                | "msbuild"
                | "reference assemblies"
                | "modifiablewindowsapps"
                | "installshield installation information"
                | "uninstall information"
                | "internet explorer"
                | "wincert"
                | "packages"
        )
}

enum Outcome {
    NoEvidence,
    Finding(Box<Finding>),
    Refused(Refused),
}

fn push(evidence: &mut Vec<Evidence>, kind: EvidenceKind, points: u32, detail: String) {
    evidence.push(Evidence {
        kind,
        points,
        detail,
    });
}

fn examine(ctx: &Ctx<'_>, child: Child, siblings: &HashMap<(NodeId, String), NodeId>) -> Outcome {
    let Some(node) = ctx.tree.get(child.node) else {
        return Outcome::NoEvidence;
    };
    let id = child.node;
    let name = node.name.clone();
    let lower = name.to_lowercase();
    let assignment = ctx.assignments.get(&id).copied();
    let facts = &ctx.facts;
    let programs = facts.programs.get(id as usize).copied().unwrap_or(false);

    let mut evidence: Vec<Evidence> = Vec::new();
    let mut by_kind: Vec<(Kind, u32)> = Vec::new();
    let mut app: Option<String> = None;
    let mut report_only_app = false;

    // 1. Data of an app that is registered but whose program folder is gone.
    if matches!(
        child.container,
        Container::UserData | Container::ProgramData
    ) {
        let stale = assignment
            .filter(|a| {
                matches!(
                    a.confidence,
                    AttribConfidence::Medium | AttribConfidence::High
                )
            })
            .and_then(|a| a.owner.map(|o| (a, o)))
            .and_then(|(a, o)| ctx.stale.get(&o).map(|app| (a, *app)));
        if let Some((a, stale_app)) = stale {
            let folder = stale_app.install_folder.clone().unwrap_or_default();
            push(
                &mut evidence,
                EvidenceKind::StaleRegistration,
                STALE_REGISTRATION,
                format!(
                    "{} is still registered, but its program folder {folder} does not exist",
                    stale_app.name
                ),
            );
            let mut points = STALE_REGISTRATION;
            if a.evidence
                .iter()
                .any(|e| e.kind == AttribEvidence::NameMatch)
            {
                push(
                    &mut evidence,
                    EvidenceKind::NameMatch,
                    NAME_MATCH,
                    format!("the folder is named exactly like {}", stale_app.name),
                );
                points += NAME_MATCH;
            }
            by_kind.push((Kind::MissingApp, points));
            app = Some(stale_app.name.clone());
        }
    }

    // 2. A backup-style name next to the folder it was made from.
    if let Some(base) = backup_base(&lower) {
        if let Some(&sibling) = siblings.get(&(child.parent, base)) {
            if sibling != id {
                let sibling_name = ctx
                    .tree
                    .get(sibling)
                    .map_or_else(String::new, |n| n.name.clone());
                push(
                    &mut evidence,
                    EvidenceKind::BackupName,
                    BACKUP_NAME,
                    format!("named like a backup of {sibling_name}, which is still here"),
                );
                by_kind.push((Kind::BackupCopy, BACKUP_NAME));
                app.get_or_insert(sibling_name);
            }
        }
    }

    // 3. Named like a well-known app that is not installed any more.
    if assignment.is_none() {
        if let Some(known) = ctx.known.for_folder(&name) {
            if !ctx.known.is_installed(known, &ctx.installed_names) {
                push(
                    &mut evidence,
                    EvidenceKind::KnownApp,
                    KNOWN_APP,
                    format!("named like {}, which is not installed any more", known.name),
                );
                by_kind.push((Kind::KnownAppRemoved, KNOWN_APP));
                app.get_or_insert_with(|| known.name.clone());
                report_only_app = known.report_only;
            }
        }
    }

    // 4. Programs in Program Files that no installed app claims.
    if matches!(
        child.container,
        Container::ProgramFiles | Container::CommonFiles
    ) && assignment.is_none()
        && programs
        && !is_os_area(&lower)
    {
        push(
            &mut evidence,
            EvidenceKind::UnregisteredPrograms,
            UNREGISTERED_PROGRAMS,
            "contains programs, but no installed app claims this folder".to_owned(),
        );
        by_kind.push((Kind::UnregisteredProgram, UNREGISTERED_PROGRAMS));
        app.get_or_insert_with(|| name.clone());
    }

    if evidence.is_empty() {
        return Outcome::NoEvidence;
    }

    // Age supports the evidence; a recent change caps the confidence.
    let newest = facts.newest.get(id as usize).copied().flatten();
    let mut points: u32 = evidence.iter().map(|e| e.points).sum();
    let mut recent = false;
    if let Some(newest) = newest {
        let age = ctx.now.saturating_sub(newest);
        if age < RECENT_DAYS * DAY {
            recent = true;
            push(
                &mut evidence,
                EvidenceKind::RecentlyChanged,
                0,
                "something inside changed in the last 30 days".to_owned(),
            );
        } else if age >= 365 * DAY {
            points += UNTOUCHED_365;
            push(
                &mut evidence,
                EvidenceKind::Untouched365,
                UNTOUCHED_365,
                "nothing inside was changed for a year or more".to_owned(),
            );
        } else if age >= 180 * DAY {
            points += UNTOUCHED_180;
            push(
                &mut evidence,
                EvidenceKind::Untouched180,
                UNTOUCHED_180,
                "nothing inside was changed for six months or more".to_owned(),
            );
        }
    }
    points = points.min(100);
    if recent {
        points = points.min(LOW_CAP);
    }
    // "No installed app claims it" alone is weak: portable tools, manual unzips and runtimes
    // look exactly like this. Without a second, independent kind of evidence, stay at Low.
    let only_unregistered = by_kind.iter().all(|(k, _)| *k == Kind::UnregisteredProgram);
    if only_unregistered {
        points = points.min(LOW_CAP);
    }
    let confidence = Confidence::from_points(points);

    // Primary kind: the strongest; ties go to the one found first.
    let kind = by_kind
        .iter()
        .fold(None::<(Kind, u32)>, |best, &(k, p)| match best {
            Some((_, bp)) if bp >= p => best,
            _ => Some((k, p)),
        })
        .map_or(Kind::BackupCopy, |(k, _)| k);
    let mut kinds: Vec<Kind> = by_kind.iter().map(|(k, _)| *k).collect();
    kinds.sort();
    kinds.dedup();
    evidence.sort_by_key(|e| std::cmp::Reverse(e.points));

    let path = display_path(&ctx.tree, &ctx.root_text, id);
    let refuse = |because: String| {
        Outcome::Refused(Refused {
            path: path.clone(),
            kind,
            because,
        })
    };

    // Vetoes, in order. Any of them stops the offer.
    let verdict = ctx.classifier.protected().check(&path, ctx.resolver);
    if verdict.protected {
        let why = verdict
            .reasons
            .first()
            .map_or_else(|| "protected".to_owned(), |r| r.description.clone());
        return refuse(format!("something protected: {why}"));
    }
    let owner_is_windows = assignment
        .and_then(|a| a.owner)
        .and_then(|o| ctx.owners.get(&o))
        .is_some_and(|o| {
            o.kind == OwnerKind::Windows
                || is_microsoft(&o.name)
                || o.publisher.as_deref().is_some_and(is_microsoft)
        });
    if owner_is_windows || is_os_area(&lower) {
        return refuse("it belongs to Windows or Microsoft".to_owned());
    }
    if let Some(blocker) = folder_blocker(&ctx.tree, &ctx.children, id, &path, ctx.classifier) {
        return refuse(format!("it contains {blocker}"));
    }

    // Personal data and risk.
    let bits = facts.personal.get(id as usize).copied().unwrap_or(0);
    let personal_data = personal::describe(bits);
    let mut risks = vec![if confidence >= Confidence::Medium {
        Risk::ProbablySafe
    } else {
        Risk::DontTouch
    }];
    if bits != 0 || report_only_app {
        risks.push(Risk::DontTouch);
    }
    let risk = Risk::escalate_all(risks);

    let (reason, note) = if bits != 0 {
        (
            "may_hold_personal_data",
            format!(
                " (shown only: may hold personal data: {})",
                personal_data.join(", ")
            ),
        )
    } else if report_only_app {
        (
            "may_hold_installed_games",
            " (shown only: apps like this can hold installed games or other data you may want)"
                .to_owned(),
        )
    } else if confidence == Confidence::Low {
        (
            "low_confidence",
            " (shown only: not enough evidence)".to_owned(),
        )
    } else {
        (kind.id(), String::new())
    };
    let who = app.clone().unwrap_or_else(|| name.clone());
    let summary = match kind {
        Kind::MissingApp => format!("Data of {who}, whose program folder no longer exists"),
        Kind::BackupCopy => format!("Looks like an old backup copy of {who}"),
        Kind::KnownAppRemoved => format!("Data folder of {who}, which is not installed any more"),
        Kind::UnregisteredProgram => "Program files that no installed app claims".to_owned(),
    } + &note;

    Outcome::Finding(Box::new(Finding {
        candidate: Candidate {
            node: id,
            path,
            risk,
            rule: kind.id().to_owned(),
            reason: reason.to_owned(),
            summary,
            sizes: Sizes {
                logical: node.agg.total_logical,
                on_disk: node.agg.total_on_disk,
                files: node.agg.file_count,
            },
            partial: false,
        },
        kind,
        kinds,
        confidence,
        points,
        evidence,
        app,
        newest_modified_unix: newest,
        personal_data,
        report_only_app,
    }))
}

fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        a.candidate
            .risk
            .cmp(&b.candidate.risk)
            .then(b.candidate.sizes.on_disk.cmp(&a.candidate.sizes.on_disk))
            .then(a.candidate.path.cmp(&b.candidate.path))
    });
}

/// Add up the offered and reported-only totals.
pub(crate) fn totals(findings: &[Finding]) -> (Sizes, Sizes) {
    let (mut offered, mut shown) = (Sizes::default(), Sizes::default());
    for f in findings {
        if f.candidate.risk == Risk::DontTouch {
            shown.add(f.candidate.sizes);
        } else {
            offered.add(f.candidate.sizes);
        }
    }
    (offered, shown)
}

pub(crate) fn analyze(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    attribution: &AttribResult,
    classifier: &Classifier,
    resolver: &dyn PathResolver,
    known: &KnownApps,
) -> Result<Analysis, CoreError> {
    let tree = scan.tree()?;
    let children = child_lists(&tree);
    let root_text = strip_verbatim(scan.root.trim()).replace('/', "\\");
    let facts = facts::compute(&tree);

    let stale_list = stale_apps(snapshot);
    let stale_keys: HashMap<&str, &InstalledApp> =
        stale_list.iter().map(|a| (a.key.as_str(), *a)).collect();
    let owners: HashMap<OwnerId, &Owner> = attribution.owners.iter().map(|o| (o.id, o)).collect();
    let stale: HashMap<OwnerId, &InstalledApp> = attribution
        .owners
        .iter()
        .filter(|o| o.kind == OwnerKind::App)
        .filter_map(|o| stale_keys.get(o.key.as_str()).map(|a| (o.id, *a)))
        .collect();
    let assignments: HashMap<NodeId, &Assignment> = attribution
        .assignments
        .iter()
        .map(|a| (a.node, a))
        .collect();
    let installed_names: Vec<String> = snapshot
        .apps
        .iter()
        .map(|a| normalize(&a.name))
        .filter(|n| !n.is_empty())
        .collect();

    let container_children = container_children(&tree, &children, &root_text);
    let mut siblings: HashMap<(NodeId, String), NodeId> = HashMap::new();
    for child in &container_children {
        if let Some(node) = tree.get(child.node) {
            siblings
                .entry((child.parent, node.name.to_lowercase()))
                .or_insert(child.node);
        }
    }

    let ctx = Ctx {
        tree,
        children,
        root_text: root_text.clone(),
        facts,
        now: scan.finished_unix,
        owners,
        assignments,
        stale,
        installed_names,
        known,
        classifier,
        resolver,
    };

    let mut findings: Vec<Finding> = Vec::new();
    let mut refused: Vec<Refused> = Vec::new();
    let mut examined: Vec<(String, NodeId)> = Vec::new();
    let mut seen: HashSet<NodeId> = HashSet::new();
    for child in &container_children {
        if !seen.insert(child.node) {
            continue;
        }
        examined.push((
            display_path(&ctx.tree, &ctx.root_text, child.node).to_lowercase(),
            child.node,
        ));
        match examine(&ctx, *child, &siblings) {
            Outcome::NoEvidence => {}
            Outcome::Finding(f) => findings.push(*f),
            Outcome::Refused(r) => refused.push(r),
        }
    }
    sort_findings(&mut findings);
    refused.sort_by(|a, b| a.path.cmp(&b.path));
    let (offered, reported_only) = totals(&findings);
    let mut stale_out: Vec<StaleApp> = stale_list
        .iter()
        .map(|a| StaleApp {
            name: a.name.clone(),
            publisher: a.publisher.clone(),
            registered_folder: a.install_folder.clone().unwrap_or_default(),
        })
        .collect();
    stale_out.sort_by(|a, b| a.name.cmp(&b.name));

    let count = u32::try_from(examined.len()).unwrap_or(u32::MAX);
    Ok(Analysis {
        leftovers: Leftovers {
            schema_version: SCHEMA_VERSION,
            scan_root: scan.root.clone(),
            findings,
            refused,
            stale_apps: stale_out,
            offered,
            reported_only,
            examined: count,
        },
        examined,
        ctx_root: root_text,
    })
}

//! Evidence rules. Each rule turns facts into claims; none of them decides ownership.

use std::collections::BTreeMap;

use dc_core::NodeId;
use dc_winmeta::{path_key, AppSnapshot, InstalledApp, SystemInfoSource, VersionInfo};

use crate::index::{container_kind, Container, Index};
use crate::model::{Evidence, EvidenceKind, Owner, OwnerId, OwnerKind, ProgramFinding};
use crate::names::{normalize, same_name, MIN_LEN};

/// A statement "this folder probably belongs to this owner".
#[derive(Debug, Clone)]
pub struct Claim {
    pub node: NodeId,
    pub owner: OwnerId,
    pub strength: u8,
    pub evidence: Evidence,
}

pub const STRENGTH_INSTALL: u8 = 90;
pub const STRENGTH_WINDOWS: u8 = 85;
pub const STRENGTH_MAIN_PROGRAM: u8 = 80;
pub const STRENGTH_NAME: u8 = 40;
/// A folder named like the app one level further down (`AppData\Local\<studio>\<app>`).
pub const STRENGTH_NAME_DEEPER: u8 = 35;
/// An install folder registered by several apps, for those not named like the folder (a
/// launcher's folder that its games also register).
pub const STRENGTH_SHARED_INSTALL: u8 = 60;
/// How deep inside a shared install folder a folder named like one of its apps is looked for.
const SHARED_DEPTH: u32 = 3;
pub const STRENGTH_VENDOR: u8 = 30;
pub const STRENGTH_PROGRAM: u8 = 25;

/// Version-info lookups per folder, and for the whole run.
const PROBES_PER_FOLDER: usize = 3;
const PROBES_TOTAL: usize = 3000;
/// How deep below a folder programs are looked for.
const PROBE_DEPTH: u32 = 4;

/// Apps that can own folders: real apps, not updates or nameless components.
fn eligible(app: &InstalledApp) -> bool {
    !app.flags.hidden_entry && !app.flags.update_or_patch && !app.name.trim().is_empty()
}

/// Owners and claims produced from the facts.
pub struct Gathered {
    pub owners: Vec<Owner>,
    pub claims: Vec<Claim>,
    pub programs: Vec<ProgramFinding>,
}

fn add_owner(
    owners: &mut Vec<Owner>,
    name: &str,
    kind: OwnerKind,
    key: &str,
    publisher: Option<&str>,
) -> OwnerId {
    let id = u32::try_from(owners.len()).unwrap_or(u32::MAX);
    owners.push(Owner {
        id,
        name: name.to_owned(),
        kind,
        key: key.to_owned(),
        publisher: publisher.map(str::to_owned),
    });
    id
}

const WINDOWS_AREAS: [&str; 7] = [
    r"\windows",
    r"\programdata\microsoft",
    r"\boot",
    r"\recovery",
    r"\efi",
    r"\system volume information",
    r"\$recycle.bin",
];

/// Run every rule.
pub fn gather(
    index: &Index<'_>,
    snapshot: &AppSnapshot,
    source: &dyn SystemInfoSource,
) -> Gathered {
    let mut owners: Vec<Owner> = Vec::new();
    let mut claims: Vec<Claim> = Vec::new();

    let windows = add_owner(&mut owners, "Windows", OwnerKind::Windows, "", None);

    // One owner per eligible app; apps are already sorted by name, so ids are deterministic.
    let mut app_ids: Vec<(OwnerId, &InstalledApp)> = Vec::new();
    for app in snapshot.apps.iter().filter(|a| eligible(a)) {
        let id = add_owner(
            &mut owners,
            &app.name,
            OwnerKind::App,
            &app.key,
            app.publisher.as_deref(),
        );
        app_ids.push((id, app));
    }

    // Vendors: one per distinct normalized publisher.
    let mut vendors: BTreeMap<String, OwnerId> = BTreeMap::new();
    for (_, app) in &app_ids {
        if let Some(publisher) = app.publisher.as_deref() {
            let norm = normalize(publisher);
            if norm.chars().count() >= MIN_LEN && !vendors.contains_key(&norm) {
                let id = add_owner(
                    &mut owners,
                    publisher.trim(),
                    OwnerKind::Vendor,
                    "",
                    Some(publisher.trim()),
                );
                vendors.insert(norm, id);
            }
        }
    }

    // Install folders and main programs.
    let mut by_folder: BTreeMap<String, Vec<(OwnerId, &InstalledApp)>> = BTreeMap::new();
    for (id, app) in &app_ids {
        if let Some(folder) = app.install_folder.as_deref() {
            by_folder
                .entry(path_key(folder))
                .or_default()
                .push((*id, app));
        }
    }
    // Where a game launcher's own record says the app's files are (anywhere on any drive).
    let mut launched: BTreeMap<String, Vec<(OwnerId, &InstalledApp)>> = BTreeMap::new();
    for (id, app) in &app_ids {
        for folder in &app.launcher_folders {
            launched
                .entry(path_key(folder))
                .or_default()
                .push((*id, app));
        }
    }
    let mut exe_folders: BTreeMap<String, Vec<(OwnerId, &InstalledApp)>> = BTreeMap::new();
    for (id, app) in &app_ids {
        if app.install_folder.is_none() {
            if let Some(exe) = app.main_exe.as_deref() {
                if let Some(parent) = dc_winmeta::parent_folder(exe) {
                    exe_folders
                        .entry(path_key(&parent))
                        .or_default()
                        .push((*id, app));
                }
            }
        }
    }

    // Install folders registered by several apps (a launcher and its games: HoYoPlay and
    // Genshin Impact both register C:\HoYoPlay). The app named like the folder keeps it; each
    // other app gets the folder named like it inside (`games\Genshin Impact game`).
    let mut shared: Vec<(NodeId, Vec<(OwnerId, &InstalledApp)>)> = Vec::new();
    for node in index.dirs() {
        let key = index.key(node.id);
        if key.is_empty() {
            continue;
        }
        if let Some(list) = by_folder.get(key) {
            let named: Vec<OwnerId> = if list.len() > 1 {
                list.iter()
                    .filter(|(_, app)| same_name(&node.name, &app.name))
                    .map(|(o, _)| *o)
                    .collect()
            } else {
                Vec::new()
            };
            for (owner, app) in list {
                let others = named.len() == 1 && named[0] != *owner;
                claims.push(Claim {
                    node: node.id,
                    owner: *owner,
                    strength: if others {
                        STRENGTH_SHARED_INSTALL
                    } else {
                        STRENGTH_INSTALL
                    },
                    evidence: Evidence {
                        kind: EvidenceKind::InstallFolder,
                        detail: if list.len() > 1 {
                            format!(
                                "registered install folder of \"{}\" (shared by {} apps)",
                                app.name,
                                list.len()
                            )
                        } else {
                            format!("registered install folder of \"{}\"", app.name)
                        },
                    },
                });
            }
            if list.len() > 1 {
                shared.push((node.id, list.clone()));
            }
        }
        if let Some(list) = launched.get(key) {
            for (owner, app) in list {
                claims.push(Claim {
                    node: node.id,
                    owner: *owner,
                    strength: STRENGTH_INSTALL,
                    evidence: Evidence {
                        kind: EvidenceKind::InstallFolder,
                        detail: format!(
                            "{} records that it installed \"{}\" here",
                            dc_winmeta::launcher_name(app.launcher.as_deref().unwrap_or_default()),
                            app.name
                        ),
                    },
                });
            }
        }
        if let Some(list) = exe_folders.get(key) {
            for (owner, app) in list {
                claims.push(Claim {
                    node: node.id,
                    owner: *owner,
                    strength: STRENGTH_MAIN_PROGRAM,
                    evidence: Evidence {
                        kind: EvidenceKind::MainProgram,
                        detail: format!("holds the main program of \"{}\"", app.name),
                    },
                });
            }
        }
        // Windows areas.
        if let Some(rest) = key.get(2..) {
            if WINDOWS_AREAS.contains(&rest) {
                claims.push(Claim {
                    node: node.id,
                    owner: windows,
                    strength: STRENGTH_WINDOWS,
                    evidence: Evidence {
                        kind: EvidenceKind::WindowsArea,
                        detail: "a Windows system area".to_owned(),
                    },
                });
            }
        }
        // Name matches: direct children of the known containers, and (a little weaker) one
        // level further down, where studios keep their apps (`LocalLow\miHoYo\Genshin Impact`).
        let parent_key = node.parent.map_or("", |p| index.key(p));
        let direct = container_kind(parent_key).is_some();
        // At the top of a drive this covers `C:\Games\Genshin Impact game`; never inside `Users`.
        let parent_node = node.parent.and_then(|p| index.tree.get(p));
        let deeper = !direct
            && parent_node.is_some_and(|p| !p.name.eq_ignore_ascii_case("users"))
            && parent_node.and_then(|p| p.parent).is_some_and(|g| {
                matches!(
                    container_kind(index.key(g)),
                    Some(
                        Container::UserData
                            | Container::ProgramData
                            | Container::ProgramFiles
                            | Container::DriveRoot
                    )
                )
            });
        if !direct && !deeper {
            continue;
        }
        let folder_name = normalize(&node.name);
        if folder_name.chars().count() < MIN_LEN {
            continue;
        }
        for (owner, app) in &app_ids {
            if same_name(&node.name, &app.name) {
                claims.push(Claim {
                    node: node.id,
                    owner: *owner,
                    strength: if direct {
                        STRENGTH_NAME
                    } else {
                        STRENGTH_NAME_DEEPER
                    },
                    evidence: Evidence {
                        kind: EvidenceKind::NameMatch,
                        detail: format!("folder name matches app \"{}\"", app.name),
                    },
                });
            }
        }
        if !direct {
            continue;
        }
        if let Some(owner) = vendors.get(&folder_name) {
            claims.push(Claim {
                node: node.id,
                owner: *owner,
                strength: STRENGTH_VENDOR,
                evidence: Evidence {
                    kind: EvidenceKind::PublisherFolder,
                    detail: "folder name matches the publisher of installed apps".to_owned(),
                },
            });
        }
    }
    for (folder, list) in &shared {
        for (owner, app) in list {
            for found in named_inside(index, *folder, &app.name) {
                claims.push(Claim {
                    node: found,
                    owner: *owner,
                    strength: STRENGTH_INSTALL,
                    evidence: Evidence {
                        kind: EvidenceKind::InstallFolder,
                        detail: format!(
                            "named like \"{}\" inside the install folder it shares",
                            app.name
                        ),
                    },
                });
            }
        }
    }
    let programs = probe_programs(index, source, &app_ids, &vendors, &mut claims);
    Gathered {
        owners,
        claims,
        programs,
    }
}

/// Folders below `dir` (at most [`SHARED_DEPTH`] down) named like `app`; the outermost ones only.
fn named_inside(index: &Index<'_>, dir: NodeId, app: &str) -> Vec<NodeId> {
    let base_depth = index.tree.get(dir).map_or(0, |n| n.depth);
    let mut stack = vec![dir];
    let mut found = Vec::new();
    while let Some(current) = stack.pop() {
        for &child in index.children(current) {
            let Some(node) = index.tree.get(child) else {
                continue;
            };
            if node.kind != dc_core::Kind::Dir {
                continue;
            }
            if same_name(&node.name, app) {
                found.push(child);
            } else if node.depth.saturating_sub(base_depth) < SHARED_DEPTH {
                stack.push(child);
            }
        }
    }
    found.sort_unstable();
    found
}

/// The largest `.exe` files below `dir`, at most [`PROBES_PER_FOLDER`], within [`PROBE_DEPTH`].
fn programs_in(index: &Index<'_>, dir: NodeId) -> Vec<NodeId> {
    let base_depth = index.tree.get(dir).map_or(0, |n| n.depth);
    let mut stack = vec![dir];
    let mut found: Vec<(u64, NodeId)> = Vec::new();
    while let Some(current) = stack.pop() {
        for &child in index.children(current) {
            let Some(node) = index.tree.get(child) else {
                continue;
            };
            match node.kind {
                dc_core::Kind::Dir if node.depth.saturating_sub(base_depth) < PROBE_DEPTH => {
                    stack.push(child);
                }
                dc_core::Kind::File if node.counted && node.ext.as_deref() == Some("exe") => {
                    found.push((node.logical_size, child));
                }
                _ => {}
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found
        .into_iter()
        .take(PROBES_PER_FOLDER)
        .map(|(_, id)| id)
        .collect()
}

fn text(value: Option<&String>) -> Option<&str> {
    value.map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// Look at programs inside folders (read-only version resources) to find main programs of apps
/// with an install folder, and to identify folders of apps without one.
fn probe_programs(
    index: &Index<'_>,
    source: &dyn SystemInfoSource,
    app_ids: &[(OwnerId, &InstalledApp)],
    vendors: &BTreeMap<String, OwnerId>,
    claims: &mut Vec<Claim>,
) -> Vec<ProgramFinding> {
    let mut budget = PROBES_TOTAL;
    let mut programs: Vec<ProgramFinding> = Vec::new();
    let mut new_claims: Vec<Claim> = Vec::new();

    // Which nodes already have claims, and which apps claim them by install folder.
    let claimed: std::collections::BTreeSet<NodeId> = claims.iter().map(|c| c.node).collect();
    let mut install_nodes: BTreeMap<NodeId, Vec<(OwnerId, &InstalledApp)>> = BTreeMap::new();
    for claim in claims
        .iter()
        .filter(|c| c.evidence.kind == EvidenceKind::InstallFolder)
    {
        if let Some((owner, app)) = app_ids.iter().find(|(o, _)| *o == claim.owner) {
            if app.main_exe.is_none() {
                install_nodes
                    .entry(claim.node)
                    .or_default()
                    .push((*owner, app));
            }
        }
    }

    // A. Apps with an install folder but no known main program.
    for (node, apps) in &install_nodes {
        for exe in programs_in(index, *node) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let path = index.display(exe);
            let Some(info) = source.version_info(&path) else {
                continue;
            };
            for (owner, app) in apps {
                if programs.iter().any(|p| p.owner == *owner) {
                    continue;
                }
                if matches_app(&info, app) {
                    programs.push(ProgramFinding {
                        owner: *owner,
                        path: path.clone(),
                        product: text(info.product.as_ref()).map(str::to_owned),
                        company: text(info.company.as_ref()).map(str::to_owned),
                    });
                    new_claims.push(Claim {
                        node: *node,
                        owner: *owner,
                        strength: STRENGTH_MAIN_PROGRAM,
                        evidence: Evidence {
                            kind: EvidenceKind::ProgramIdentity,
                            detail: format!(
                                "a program inside reports it belongs to \"{}\"",
                                app.name
                            ),
                        },
                    });
                }
            }
        }
    }

    // B. Unowned folders in the programs areas: who made the programs inside?
    let apps_without_folder: Vec<&(OwnerId, &InstalledApp)> = app_ids
        .iter()
        .filter(|(_, a)| a.install_folder.is_none())
        .collect();
    for node in index.dirs() {
        if budget == 0 {
            break;
        }
        let parent_key = node.parent.map_or("", |p| index.key(p));
        if !matches!(
            container_kind(parent_key),
            Some(Container::ProgramFiles | Container::CommonFiles | Container::ProgramData)
        ) || claimed.contains(&node.id)
        {
            continue;
        }
        let mut facts: Vec<(String, VersionInfo)> = Vec::new();
        for exe in programs_in(index, node.id) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let path = index.display(exe);
            if let Some(info) = source.version_info(&path) {
                facts.push((path, info));
            }
        }
        // The folder's identity is its most common product name.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for (_, info) in &facts {
            if let Some(product) = text(info.product.as_ref()) {
                *counts.entry(normalize(product)).or_insert(0) += 1;
            }
        }
        let product_norm = counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(k, _)| k.clone());
        let matched: Vec<&&(OwnerId, &InstalledApp)> = match &product_norm {
            Some(product) if product.chars().count() >= MIN_LEN => apps_without_folder
                .iter()
                .filter(|(_, app)| same_name(&app.name, product))
                .collect(),
            _ => Vec::new(),
        };
        let company_norm = facts
            .iter()
            .find_map(|(_, i)| text(i.company.as_ref()).map(normalize));
        let detail_program = facts.first().map(|(p, _)| p.clone()).unwrap_or_default();
        let publishers: std::collections::BTreeSet<String> = matched
            .iter()
            .filter_map(|(_, a)| a.publisher.as_deref().map(normalize))
            .collect();
        match matched.as_slice() {
            [] => {
                if let Some(vendor) = company_norm.as_ref().and_then(|c| vendors.get(c)) {
                    new_claims.push(Claim {
                        node: node.id,
                        owner: *vendor,
                        strength: STRENGTH_PROGRAM,
                        evidence: Evidence {
                            kind: EvidenceKind::ProgramIdentity,
                            detail: format!(
                                "programs inside are made by this publisher ({detail_program})"
                            ),
                        },
                    });
                }
            }
            [single] => new_claims.push(Claim {
                node: node.id,
                owner: single.0,
                strength: STRENGTH_PROGRAM,
                evidence: Evidence {
                    kind: EvidenceKind::ProgramIdentity,
                    detail: format!(
                        "a program inside reports product matching \"{}\"",
                        single.1.name
                    ),
                },
            }),
            many => {
                let vendor = (publishers.len() == 1)
                    .then(|| publishers.iter().next().and_then(|p| vendors.get(p)))
                    .flatten();
                if let Some(vendor) = vendor {
                    new_claims.push(Claim {
                        node: node.id,
                        owner: *vendor,
                        strength: STRENGTH_PROGRAM,
                        evidence: Evidence {
                            kind: EvidenceKind::ProgramIdentity,
                            detail: format!(
                                "programs inside match {} installed components of this publisher",
                                many.len()
                            ),
                        },
                    });
                } else {
                    for (owner, app) in many.iter().map(|m| &***m) {
                        new_claims.push(Claim {
                            node: node.id,
                            owner: *owner,
                            strength: STRENGTH_PROGRAM,
                            evidence: Evidence {
                                kind: EvidenceKind::ProgramIdentity,
                                detail: format!(
                                    "a program inside reports product matching \"{}\"",
                                    app.name
                                ),
                            },
                        });
                    }
                }
            }
        }
    }
    claims.extend(new_claims);
    programs.sort_by_key(|p| p.owner);
    programs
}

/// Does a program's version resource describe this app?
fn matches_app(info: &VersionInfo, app: &InstalledApp) -> bool {
    let by_product = text(info.product.as_ref()).is_some_and(|p| same_name(p, &app.name));
    let by_description = text(info.description.as_ref()).is_some_and(|d| same_name(d, &app.name));
    let by_company = match (text(info.company.as_ref()), app.publisher.as_deref()) {
        (Some(c), Some(p)) => {
            let (c, p) = (normalize(c), normalize(p));
            c.chars().count() >= MIN_LEN && c == p
        }
        _ => false,
    };
    by_product || by_description || (by_company && text(info.product.as_ref()).is_none())
}

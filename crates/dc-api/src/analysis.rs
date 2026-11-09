//! Rules, attribution and leftovers for one scan, turned into marks and owners a view can show.

use std::collections::HashMap;

use dc_core::{Risk, ScanResult, Tree};
use dc_protected::{FakeResolver, FsResolver};
use dc_rules::Classifier;
use dc_winmeta::AppSnapshot;
use serde::Serialize;

use crate::ApiError;

/// A piece of evidence and its points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    /// Points it adds.
    pub points: u32,
    /// Plain-language text.
    pub text: String,
}

/// What a check says about one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mark {
    /// The entry (index in the scan, or in the view once placed there).
    pub node: u32,
    /// "safe", "probably_safe" or "dont_touch".
    pub risk: &'static str,
    /// "rule", "leftover" or "refused".
    pub source: &'static str,
    /// Short title.
    pub title: String,
    /// Stable reason id.
    pub reason: String,
    /// Plain-language explanation.
    pub detail: String,
    /// "low", "medium" or "high" (leftovers only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<&'static str>,
    /// Why, point by point.
    pub evidence: Vec<Evidence>,
    /// Offered as a suggestion (`false`: shown only, or refused).
    pub offered: bool,
    /// Size on disk.
    pub bytes: u64,
}

/// An app (or vendor, or Windows) that owns folders. Every installed app is listed, also one
/// that owns nothing in the scanned folders (then it has no assignments).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Owner {
    /// Display name.
    pub name: String,
    /// "app", "vendor" or "windows".
    pub kind: &'static str,
    /// Publisher, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    /// Registry key, to tell same-named apps apart (not sent to views).
    #[serde(skip)]
    pub key: String,
    /// Version text (apps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Install date as `YYYY-MM-DD`, when the installer wrote a valid one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// Size the installer stated, in bytes (a hint; the scan's own numbers are what counts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stated_bytes: Option<u64>,
    /// The app has an uninstaller registered.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub uninstaller: bool,
    /// Last started, Unix seconds, from Windows' record of started programs. `None`: not known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
    /// How often it was started, from the same record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<u32>,
    /// Registered install folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// `20240115` as `2024-01-15`; anything else gives `None`.
fn install_date(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() != 8 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&t[..4], &t[4..6], &t[6..]);
    let ok =
        ("1990"..="2100").contains(&y) && ("01"..="12").contains(&m) && ("01"..="31").contains(&d);
    ok.then(|| format!("{y}-{m}-{d}"))
}

/// Apps that are worth listing: named, not updates or hidden parts, not system components.
fn listed(app: &dc_winmeta::InstalledApp) -> bool {
    let f = &app.flags;
    !f.hidden_entry && !f.update_or_patch && !f.system_component && !app.name.trim().is_empty()
}

/// Fill in what the snapshot knows about each app owner, and add the installed apps that own
/// nothing in the scan.
fn describe_apps(owners: &mut Vec<Owner>, snapshot: &AppSnapshot) {
    for app in snapshot.apps.iter().filter(|a| listed(a)) {
        let at = owners
            .iter()
            .position(|o| o.kind == "app" && o.key == app.key && o.name == app.name);
        let at = at.unwrap_or_else(|| {
            owners.push(Owner {
                name: app.name.clone(),
                kind: "app",
                publisher: app.publisher.clone(),
                key: app.key.clone(),
                ..Owner::default()
            });
            owners.len() - 1
        });
        let use_ = dc_winmeta::last_use(app, &snapshot.usage);
        let o = &mut owners[at];
        o.version = app.version.clone();
        o.installed = app.install_date.as_deref().and_then(install_date);
        o.stated_bytes = app.estimated_size_kb.map(|kb| u64::from(kb) * 1024);
        o.uninstaller = app.uninstall_command.is_some();
        o.last_used = use_.and_then(|(last, _)| last);
        o.runs = use_.map(|(_, runs)| runs);
        o.folder = app.install_folder.clone();
    }
}

/// Everything the checks found in one scan. Node numbers are indexes into that scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanAnalysis {
    /// Rule candidates, leftovers and refusals.
    pub marks: Vec<Mark>,
    /// Owners, indexed by `assignments`.
    pub owners: Vec<Owner>,
    /// `[node, owner, confidence]`; confidence 0 low, 1 medium, 2 high.
    pub assignments: Vec<[u32; 3]>,
}

pub(crate) fn risk_id(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::ProbablySafe => "probably_safe",
        Risk::DontTouch => "dont_touch",
    }
}

/// Plain words for a kind of leftover.
#[must_use]
pub fn leftover_kind_text(kind: dc_leftovers::Kind) -> &'static str {
    match kind {
        dc_leftovers::Kind::MissingApp => "app's program folder is gone",
        dc_leftovers::Kind::BackupCopy => "old backup copy",
        dc_leftovers::Kind::KnownAppRemoved => "known app, not installed",
        dc_leftovers::Kind::UnregisteredProgram => "programs nobody registered",
    }
}

fn refused(node: u32, reason: &str, because: &str, source: &'static str) -> Mark {
    Mark {
        node,
        risk: risk_id(Risk::DontTouch),
        source: "refused",
        title: format!("Refused ({source})"),
        reason: reason.to_owned(),
        detail: because.to_owned(),
        confidence: None,
        evidence: Vec::new(),
        offered: false,
        bytes: 0,
    }
}

/// Run the checks on one scan. With `apps`, owners and leftovers are included too.
/// Read-only: the only disk access is a second look at links and protected targets.
pub fn analyze(
    scan: &ScanResult,
    tree: &Tree,
    apps: Option<&AppSnapshot>,
    classifier: &Classifier,
) -> Result<ScanAnalysis, ApiError> {
    let mut out = ScanAnalysis::default();
    let now = scan.finished_unix;
    let found = dc_rules::find_candidates(scan, classifier, &FakeResolver::new(), now)?;
    let found = dc_rules::verify(found, classifier, &FsResolver);
    for c in &found.items {
        out.marks.push(Mark {
            node: c.node,
            risk: risk_id(c.risk),
            source: "rule",
            title: c.summary.clone(),
            reason: c.reason.clone(),
            detail: format!("rule {}", c.rule),
            confidence: None,
            evidence: Vec::new(),
            offered: c.risk != Risk::DontTouch,
            bytes: c.sizes.on_disk,
        });
    }
    let mut by_path: Option<HashMap<String, u32>> = None;
    for b in &found.blocked {
        if let Some(node) = lookup(&mut by_path, tree, &scan.root, &b.path) {
            out.marks.push(refused(node, &b.rule, &b.because, "rule"));
        }
    }

    let Some(snapshot) = apps else {
        return Ok(out);
    };
    let attrib = dc_attrib::attribute(scan, snapshot)?;
    let owners: HashMap<u32, &dc_attrib::Owner> = attrib.owners.iter().map(|o| (o.id, o)).collect();
    let mut ids: HashMap<(String, &'static str, String), u32> = HashMap::new();
    for a in &attrib.assignments {
        let Some(owner) = a.owner.and_then(|o| owners.get(&o).copied()) else {
            continue;
        };
        let confidence = match a.confidence {
            // Several possible owners: claim none.
            dc_attrib::Confidence::Ambiguous => continue,
            dc_attrib::Confidence::Low => 0,
            dc_attrib::Confidence::Medium => 1,
            dc_attrib::Confidence::High => 2,
        };
        let kind = match owner.kind {
            dc_attrib::OwnerKind::App => "app",
            dc_attrib::OwnerKind::Vendor => "vendor",
            dc_attrib::OwnerKind::Windows => "windows",
        };
        let next = u32::try_from(out.owners.len()).map_err(|_| ApiError::TooMany)?;
        let id = *ids
            .entry((owner.name.clone(), kind, owner.key.clone()))
            .or_insert_with(|| {
                out.owners.push(Owner {
                    name: owner.name.clone(),
                    kind,
                    publisher: owner.publisher.clone(),
                    key: owner.key.clone(),
                    ..Owner::default()
                });
                next
            });
        out.assignments.push([a.node, id, confidence]);
    }
    describe_apps(&mut out.owners, snapshot);
    let left = dc_leftovers::find_leftovers(scan, snapshot, &attrib, classifier, &FsResolver)?;
    let left = dc_leftovers::verify(left, classifier, &FsResolver);
    for f in &left.findings {
        let c = &f.candidate;
        out.marks.push(Mark {
            node: c.node,
            risk: risk_id(c.risk),
            source: "leftover",
            title: format!(
                "{}: {}",
                f.app.as_deref().unwrap_or("unknown app"),
                leftover_kind_text(f.kind)
            ),
            reason: c.reason.clone(),
            detail: c.summary.clone(),
            confidence: Some(match f.confidence {
                dc_leftovers::Confidence::Low => "low",
                dc_leftovers::Confidence::Medium => "medium",
                dc_leftovers::Confidence::High => "high",
            }),
            evidence: f
                .evidence
                .iter()
                .map(|e| Evidence {
                    points: e.points,
                    text: e.detail.clone(),
                })
                .collect(),
            offered: c.risk != Risk::DontTouch,
            bytes: c.sizes.on_disk,
        });
    }
    for r in &left.refused {
        if let Some(node) = lookup(&mut by_path, tree, &scan.root, &r.path) {
            out.marks
                .push(refused(node, r.kind.id(), &r.because, "leftover"));
        }
    }
    Ok(out)
}

/// Full paths of every entry, built once (the root keeps its path as given).
#[must_use]
pub fn full_paths(tree: &Tree, root: &str) -> Vec<String> {
    let nodes = tree.nodes();
    let mut full: Vec<String> = Vec::with_capacity(nodes.len());
    let head = root.trim_end_matches(['\\', '/']).to_owned();
    for n in nodes {
        let p = match n.parent.and_then(|p| full.get(p as usize)) {
            None => head.clone(),
            Some(parent) => format!("{parent}{}{}", std::path::MAIN_SEPARATOR, n.name),
        };
        full.push(p);
    }
    full
}

/// Find a folder by its full path (built once, folders only, compared without case).
fn lookup(
    cache: &mut Option<HashMap<String, u32>>,
    tree: &Tree,
    root: &str,
    path: &str,
) -> Option<u32> {
    let map = cache.get_or_insert_with(|| {
        let paths = full_paths(tree, root);
        let mut map = HashMap::new();
        for (n, p) in tree.nodes().iter().zip(paths) {
            if n.kind == dc_core::Kind::Dir {
                map.insert(normal(&p), n.id);
            }
        }
        map
    });
    map.get(&normal(path)).copied()
}

fn normal(p: &str) -> String {
    p.trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{describe_apps, install_date, Owner};
    use dc_winmeta::{
        build_snapshot_with, rot13, BuildOptions, FakeSource, KnownFolder, RawUninstallEntry,
        RawUsage, RegistrySource,
    };

    #[test]
    fn every_installed_app_is_listed_with_what_windows_knows() {
        let entry = |key: &str, name: &str, folder: &str| RawUninstallEntry {
            key: key.into(),
            source: Some(RegistrySource::Machine64),
            display_name: Some(name.into()),
            install_location: Some(folder.into()),
            display_version: Some("1.2".into()),
            install_date: Some("20240115".into()),
            estimated_size_kb: Some(2048),
            uninstall_string: Some(format!("\"{folder}\\uninstall.exe\"")),
            ..RawUninstallEntry::default()
        };
        let mut fake = FakeSource::new()
            .with_existing(r"C:\Program Files\Owns")
            .with_existing(r"C:\Program Files\Elsewhere");
        fake.folders = vec![KnownFolder {
            name: "ProgramFiles".into(),
            path: Some(r"C:\Program Files".into()),
        }];
        fake.entries = vec![
            entry("owns", "Owns Folders", r"C:\Program Files\Owns"),
            entry("else", "Elsewhere App", r"C:\Program Files\Elsewhere"),
        ];
        let mut data = vec![0u8; 72];
        data[4..8].copy_from_slice(&4u32.to_le_bytes());
        let jan_2025: u64 = (1_735_689_600 + 11_644_473_600) * 10_000_000;
        data[60..68].copy_from_slice(&jan_2025.to_le_bytes());
        fake.usage = vec![RawUsage {
            name: rot13(r"{905E63B6-C1BF-494E-B29C-65B732D3D21A}\Elsewhere\app.exe"),
            data,
        }];
        let snap = build_snapshot_with(
            &fake,
            &BuildOptions {
                read_usage: true,
                ..BuildOptions::default()
            },
        );
        let mut owners = vec![Owner {
            name: "Owns Folders".into(),
            kind: "app",
            key: "owns".into(),
            ..Owner::default()
        }];
        describe_apps(&mut owners, &snap);
        assert_eq!(owners.len(), 2, "the app that owns nothing is listed too");
        let owns = &owners[0];
        assert_eq!(owns.installed.as_deref(), Some("2024-01-15"));
        assert_eq!(owns.stated_bytes, Some(2048 * 1024));
        assert!(owns.uninstaller);
        assert_eq!(owns.last_used, None, "no record means not known");
        let other = &owners[1];
        assert_eq!(other.name, "Elsewhere App");
        assert_eq!(other.last_used, Some(1_735_689_600));
        assert_eq!(other.runs, Some(4));
    }

    #[test]
    fn install_dates_are_checked() {
        assert_eq!(install_date("20240115").as_deref(), Some("2024-01-15"));
        assert_eq!(install_date(" 20240115 ").as_deref(), Some("2024-01-15"));
        for bad in [
            "",
            "2024",
            "2024-01-15",
            "20241315",
            "20240100",
            "18000101",
            "2024011a",
            "２０２４０１１５",
        ] {
            assert_eq!(install_date(bad), None, "{bad}");
        }
    }
}

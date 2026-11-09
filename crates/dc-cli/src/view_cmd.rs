//! `dc view-data`: one compact file for the graph view (read-only).
//!
//! The file is columnar (one array per field) so a browser can load 1.5 million entries without
//! running out of memory. Several scans are joined under one virtual root. The file holds names
//! and sizes only (metadata), never file contents, and it never overwrites an existing file.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use dc_core::{Category, Kind, Risk, ScanResult};
use dc_protected::{FakeResolver, FsResolver};
use dc_rules::Classifier;
use serde::Serialize;

use crate::apps_cmd;
use crate::size::format_bytes;
use crate::timing::Stopwatch;

/// Version of the `dc-view` format. Bump on any change the viewer must know about.
pub const VIEW_SCHEMA_VERSION: u32 = 1;

pub struct ViewArgs {
    pub scans: Vec<PathBuf>,
    pub apps: Option<PathBuf>,
    pub out: PathBuf,
    pub timing: bool,
}

/// Flag bits in `nodes.flags`.
pub mod flag {
    pub const LINK: u32 = 1;
    pub const HARDLINK: u32 = 2;
    pub const CLOUD: u32 = 4;
    pub const SYSTEM: u32 = 8;
    pub const HIDDEN: u32 = 16;
    /// A second name of a hard-linked file: shown, but its size is not counted again.
    pub const NOT_COUNTED: u32 = 32;
    pub const COMPRESSED: u32 = 64;
    pub const SPARSE: u32 = 128;
}

const CATEGORIES: [Category; 11] = Category::ALL;

fn category_id(c: Category) -> &'static str {
    match c {
        Category::Video => "video",
        Category::Image => "image",
        Category::Audio => "audio",
        Category::Archive => "archive",
        Category::Code => "code",
        Category::Document => "document",
        Category::GameAsset => "game_asset",
        Category::Installer => "installer",
        Category::Executable => "executable",
        Category::Temporary => "temporary",
        Category::Other => "other",
    }
}

fn risk_id(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::ProbablySafe => "probably_safe",
        Risk::DontTouch => "dont_touch",
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Columns {
    pub name: Vec<String>,
    /// Index of the parent; -1 for the virtual root. Always smaller than the node's own index.
    pub parent: Vec<i64>,
    /// 0 file, 1 folder, 2 link, 3 other.
    pub kind: Vec<u8>,
    /// Own size on disk in bytes (0 for folders and for second names of hard links).
    pub size: Vec<u64>,
    /// Last change, seconds since 1970; 0 when unknown.
    pub mtime: Vec<i64>,
    pub flags: Vec<u32>,
    /// Index into `categories`, or -1.
    pub cat: Vec<i8>,
}

#[derive(Debug, Serialize)]
pub struct ViewOwner {
    pub name: String,
    /// "app", "vendor" or "windows".
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ViewEvidence {
    pub points: u32,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct Mark {
    pub node: u32,
    /// "safe", "probably_safe" or "dont_touch".
    pub risk: &'static str,
    /// "rule", "leftover" or "refused".
    pub source: &'static str,
    pub title: String,
    /// Stable reason id.
    pub reason: String,
    /// Plain-language explanation.
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<&'static str>,
    pub evidence: Vec<ViewEvidence>,
    /// Offered as a suggestion (`false`: shown only, or refused).
    pub offered: bool,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub schema: &'static str,
    pub schema_version: u32,
    pub tool_version: &'static str,
    pub created_unix: i64,
    pub roots: Vec<String>,
    pub categories: Vec<&'static str>,
    pub nodes: Columns,
    pub owners: Vec<ViewOwner>,
    /// `[node, owner, confidence]`, confidence 0 low, 1 medium, 2 high. Children inherit.
    pub assignments: Vec<[u32; 3]>,
    pub marks: Vec<Mark>,
}

fn node_index(offset: u32, id: u32) -> Result<u32> {
    offset
        .checked_add(id)
        .context("too many entries for one view file")
}

pub fn run(args: &ViewArgs) -> Result<ExitCode> {
    let mut watch = Stopwatch::new(args.timing);
    if args.out.exists() {
        bail!(
            "{} already exists; choose a new file name",
            args.out.display()
        );
    }
    let snapshot = match &args.apps {
        Some(p) => Some(apps_cmd::load(p)?),
        None => None,
    };
    let classifier = Classifier::builtin_with(crate::protected_cmd::build_list());
    watch.lap("read apps, protected");

    let mut view = View {
        schema: "dc-view",
        schema_version: VIEW_SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION"),
        created_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0)),
        roots: Vec::new(),
        categories: CATEGORIES.iter().map(|c| category_id(*c)).collect(),
        nodes: Columns::default(),
        owners: Vec::new(),
        assignments: Vec::new(),
        marks: Vec::new(),
    };
    // Node 0: the virtual root that holds every scan.
    push_node(&mut view.nodes, "This PC".to_owned(), -1, 1, 0, 0, 0, -1);
    let mut owner_ids: HashMap<(String, &'static str, String), u32> = HashMap::new();

    for path in &args.scans {
        let reader = BufReader::new(
            File::open(path).with_context(|| format!("cannot open {}", path.display()))?,
        );
        let scan: ScanResult = serde_json::from_reader(reader)
            .with_context(|| format!("{} is not a valid scan file", path.display()))?;
        watch.lap("read scan file");
        let tree = scan.tree().context("the scan file is damaged")?;
        let offset = u32::try_from(view.nodes.name.len()).context("too many entries")?;
        view.roots.push(scan.root.clone());
        for node in tree.nodes() {
            let parent = match node.parent {
                None => 0,
                Some(p) => i64::from(node_index(offset, p)?),
            };
            let kind = match node.kind {
                Kind::File => 0,
                Kind::Dir => 1,
                Kind::Link => 2,
                Kind::Other => 3,
            };
            let f = node.flags;
            let mut bits = 0;
            for (on, bit) in [
                (f.reparse, flag::LINK),
                (f.hardlink, flag::HARDLINK),
                (f.cloud, flag::CLOUD),
                (f.system, flag::SYSTEM),
                (f.hidden, flag::HIDDEN),
                (!node.counted, flag::NOT_COUNTED),
                (f.compressed, flag::COMPRESSED),
                (f.sparse, flag::SPARSE),
            ] {
                if on {
                    bits |= bit;
                }
            }
            let own = if node.kind == Kind::File && node.counted {
                node.size_on_disk
            } else {
                0
            };
            let cat = node
                .category
                .and_then(|c| CATEGORIES.iter().position(|x| *x == c))
                .and_then(|i| i8::try_from(i).ok())
                .unwrap_or(-1);
            let name = if node.parent.is_none() {
                scan.root.clone()
            } else {
                node.name.clone()
            };
            push_node(
                &mut view.nodes,
                name,
                parent,
                kind,
                own,
                node.mtime.map_or(0, |t| t.secs),
                bits,
                cat,
            );
        }
        watch.lap("columns");

        // Rule candidates (what regenerates or is junk), checked a second time against the disk.
        let now = scan.finished_unix;
        let found = dc_rules::find_candidates(&scan, &classifier, &FakeResolver::new(), now)
            .context("the scan file is damaged")?;
        let found = dc_rules::verify(found, &classifier, &FsResolver);
        for c in &found.items {
            view.marks.push(Mark {
                node: node_index(offset, c.node)?,
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
            if let Some(node) = lookup(&mut by_path, &tree, &scan.root, &b.path) {
                view.marks.push(refused(
                    node_index(offset, node)?,
                    &b.rule,
                    &b.because,
                    "rule",
                ));
            }
        }
        watch.lap("rules");

        if let Some(snapshot) = &snapshot {
            let attrib =
                dc_attrib::attribute(&scan, snapshot).context("the scan file is damaged")?;
            let owners: HashMap<u32, &dc_attrib::Owner> =
                attrib.owners.iter().map(|o| (o.id, o)).collect();
            for a in &attrib.assignments {
                let Some(owner) = a.owner.and_then(|o| owners.get(&o).copied()) else {
                    continue;
                };
                let kind = match owner.kind {
                    dc_attrib::OwnerKind::App => "app",
                    dc_attrib::OwnerKind::Vendor => "vendor",
                    dc_attrib::OwnerKind::Windows => "windows",
                };
                let key = (owner.name.clone(), kind, owner.key.clone());
                let next = u32::try_from(view.owners.len()).context("too many owners")?;
                let id = *owner_ids.entry(key).or_insert_with(|| {
                    view.owners.push(ViewOwner {
                        name: owner.name.clone(),
                        kind,
                        publisher: owner.publisher.clone(),
                    });
                    next
                });
                let confidence = match a.confidence {
                    // Several possible owners: claim none.
                    dc_attrib::Confidence::Ambiguous => continue,
                    dc_attrib::Confidence::Low => 0,
                    dc_attrib::Confidence::Medium => 1,
                    dc_attrib::Confidence::High => 2,
                };
                view.assignments
                    .push([node_index(offset, a.node)?, id, confidence]);
            }
            watch.lap("attribution");
            let left =
                dc_leftovers::find_leftovers(&scan, snapshot, &attrib, &classifier, &FsResolver)
                    .context("the scan file is damaged")?;
            let left = dc_leftovers::verify(left, &classifier, &FsResolver);
            for f in &left.findings {
                let c = &f.candidate;
                view.marks.push(Mark {
                    node: node_index(offset, c.node)?,
                    risk: risk_id(c.risk),
                    source: "leftover",
                    title: format!(
                        "{}: {}",
                        f.app.as_deref().unwrap_or("unknown app"),
                        crate::leftovers_cmd::kind_text(f.kind)
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
                        .map(|e| ViewEvidence {
                            points: e.points,
                            text: e.detail.clone(),
                        })
                        .collect(),
                    offered: c.risk != Risk::DontTouch,
                    bytes: c.sizes.on_disk,
                });
            }
            for r in &left.refused {
                if let Some(node) = lookup(&mut by_path, &tree, &scan.root, &r.path) {
                    view.marks.push(refused(
                        node_index(offset, node)?,
                        r.kind.id(),
                        &r.because,
                        "leftover",
                    ));
                }
            }
            watch.lap("leftovers");
        }
    }
    view.roots.sort();

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .with_context(|| format!("cannot create {}", args.out.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &view)?;
    writer.flush()?;
    watch.lap("write");
    let total: u64 = view.nodes.size.iter().sum();
    println!("entries   {}", view.nodes.name.len());
    println!("size      {}", format_bytes(total));
    println!("marks     {}", view.marks.len());
    println!("owners    {}", view.owners.len());
    println!("saved     {}", args.out.display());
    println!();
    println!(
        "Open the graph view and load this file. It stays on this computer. Nothing was changed."
    );
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::too_many_arguments)]
fn push_node(
    c: &mut Columns,
    name: String,
    parent: i64,
    kind: u8,
    size: u64,
    mtime: i64,
    flags: u32,
    cat: i8,
) {
    c.name.push(name);
    c.parent.push(parent);
    c.kind.push(kind);
    c.size.push(size);
    c.mtime.push(mtime);
    c.flags.push(flags);
    c.cat.push(cat);
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

/// Find a folder by its full path (built once, folders only, compared without case).
fn lookup(
    cache: &mut Option<HashMap<String, u32>>,
    tree: &dc_core::Tree,
    root: &str,
    path: &str,
) -> Option<u32> {
    let map = cache.get_or_insert_with(|| {
        let nodes = tree.nodes();
        let mut full: Vec<String> = Vec::with_capacity(nodes.len());
        let mut map = HashMap::new();
        for n in nodes {
            let p = match n.parent.and_then(|p| full.get(p as usize)) {
                None => root.trim_end_matches(['\\', '/']).to_owned(),
                Some(parent) => format!("{parent}\\{}", n.name),
            };
            if n.kind == Kind::Dir {
                map.insert(p.to_lowercase(), n.id);
            }
            full.push(p);
        }
        map
    });
    let wanted = path
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase();
    map.get(&wanted).copied()
}

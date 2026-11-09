//! The compact `dc-view` file: one array per field, so a browser can hold 1.5 million entries.
//! Several scans are joined under a virtual root "This PC".

use std::collections::HashMap;

use dc_core::{Category, Kind, Node, ScanResult, ScanStatus, Tree};
use dc_rules::Classifier;
use dc_winmeta::AppSnapshot;
use serde::Serialize;

use crate::analysis::{analyze, Mark, Owner, ScanAnalysis};
use crate::ApiError;

/// Version of the `dc-view` format. Bump on any change the viewer must know about.
pub const VIEW_SCHEMA_VERSION: u32 = 1;

/// Flag bits in `nodes.flags`.
pub mod flag {
    /// A link or junction (never followed).
    pub const LINK: u32 = 1;
    /// A file with more than one name.
    pub const HARDLINK: u32 = 2;
    /// A cloud placeholder.
    pub const CLOUD: u32 = 4;
    /// System attribute.
    pub const SYSTEM: u32 = 8;
    /// Hidden attribute.
    pub const HIDDEN: u32 = 16;
    /// A second name of a hard-linked file: shown, but its size is not counted again.
    pub const NOT_COUNTED: u32 = 32;
    /// Compressed by the file system.
    pub const COMPRESSED: u32 = 64;
    /// Sparse file.
    pub const SPARSE: u32 = 128;
}

/// File categories, in dc-core order (the view refers to them by index).
pub const CATEGORIES: [Category; 11] = Category::ALL;

/// Stable id of a category.
#[must_use]
pub fn category_id(c: Category) -> &'static str {
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

/// 0 file, 1 folder, 2 link, 3 other.
#[must_use]
pub fn kind_code(k: Kind) -> u8 {
    match k {
        Kind::File => 0,
        Kind::Dir => 1,
        Kind::Link => 2,
        Kind::Other => 3,
    }
}

/// Flag bits for a node.
#[must_use]
pub fn flag_bits(node: &Node) -> u32 {
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
    bits
}

/// Own size on disk as the view counts it (files only; second hard-link names count 0).
#[must_use]
pub fn own_size(node: &Node) -> u64 {
    if node.kind == Kind::File && node.counted {
        node.size_on_disk
    } else {
        0
    }
}

/// Index of a node's category, or -1.
#[must_use]
pub fn category_index(node: &Node) -> i8 {
    node.category
        .and_then(|c| CATEGORIES.iter().position(|x| *x == c))
        .and_then(|i| i8::try_from(i).ok())
        .unwrap_or(-1)
}

/// One array per field.
#[derive(Debug, Default, Serialize)]
pub struct Columns {
    /// Names (the scan roots keep their full path).
    pub name: Vec<String>,
    /// Index of the parent; -1 for the virtual root. Always smaller than the node's own index.
    pub parent: Vec<i64>,
    /// 0 file, 1 folder, 2 link, 3 other.
    pub kind: Vec<u8>,
    /// Own size on disk in bytes (0 for folders and for second names of hard links).
    pub size: Vec<u64>,
    /// Last change, seconds since 1970; 0 when unknown.
    pub mtime: Vec<i64>,
    /// Flag bits (see [`flag`]).
    pub flags: Vec<u32>,
    /// Index into `categories`, or -1.
    pub cat: Vec<i8>,
}

impl Columns {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        name: String,
        parent: i64,
        kind: u8,
        size: u64,
        mtime: i64,
        flags: u32,
        cat: i8,
    ) {
        self.name.push(name);
        self.parent.push(parent);
        self.kind.push(kind);
        self.size.push(size);
        self.mtime.push(mtime);
        self.flags.push(flags);
        self.cat.push(cat);
    }
}

/// The whole view file.
#[derive(Debug, Serialize)]
pub struct View {
    /// Always "dc-view".
    pub schema: &'static str,
    /// [`VIEW_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Version of the tool that made it.
    pub tool_version: &'static str,
    /// When it was made.
    pub created_unix: i64,
    /// The scanned folders.
    pub roots: Vec<String>,
    /// Category ids (indexed by `nodes.cat`).
    pub categories: Vec<&'static str>,
    /// The entries.
    pub nodes: Columns,
    /// Owners.
    pub owners: Vec<Owner>,
    /// `[node, owner, confidence]`, confidence 0 low, 1 medium, 2 high. Children inherit.
    pub assignments: Vec<[u32; 3]>,
    /// What the checks say.
    pub marks: Vec<Mark>,
}

/// Builds a [`View`] from one or more scans.
pub struct ViewBuilder {
    view: View,
    owner_ids: HashMap<(String, &'static str, String), u32>,
}

impl Default for ViewBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewBuilder {
    /// An empty view with the virtual root.
    #[must_use]
    pub fn new() -> Self {
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
        view.nodes.push("This PC".to_owned(), -1, 1, 0, 0, 0, -1);
        Self {
            view,
            owner_ids: HashMap::new(),
        }
    }

    /// Add one scan and what the checks found in it.
    pub fn add(
        &mut self,
        scan: &ScanResult,
        tree: &Tree,
        analysis: &ScanAnalysis,
    ) -> Result<(), ApiError> {
        let offset = u32::try_from(self.view.nodes.name.len()).map_err(|_| ApiError::TooMany)?;
        let at = |id: u32| offset.checked_add(id).ok_or(ApiError::TooMany);
        self.view.roots.push(scan.root.clone());
        for node in tree.nodes() {
            let parent = match node.parent {
                None => 0,
                Some(p) => i64::from(at(p)?),
            };
            let name = if node.parent.is_none() {
                scan.root.clone()
            } else {
                node.name.clone()
            };
            self.view.nodes.push(
                name,
                parent,
                kind_code(node.kind),
                own_size(node),
                node.mtime.map_or(0, |t| t.secs),
                flag_bits(node),
                category_index(node),
            );
        }
        for m in &analysis.marks {
            let mut m = m.clone();
            m.node = at(m.node)?;
            self.view.marks.push(m);
        }
        let mut local: Vec<u32> = Vec::with_capacity(analysis.owners.len());
        for o in &analysis.owners {
            let next = u32::try_from(self.view.owners.len()).map_err(|_| ApiError::TooMany)?;
            let id = *self
                .owner_ids
                .entry((o.name.clone(), o.kind, o.key.clone()))
                .or_insert_with(|| {
                    self.view.owners.push(o.clone());
                    next
                });
            local.push(id);
        }
        for a in &analysis.assignments {
            let Some(owner) = local.get(a[1] as usize) else {
                continue;
            };
            self.view.assignments.push([at(a[0])?, *owner, a[2]]);
        }
        Ok(())
    }

    /// The finished view (scan roots sorted for display).
    #[must_use]
    pub fn finish(mut self) -> View {
        self.view.roots.sort();
        self.view
    }
}

/// One view file for one scan, with the same checks a live scan shows (none for a stopped scan,
/// whose checks would be unreliable).
pub fn build(scan: &ScanResult, apps: Option<&AppSnapshot>) -> Result<View, ApiError> {
    let tree = scan.tree()?;
    let analysis = if scan.status == ScanStatus::Cancelled {
        ScanAnalysis::default()
    } else {
        let classifier = Classifier::builtin_with(crate::system::protected_list());
        analyze(scan, &tree, apps, &classifier)?
    };
    let mut builder = ViewBuilder::new();
    builder.add(scan, &tree, &analysis)?;
    Ok(builder.finish())
}

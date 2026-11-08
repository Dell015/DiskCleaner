//! The manifest: a JSON description of the fixture tree and its expected totals.
//!
//! It is written to `<out>/manifest.json`, next to `tree/` and never inside it, so it is not
//! part of the tree it describes. It contains no absolute paths, timestamps or machine details,
//! so two runs with the same seed and options produce byte-identical manifests.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{io_err, GenError};
use crate::writer::{Entry, EntryKind, Parts};
use crate::Options;

/// Version of the manifest format.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// How special entries are counted, stated in every manifest so later phases can assert
/// exact totals.
pub const COUNTING_RULES: &str = "Regular files are counted by logical size. Every hardlink path \
counts as a file path; the unique_* values count each hardlinked file once. Symlinks and \
junctions are links: they count as entries of 0 bytes and are never followed. Sparse files are \
counted by logical size. Each directory below the tree root counts once (so a top-level \
directory counts itself). Size on disk is not stated because it depends on the file system.";

/// The whole manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest format version.
    pub schema_version: u32,
    /// Which generator produced this.
    pub generator: GeneratorInfo,
    /// The options the tree was generated with.
    pub options: ManifestOptions,
    /// How totals are counted (see [`COUNTING_RULES`]).
    pub counting_rules: String,
    /// Expected totals for the whole tree.
    pub totals: Totals,
    /// Expected values per top-level directory.
    pub top_level: Vec<TopLevel>,
    /// What each category contains.
    pub categories: Vec<Category>,
    /// Every special entry (links, junction, sparse file) and whether it was created.
    pub special: Vec<SpecialEntry>,
}

/// Generator identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratorInfo {
    /// Always `fixture-gen`.
    pub name: String,
    /// The generator's version.
    pub version: String,
}

/// The options echoed into the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestOptions {
    /// The RNG seed.
    pub seed: u64,
    /// Number of files in the huge-file-count directory.
    pub file_count: u32,
}

/// Expected totals for the whole tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// Directories below the tree root.
    pub dirs: u64,
    /// Regular file paths, counting every hardlink path.
    pub file_paths: u64,
    /// Regular files, counting each hardlinked file once.
    pub unique_files: u64,
    /// Symlinks and junctions.
    pub links: u64,
    /// Sum of logical sizes over every file path.
    pub logical_bytes_all_paths: u64,
    /// Sum of logical sizes, each hardlinked file once.
    pub logical_bytes_unique: u64,
}

/// Expected values for one top-level directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopLevel {
    /// Directory name, relative to the tree root.
    pub path: String,
    /// Directories, including the top-level directory itself.
    pub dirs: u64,
    /// Regular file paths.
    pub file_paths: u64,
    /// Symlinks and junctions.
    pub links: u64,
    /// Sum of logical sizes over every file path.
    pub logical_bytes_all_paths: u64,
}

/// A named group of entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Category name.
    pub name: String,
    /// Entries, relative to the tree root with `/` separators. For summarized categories
    /// this is just the directory.
    pub paths: Vec<String>,
    /// For summarized categories, how many files they hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

/// Whether a special entry was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecialStatus {
    /// Created (and verified).
    Created,
    /// Not created, or not verifiable; see the reason.
    Skipped,
}

/// A junction, symlink, hardlink or sparse file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecialEntry {
    /// `junction`, `symlink_file`, `symlink_dir`, `hardlink` or `sparse`.
    pub kind: String,
    /// Where it is, relative to the tree root.
    pub path: String,
    /// What it points to (or, for a hardlink, the path it shares data with), relative to the
    /// tree root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Created or skipped.
    pub status: SpecialStatus,
    /// Why it was skipped. Always present when skipped, absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Build the manifest from what the writer actually created.
pub(crate) fn build(options: &Options, parts: Parts) -> Manifest {
    let mut totals = Totals::default();
    let mut tops: BTreeMap<String, TopLevel> = BTreeMap::new();

    for entry in &parts.entries {
        let top = top_component(entry);
        let slot = tops.entry(top.clone()).or_insert_with(|| TopLevel {
            path: top,
            ..TopLevel::default()
        });
        match entry.kind {
            EntryKind::Dir => {
                totals.dirs += 1;
                slot.dirs += 1;
            }
            EntryKind::File => {
                totals.file_paths += 1;
                totals.unique_files += 1;
                totals.logical_bytes_all_paths += entry.size;
                totals.logical_bytes_unique += entry.size;
                slot.file_paths += 1;
                slot.logical_bytes_all_paths += entry.size;
            }
            EntryKind::Hardlink => {
                totals.file_paths += 1;
                totals.logical_bytes_all_paths += entry.size;
                slot.file_paths += 1;
                slot.logical_bytes_all_paths += entry.size;
            }
            EntryKind::Link => {
                totals.links += 1;
                slot.links += 1;
            }
        }
    }

    let mut categories: Vec<Category> = parts
        .categories
        .into_iter()
        .map(|rec| {
            let mut paths = rec.paths;
            paths.sort();
            Category {
                name: rec.name,
                paths,
                count: rec.summarized.then_some(rec.count),
            }
        })
        .collect();
    categories.sort_by(|a, b| a.name.cmp(&b.name));

    let mut special = parts.special;
    special.sort_by(|a, b| a.path.cmp(&b.path));

    Manifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        generator: GeneratorInfo {
            name: "fixture-gen".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
        options: ManifestOptions {
            seed: options.seed,
            file_count: options.file_count,
        },
        counting_rules: COUNTING_RULES.to_owned(),
        totals,
        top_level: tops.into_values().collect(),
        categories,
        special,
    }
}

fn top_component(entry: &Entry) -> String {
    entry.path.split('/').next().unwrap_or_default().to_owned()
}

/// Write the manifest with `create_new`: it never overwrites, and it is the last thing written,
/// so its presence means generation completed.
pub(crate) fn write(path: &Path, manifest: &Manifest) -> Result<(), GenError> {
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|err| GenError::Io {
        context: "serialize manifest",
        path: "manifest.json".to_owned(),
        source: std::io::Error::other(err),
    })?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_err("create manifest", "manifest.json"))?;
    file.write_all(&bytes)
        .map_err(io_err("write manifest", "manifest.json"))?;
    file.flush()
        .map_err(io_err("flush manifest", "manifest.json"))
}

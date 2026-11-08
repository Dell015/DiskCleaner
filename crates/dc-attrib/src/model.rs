//! The data the attribution produces. Everything is plain, serializable data.

use serde::{Deserialize, Serialize};

/// Version of the attribution file format.
pub const SCHEMA_VERSION: u32 = 1;

/// Identifies an owner inside one [`AttribResult`].
pub type OwnerId = u32;

/// What kind of owner this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    /// An installed program.
    App,
    /// A folder named after a publisher that holds data or programs of that publisher.
    Vendor,
    /// Windows itself.
    Windows,
}

/// Someone who may own folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    /// Id inside this result.
    pub id: OwnerId,
    /// Display name.
    pub name: String,
    /// Kind of owner.
    pub kind: OwnerKind,
    /// Registry key for apps (to tell same-named apps apart); empty otherwise.
    pub key: String,
    /// Publisher, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
}

/// Why we think a folder belongs to someone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// The folder is the app's registered install folder.
    InstallFolder,
    /// The app's main program lives in this folder.
    MainProgram,
    /// A built-in table of Windows areas.
    WindowsArea,
    /// The folder is named like the app (in a per-user, shared-data or programs folder).
    NameMatch,
    /// The folder is named like the app's publisher.
    PublisherFolder,
    /// A program inside the folder reports (in its version resource) a product or company
    /// that matches the app or publisher.
    ProgramIdentity,
}

/// One piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Kind.
    pub kind: EvidenceKind,
    /// Human-readable detail (what matched what).
    pub detail: String,
}

/// How sure we are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Weak evidence.
    Low,
    /// Reasonable evidence.
    Medium,
    /// Strong evidence.
    High,
    /// Two or more owners have about equally strong evidence; no owner is claimed.
    Ambiguous,
}

/// Ownership of a folder (and, unless a deeper folder has its own assignment, everything below).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    /// The folder's node id in the scan.
    pub node: u32,
    /// The primary owner, or `None` when ambiguous.
    pub owner: Option<OwnerId>,
    /// Confidence.
    pub confidence: Confidence,
    /// Evidence for the primary owner (or for the leading candidates when ambiguous).
    pub evidence: Vec<Evidence>,
    /// Other owners with evidence for the same folder.
    pub alternatives: Vec<OwnerId>,
}

/// Sizes that belong to one bucket. Each byte of the scan is counted in exactly one bucket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sizes {
    /// Sum of file lengths.
    pub logical: u64,
    /// Sum of space on disk.
    pub on_disk: u64,
    /// Number of files.
    pub files: u64,
}

/// Totals of one owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerTotals {
    /// The owner.
    pub owner: OwnerId,
    /// Sizes that are exclusively this owner's.
    pub own: Sizes,
    /// Space on disk of the owner's folders including parts that belong to other owners nested
    /// inside them (for a vendor folder: all its members). Not additive across owners.
    pub footprint_on_disk: u64,
    /// Number of folders assigned (topmost and nested).
    pub folders: u32,
}

/// A big folder nobody owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnattributedDir {
    /// Node id.
    pub node: u32,
    /// Full path.
    pub path: String,
    /// Space on disk of what is unowned inside it.
    pub on_disk: u64,
}

/// A program found inside an app's folder whose version resource matches that app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramFinding {
    /// The app that owns it.
    pub owner: OwnerId,
    /// Full path of the program.
    pub path: String,
    /// `ProductName` of the program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// `CompanyName` of the program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
}

/// The result of attribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttribResult {
    /// Format version.
    pub schema_version: u32,
    /// The scanned path.
    pub scan_root: String,
    /// Total space on disk of the scan; equals the sum of all buckets.
    pub scan_total_on_disk: u64,
    /// All owners, by id.
    pub owners: Vec<Owner>,
    /// Assignments, sorted by node id.
    pub assignments: Vec<Assignment>,
    /// Per-owner totals, sorted by owner id.
    pub totals: Vec<OwnerTotals>,
    /// Folders claimed about equally by several owners.
    pub shared: Sizes,
    /// Everything without evidence.
    pub unattributed: Sizes,
    /// The biggest unowned folders.
    pub top_unattributed: Vec<UnattributedDir>,
    /// Programs identified inside app folders (optional in the file).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<ProgramFinding>,
}

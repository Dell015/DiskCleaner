//! The data the leftover search produces. Everything is plain, serializable data.

use dc_rules::{Candidate, Sizes};
use serde::{Deserialize, Serialize};

/// Version of the stored result format.
pub const SCHEMA_VERSION: u32 = 1;

/// The kind of leftover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Data of an app that is still registered but whose program folder is gone.
    MissingApp,
    /// A folder named like an old backup of a folder that is still there (`X.old`).
    BackupCopy,
    /// A folder named like a well-known app that is not installed any more.
    KnownAppRemoved,
    /// Program files in Program Files that no installed app claims.
    UnregisteredProgram,
}

impl Kind {
    /// Stable id, also used as the rule and reason id of the candidate.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Kind::MissingApp => "leftover_missing_app",
            Kind::BackupCopy => "leftover_backup_copy",
            Kind::KnownAppRemoved => "leftover_known_app_removed",
            Kind::UnregisteredProgram => "leftover_unregistered_program",
        }
    }
}

/// How sure we are that a folder is a leftover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Weak evidence: reported, never offered.
    Low,
    /// Reasonable evidence.
    Medium,
    /// Strong evidence.
    High,
}

impl Confidence {
    /// Points needed for `High`.
    pub const HIGH_AT: u32 = 80;
    /// Points needed for `Medium`.
    pub const MEDIUM_AT: u32 = 50;

    /// Confidence for a number of points.
    #[must_use]
    pub fn from_points(points: u32) -> Confidence {
        if points >= Self::HIGH_AT {
            Confidence::High
        } else if points >= Self::MEDIUM_AT {
            Confidence::Medium
        } else {
            Confidence::Low
        }
    }
}

/// What a piece of evidence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// The app that owns the folder is registered, but its program folder does not exist.
    StaleRegistration,
    /// The folder is named exactly like that app.
    NameMatch,
    /// The folder is named like a backup of a sibling folder.
    BackupName,
    /// The folder is named like a well-known app that is not installed.
    KnownApp,
    /// The folder holds programs that no installed app claims.
    UnregisteredPrograms,
    /// Nothing inside was changed for 180 days or more.
    Untouched180,
    /// Nothing inside was changed for 365 days or more.
    Untouched365,
    /// Something inside changed within the last 30 days (no points; caps confidence at Low).
    RecentlyChanged,
}

/// One piece of evidence with the points it adds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// What it is.
    pub kind: EvidenceKind,
    /// Points added to the score.
    pub points: u32,
    /// Plain-language detail.
    pub detail: String,
}

/// A folder that looks like something an app left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Path, risk, reason and sizes in the same form as a rule candidate (B4b).
    pub candidate: Candidate,
    /// The main kind of evidence.
    pub kind: Kind,
    /// Every kind of evidence that applies.
    pub kinds: Vec<Kind>,
    /// How sure we are.
    pub confidence: Confidence,
    /// The score behind `confidence` (0 to 100).
    pub points: u32,
    /// The evidence, strongest first.
    pub evidence: Vec<Evidence>,
    /// The app it probably belonged to (for display only).
    pub app: Option<String>,
    /// Newest modified time inside, seconds since the Unix epoch.
    pub newest_modified_unix: Option<i64>,
    /// What looks like personal data inside (empty if nothing).
    pub personal_data: Vec<String>,
    /// The list entry says this kind of app can hold games or other wanted data.
    pub report_only_app: bool,
}

/// A folder that has evidence but must not be offered at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    /// The folder.
    pub path: String,
    /// The kind of evidence it had.
    pub kind: Kind,
    /// Why it was refused.
    pub because: String,
}

/// An installed app whose program folder no longer exists (information only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleApp {
    /// Display name.
    pub name: String,
    /// Publisher.
    pub publisher: Option<String>,
    /// The folder the registration points to.
    pub registered_folder: String,
}

/// The result of looking for leftovers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leftovers {
    /// Format version.
    pub schema_version: u32,
    /// The scan root.
    pub scan_root: String,
    /// Findings: offered ones first, then biggest first.
    pub findings: Vec<Finding>,
    /// Folders with evidence that were refused.
    pub refused: Vec<Refused>,
    /// Registered apps whose program folder is gone.
    pub stale_apps: Vec<StaleApp>,
    /// Total of the findings that are offered (`ProbablySafe`).
    pub offered: Sizes,
    /// Total of the findings that are only reported (`DontTouch`).
    pub reported_only: Sizes,
    /// How many folders were looked at.
    pub examined: u32,
}

/// Where a folder is, for `explain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The path is not in the scan.
    NotInScan,
    /// It is in the scan but not a folder directly inside a container.
    NotExamined,
    /// It was examined and no evidence was found.
    NoEvidence,
    /// It is a finding.
    Finding,
    /// It had evidence but was refused.
    Refused,
}

/// What the search says about one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The path as asked about, as found in the scan (empty if not in the scan).
    pub path: String,
    /// What happened to it.
    pub standing: Standing,
    /// The finding, if there is one.
    pub finding: Option<Finding>,
    /// The refusal, if there is one.
    pub refused: Option<Refused>,
    /// Plain-language notes about the steps.
    pub notes: Vec<String>,
}

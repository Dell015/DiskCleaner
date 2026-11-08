//! Where system information comes from. Logic is written against [`SystemInfoSource`] so it
//! can be tested with [`FakeSource`] on any platform; the real source is Windows-only.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{KnownFolder, Problem, RawUninstallEntry, Shortcut, VersionInfo};
use crate::parse::path_key;
use crate::signature::SignatureStatus;

/// Data plus the non-fatal problems met while collecting it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceReport<T> {
    /// What was read.
    pub items: T,
    /// What could not be read.
    pub problems: Vec<Problem>,
}

/// A read-only view of the system.
pub trait SystemInfoSource {
    /// The known folders (an unresolved one has `path: None`).
    fn known_folders(&self) -> Vec<KnownFolder>;
    /// All uninstall entries, from every registry location.
    fn uninstall_entries(&self) -> SourceReport<Vec<RawUninstallEntry>>;
    /// Start Menu and Desktop shortcuts that could be read.
    fn shortcuts(&self) -> SourceReport<Vec<Shortcut>>;
    /// The version resource of a program file, if it has one. Must not run the program.
    fn version_info(&self, exe: &str) -> Option<VersionInfo>;
    /// Check the digital signature of a program file. Offline, read-only, never runs the file.
    fn signature(&self, exe: &str) -> SignatureStatus;
    /// Whether a file or folder exists. Must not follow links or touch contents.
    fn path_exists(&self, path: &str) -> bool;
    /// The value of an environment variable (case-insensitive on Windows).
    fn env_var(&self, name: &str) -> Option<String>;
}

/// An in-memory source for tests.
#[derive(Debug, Clone, Default)]
pub struct FakeSource {
    /// Known folders to report.
    pub folders: Vec<KnownFolder>,
    /// Entries to report.
    pub entries: Vec<RawUninstallEntry>,
    /// Problems to report along with the entries.
    pub problems: Vec<Problem>,
    /// Shortcuts to report.
    pub shortcuts: Vec<Shortcut>,
    /// Version info by program path (compared case-insensitively).
    pub versions: BTreeMap<String, VersionInfo>,
    /// Signature results by program path (compared case-insensitively).
    pub signatures: BTreeMap<String, SignatureStatus>,
    /// Environment variables (names are matched case-insensitively).
    pub env: BTreeMap<String, String>,
    existing: BTreeSet<String>,
}

impl FakeSource {
    /// An empty source.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare that a path exists (compared case-insensitively).
    pub fn with_existing(mut self, path: &str) -> Self {
        self.existing.insert(path_key(path));
        self
    }
}

impl SystemInfoSource for FakeSource {
    fn known_folders(&self) -> Vec<KnownFolder> {
        self.folders.clone()
    }

    fn uninstall_entries(&self) -> SourceReport<Vec<RawUninstallEntry>> {
        SourceReport {
            items: self.entries.clone(),
            problems: self.problems.clone(),
        }
    }

    fn shortcuts(&self) -> SourceReport<Vec<Shortcut>> {
        SourceReport {
            items: self.shortcuts.clone(),
            problems: Vec::new(),
        }
    }

    fn version_info(&self, exe: &str) -> Option<VersionInfo> {
        let wanted = path_key(exe);
        self.versions
            .iter()
            .find(|(path, _)| path_key(path) == wanted)
            .map(|(_, info)| info.clone())
    }

    fn signature(&self, exe: &str) -> SignatureStatus {
        let wanted = path_key(exe);
        self.signatures
            .iter()
            .find(|(path, _)| path_key(path) == wanted)
            .map(|(_, status)| status.clone())
            .unwrap_or(SignatureStatus::Unknown {
                reason: "not in the fake source".into(),
            })
    }

    fn path_exists(&self, path: &str) -> bool {
        self.existing.contains(&path_key(path))
    }

    fn env_var(&self, name: &str) -> Option<String> {
        self.env
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }
}

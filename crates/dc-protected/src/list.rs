//! The built-in protected list, as data. `dc protected --list` prints this same table.

use crate::normalize::{normalize, NormalPath};
use crate::pattern::Pattern;
use crate::verdict::Reason;

/// How far an entry reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The location and everything below it.
    Subtree,
    /// Only the location itself (its children are judged separately).
    SelfOnly,
    /// Any path having a component that matches, at any depth, and everything below it.
    AnyDepth,
}

impl Scope {
    /// Short name used in listings.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Subtree => "subtree",
            Scope::SelfOnly => "self",
            Scope::AnyDepth => "any-depth",
        }
    }
}

/// One row of the built-in table: `(pattern, scope, reason id, description)`.
pub type Row = (&'static str, Scope, &'static str, &'static str);

use Scope::{AnyDepth, SelfOnly, Subtree};

/// The compiled-in list. Adding a row can only protect more.
pub const BUILTIN: &[Row] = &[
    // Windows itself (includes WinSxS, System32, DriverStore, Installer, servicing).
    (
        r"*:\windows",
        Subtree,
        "windows_dir",
        "Windows system folder (includes WinSxS, System32, DriverStore, Installer)",
    ),
    (r"*:\boot", Subtree, "boot", "boot files"),
    (
        r"*:\recovery",
        Subtree,
        "recovery",
        "Windows recovery environment",
    ),
    (r"*:\efi", Subtree, "boot", "EFI boot files"),
    (
        r"*:\system volume information",
        Subtree,
        "system_volume_information",
        "restore points and volume metadata",
    ),
    (r"*:\$recycle.bin", Subtree, "recycle_bin", "Recycle Bin"),
    (
        r"*:\programdata\microsoft",
        Subtree,
        "programdata_microsoft",
        "Microsoft system data (Defender, Windows Update state)",
    ),
    // Installer bookkeeping: deleting these breaks uninstall and repair of many programs.
    (
        r"*:\programdata\package cache",
        Subtree,
        "installer_cache",
        "installer package cache (needed to repair or uninstall programs)",
    ),
    (
        r"*:\config.msi",
        Subtree,
        "installer_cache",
        "Windows Installer rollback data",
    ),
    (
        r"*:\$patchcache$",
        Subtree,
        "installer_cache",
        "Windows Installer patch cache",
    ),
    (
        r"*:\program files\windowsapps",
        Subtree,
        "store_apps",
        "Microsoft Store apps",
    ),
    (
        r"*:\program files\common files",
        Subtree,
        "common_files",
        "components shared by installed programs",
    ),
    (
        r"*:\program files (x86)\common files",
        Subtree,
        "common_files",
        "components shared by installed programs",
    ),
    // Credentials and keys.
    (
        r"*:\users\*\appdata\roaming\microsoft\credentials",
        Subtree,
        "credentials",
        "saved Windows credentials",
    ),
    (
        r"*:\users\*\appdata\roaming\microsoft\protect",
        Subtree,
        "credentials",
        "master keys that decrypt your saved secrets",
    ),
    (
        r"*:\users\*\appdata\roaming\microsoft\vault",
        Subtree,
        "credentials",
        "Windows vault",
    ),
    (
        r"*:\users\*\appdata\local\microsoft\credentials",
        Subtree,
        "credentials",
        "saved Windows credentials",
    ),
    (
        r"*:\users\*\appdata\local\microsoft\vault",
        Subtree,
        "credentials",
        "Windows vault",
    ),
    (r"*:\users\*\.ssh", Subtree, "credentials", "SSH keys"),
    (r"*:\users\*\.gnupg", Subtree, "credentials", "GPG keys"),
    (
        r"*:\users\*\.aws",
        Subtree,
        "credentials",
        "cloud credentials",
    ),
    (
        r"*:\users\*\.azure",
        Subtree,
        "credentials",
        "cloud credentials",
    ),
    (
        r"*:\users\*\.kube",
        Subtree,
        "credentials",
        "cluster credentials",
    ),
    (
        r"*:\users\*\ntuser.dat*",
        Subtree,
        "user_registry",
        "your user registry hive",
    ),
    // Personal data.
    (
        r"*:\users\*\documents",
        Subtree,
        "user_data",
        "your documents",
    ),
    (r"*:\users\*\desktop", Subtree, "user_data", "your desktop"),
    (
        r"*:\users\*\pictures",
        Subtree,
        "user_data",
        "your pictures",
    ),
    (r"*:\users\*\music", Subtree, "user_data", "your music"),
    (r"*:\users\*\videos", Subtree, "user_data", "your videos"),
    (
        r"*:\users\*\onedrive*",
        Subtree,
        "user_data",
        "OneDrive (deleting syncs the deletion to the cloud)",
    ),
    // Containers and virtual machines: one file holds a whole system.
    (
        r"*:\users\*\appdata\local\docker",
        Subtree,
        "virtual_machines",
        "Docker data (images, containers, volumes)",
    ),
    (
        r"*:\programdata\docker",
        Subtree,
        "virtual_machines",
        "Docker data (images, containers, volumes)",
    ),
    (
        r"*.vhd",
        AnyDepth,
        "virtual_disk",
        "virtual disk (holds a whole machine, WSL distro or Docker data)",
    ),
    (
        r"*.vhdx",
        AnyDepth,
        "virtual_disk",
        "virtual disk (holds a whole machine, WSL distro or Docker data)",
    ),
    (r"*.vmdk", AnyDepth, "virtual_disk", "virtual disk"),
    (r"*.vdi", AnyDepth, "virtual_disk", "virtual disk"),
    (r"*.qcow2", AnyDepth, "virtual_disk", "virtual disk"),
    // Version control, anywhere.
    (
        r".git",
        AnyDepth,
        "git_repository",
        "git repository (history may exist nowhere else)",
    ),
    (
        r"system volume information",
        AnyDepth,
        "system_volume_information",
        "restore points and volume metadata",
    ),
    (r"$recycle.bin", AnyDepth, "recycle_bin", "Recycle Bin"),
    // Structure: the folders themselves (their children are judged on their own).
    (r"*:", SelfOnly, "drive_root", "a whole drive"),
    (
        r"*:\program files",
        SelfOnly,
        "structure",
        "installed programs folder",
    ),
    (
        r"*:\program files (x86)",
        SelfOnly,
        "structure",
        "installed programs folder",
    ),
    (
        r"*:\programdata",
        SelfOnly,
        "structure",
        "shared program data folder",
    ),
    (r"*:\users", SelfOnly, "structure", "all user profiles"),
    (
        r"*:\users\*",
        SelfOnly,
        "structure",
        "a user profile folder",
    ),
    (
        r"*:\users\*\appdata",
        SelfOnly,
        "structure",
        "application data folder",
    ),
    (
        r"*:\users\*\appdata\local",
        SelfOnly,
        "structure",
        "application data folder",
    ),
    (
        r"*:\users\*\appdata\roaming",
        SelfOnly,
        "structure",
        "application data folder",
    ),
    (
        r"*:\users\*\appdata\locallow",
        SelfOnly,
        "structure",
        "application data folder",
    ),
    // Special files in the drive root.
    (r"*:\pagefile.sys", SelfOnly, "system_file", "paging file"),
    (
        r"*:\hiberfil.sys",
        SelfOnly,
        "system_file",
        "hibernation file",
    ),
    (r"*:\swapfile.sys", SelfOnly, "system_file", "swap file"),
    (r"*:\bootmgr", SelfOnly, "boot", "boot manager"),
    (r"*:\bootnxt", SelfOnly, "boot", "boot manager"),
    (
        r"*:\$*",
        Subtree,
        "system_metadata",
        "NTFS metadata or upgrade staging folder ($MFT, $WINDOWS.~BT, ...)",
    ),
];

/// Drive-root folders that start with `$` but are left over from a Windows upgrade. They are
/// not protected by the `$*` entry so that the rules engine can classify them (decision D54).
/// Every other `$` name stays protected.
pub const UPGRADE_LEFTOVERS: [&str; 2] = ["$windows.~bt", "$windows.~ws"];

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub pattern: Pattern,
    pub scope: Scope,
    pub reason: Reason,
}

/// The protected list: built-in rows plus any runtime additions (which can only add protection).
#[derive(Debug, Clone)]
pub struct ProtectedList {
    pub(crate) entries: Vec<Entry>,
    skipped: Vec<String>,
}

impl ProtectedList {
    /// The compiled-in list.
    pub fn builtin() -> ProtectedList {
        let mut entries = Vec::with_capacity(BUILTIN.len());
        let mut skipped = Vec::new();
        for (text, scope, id, description) in BUILTIN {
            // A row that does not parse is a bug in this file; it is reported, not ignored
            // silently, and a test makes sure none exist.
            let parsed = if *scope == Scope::AnyDepth {
                Some(Pattern::any_component(text))
            } else {
                Pattern::parse(text)
            };
            match parsed {
                Some(pattern) => entries.push(Entry {
                    pattern,
                    scope: *scope,
                    reason: Reason::new(id, description),
                }),
                None => skipped.push((*text).to_owned()),
            }
        }
        ProtectedList { entries, skipped }
    }

    /// A new list that also protects `path` and everything below it. If the path cannot be
    /// understood it is not added; see [`ProtectedList::skipped_extras`].
    #[must_use]
    pub fn with_extra(&self, path: &str, id: &str, description: &str) -> ProtectedList {
        let mut next = self.clone();
        match normalize(path) {
            Ok(normal) => next.entries.push(Entry {
                pattern: Pattern::literal(&normal),
                scope: Scope::Subtree,
                reason: Reason::new(id, description),
            }),
            Err(_) => next.skipped.push(path.to_owned()),
        }
        next
    }

    /// Extra paths (or built-in rows) that could not be used, so the caller can warn.
    pub fn skipped_extras(&self) -> &[String] {
        &self.skipped
    }

    /// Every protection that applies to an already-normalized path.
    pub(crate) fn matches(&self, path: &NormalPath) -> Vec<Reason> {
        let mut found = Vec::new();
        for entry in &self.entries {
            if entry.reason.id == "system_metadata"
                && path
                    .parts
                    .first()
                    .is_some_and(|p| UPGRADE_LEFTOVERS.contains(&p.as_str()))
            {
                continue;
            }
            let hit = match entry.scope {
                Scope::Subtree => entry.pattern.matches_subtree(path),
                Scope::SelfOnly => entry.pattern.matches_exactly(path),
                Scope::AnyDepth => entry.pattern.matches_any_component(path),
            };
            if hit {
                found.push(entry.reason.clone());
            } else if entry.scope == Scope::Subtree
                && entry.pattern.is_proper_ancestor_of_pattern(path)
            {
                found.push(Reason::new(
                    "contains_protected",
                    "this folder contains a protected location",
                ));
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_row_parses() {
        assert!(ProtectedList::builtin().skipped_extras().is_empty());
        assert_eq!(ProtectedList::builtin().entries.len(), BUILTIN.len());
    }
}

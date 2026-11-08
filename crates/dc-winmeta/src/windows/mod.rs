//! The real, Windows-only [`SystemInfoSource`]. Read-only: it opens registry keys with read
//! rights, asks Windows for known-folder paths, and checks whether paths exist.

mod folders;
mod registry;
mod version;

use std::path::PathBuf;

use crate::lnk::collect_shortcuts;
use crate::model::{KnownFolder, RawUninstallEntry, RegistrySource, Shortcut, VersionInfo};
use crate::source::{SourceReport, SystemInfoSource};

use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};

/// Reads the live system.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsSource;

impl SystemInfoSource for WindowsSource {
    fn known_folders(&self) -> Vec<KnownFolder> {
        folders::known_folders()
    }

    fn uninstall_entries(&self) -> SourceReport<Vec<RawUninstallEntry>> {
        let mut all = SourceReport::<Vec<RawUninstallEntry>>::default();
        let locations = [
            (
                HKEY_LOCAL_MACHINE,
                KEY_WOW64_64KEY,
                RegistrySource::Machine64,
                "registry:machine64",
            ),
            (
                HKEY_LOCAL_MACHINE,
                KEY_WOW64_32KEY,
                RegistrySource::Machine32,
                "registry:machine32",
            ),
            (
                HKEY_CURRENT_USER,
                KEY_WOW64_64KEY,
                RegistrySource::User,
                "registry:user",
            ),
        ];
        for (root, view, source, label) in locations {
            let part = registry::read_uninstall(root, view, source, label);
            all.items.extend(part.items);
            all.problems.extend(part.problems);
        }
        all
    }

    fn shortcuts(&self) -> SourceReport<Vec<Shortcut>> {
        let wanted = ["StartMenu", "CommonStartMenu", "Desktop"];
        let dirs: Vec<PathBuf> = self
            .known_folders()
            .into_iter()
            .filter(|folder| wanted.contains(&folder.name.as_str()))
            .filter_map(|folder| folder.path.map(PathBuf::from))
            .collect();
        collect_shortcuts(&dirs)
    }

    fn version_info(&self, exe: &str) -> Option<VersionInfo> {
        version::read_version_info(exe)
    }

    fn path_exists(&self, path: &str) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

//! Building the app snapshot from a [`SystemInfoSource`]. Pure logic: all system access goes
//! through the trait, so it is tested with [`crate::FakeSource`] on every platform.

use crate::model::{
    AppFlags, AppSnapshot, InstalledApp, RawUninstallEntry, RegistrySource, SNAPSHOT_SCHEMA_VERSION,
};
use crate::parse::derive_install_folder;
use crate::source::SystemInfoSource;

/// `ReleaseType` values that mark updates rather than apps.
const UPDATE_RELEASE_TYPES: [&str; 6] = [
    "update",
    "security update",
    "hotfix",
    "service pack",
    "update rollup",
    "critical update",
];

fn non_empty(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

fn is_update(entry: &RawUninstallEntry) -> bool {
    let has_parent = entry
        .parent_key_name
        .as_deref()
        .is_some_and(|p| !p.trim().is_empty());
    let release = entry
        .release_type
        .as_deref()
        .map(|r| r.trim().to_lowercase())
        .is_some_and(|r| UPDATE_RELEASE_TYPES.contains(&r.as_str()));
    has_parent || release
}

/// Read known folders and uninstall entries from `source` and turn them into a snapshot.
/// Entries are never merged or dropped; apps are sorted by name.
pub fn build_snapshot(source: &dyn SystemInfoSource) -> AppSnapshot {
    let known_folders = source.known_folders();
    let report = source.uninstall_entries();
    let env = |name: &str| source.env_var(name);

    let mut apps: Vec<InstalledApp> = report
        .items
        .iter()
        .map(|entry| {
            let name = non_empty(&entry.display_name);
            let derived = derive_install_folder(entry, &known_folders, &env);
            let stale = derived
                .as_ref()
                .is_some_and(|(folder, _)| !source.path_exists(folder));
            InstalledApp {
                key: entry.key.clone(),
                source: entry.source.unwrap_or(RegistrySource::Machine64),
                name: name.clone().unwrap_or_default(),
                publisher: non_empty(&entry.publisher),
                version: non_empty(&entry.display_version),
                install_folder: derived.as_ref().map(|(folder, _)| folder.clone()),
                folder_origin: derived.as_ref().map(|(_, origin)| *origin),
                display_icon: non_empty(&entry.display_icon),
                uninstall_command: non_empty(&entry.uninstall_string),
                estimated_size_kb: entry.estimated_size_kb,
                install_date: non_empty(&entry.install_date),
                flags: AppFlags {
                    system_component: entry.system_component,
                    update_or_patch: is_update(entry),
                    windows_installer: entry.windows_installer,
                    hidden_entry: name.is_none(),
                    stale_registration: stale,
                    no_install_folder: derived.is_none(),
                },
            }
        })
        .collect();

    apps.sort_by(|a, b| {
        (a.name.to_lowercase(), a.key.to_lowercase(), a.source).cmp(&(
            b.name.to_lowercase(),
            b.key.to_lowercase(),
            b.source,
        ))
    });

    AppSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        created_by: format!("dc-winmeta {}", env!("CARGO_PKG_VERSION")),
        known_folders,
        apps,
        problems: report.problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FolderOrigin, KnownFolder, Problem};
    use crate::FakeSource;

    fn folder(name: &str, path: &str) -> KnownFolder {
        KnownFolder {
            name: name.into(),
            path: Some(path.into()),
        }
    }

    fn entry(key: &str, name: Option<&str>) -> RawUninstallEntry {
        RawUninstallEntry {
            key: key.into(),
            source: Some(RegistrySource::Machine64),
            display_name: name.map(str::to_owned),
            ..RawUninstallEntry::default()
        }
    }

    fn source() -> FakeSource {
        let mut fake = FakeSource::new().with_existing(r"C:\Program Files\Good");
        fake.folders = vec![
            folder("Windows", r"C:\Windows"),
            folder("ProgramFiles", r"C:\Program Files"),
        ];
        fake.env
            .insert("ProgramFiles".into(), r"C:\Program Files".into());

        let mut good = entry("Good", Some("Good App"));
        good.publisher = Some(" Good Inc ".into());
        good.install_location = Some(r"%ProgramFiles%\Good".into());
        good.estimated_size_kb = Some(2048);

        let mut stale = entry("Gone", Some("Gone App"));
        stale.display_icon = Some(r"C:\Program Files\Gone\gone.exe,0".into());

        let mut nofolder = entry("Msi", Some("Msi App"));
        nofolder.uninstall_string = Some("MsiExec.exe /X{GUID}".into());
        nofolder.windows_installer = true;

        let mut patch = entry("KB1", Some("Update for X"));
        patch.parent_key_name = Some("X".into());

        let mut release = entry("KB2", Some("Security thing"));
        release.release_type = Some("Security Update".into());

        let mut hidden = entry("Comp", None);
        hidden.system_component = true;

        fake.entries = vec![hidden, stale, good, nofolder, patch, release];
        fake.problems = vec![Problem {
            source: "registry:user".into(),
            message: "access denied".into(),
        }];
        fake
    }

    fn app<'a>(snapshot: &'a AppSnapshot, key: &str) -> &'a InstalledApp {
        snapshot.apps.iter().find(|a| a.key == key).unwrap()
    }

    #[test]
    fn a_normal_app_gets_its_folder_and_clean_fields() {
        let snap = build_snapshot(&source());
        let good = app(&snap, "Good");
        assert_eq!(
            good.install_folder.as_deref(),
            Some(r"C:\Program Files\Good")
        );
        assert_eq!(good.folder_origin, Some(FolderOrigin::InstallLocation));
        assert_eq!(good.publisher.as_deref(), Some("Good Inc"));
        assert_eq!(good.estimated_size_kb, Some(2048));
        assert_eq!(good.flags, AppFlags::default());
    }

    #[test]
    fn missing_folders_are_flagged_stale_but_kept() {
        let snap = build_snapshot(&source());
        let gone = app(&snap, "Gone");
        assert!(gone.flags.stale_registration);
        assert!(!gone.flags.no_install_folder);
        assert_eq!(gone.folder_origin, Some(FolderOrigin::DisplayIcon));
    }

    #[test]
    fn entries_without_a_usable_folder_are_flagged() {
        let snap = build_snapshot(&source());
        let msi = app(&snap, "Msi");
        assert!(msi.flags.no_install_folder);
        assert!(!msi.flags.stale_registration);
        assert!(msi.flags.windows_installer);
        assert_eq!(msi.install_folder, None);
    }

    #[test]
    fn updates_hidden_and_system_entries_are_marked_not_dropped() {
        let snap = build_snapshot(&source());
        assert_eq!(snap.apps.len(), 6);
        assert!(app(&snap, "KB1").flags.update_or_patch);
        assert!(app(&snap, "KB2").flags.update_or_patch);
        let comp = app(&snap, "Comp");
        assert!(comp.flags.hidden_entry && comp.flags.system_component);
        assert_eq!(comp.name, "");
    }

    #[test]
    fn apps_are_sorted_and_problems_carried_over() {
        let snap = build_snapshot(&source());
        let names: Vec<&str> = snap.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "",
                "Gone App",
                "Good App",
                "Msi App",
                "Security thing",
                "Update for X"
            ]
        );
        assert_eq!(snap.problems.len(), 1);
        assert_eq!(snap.schema_version, SNAPSHOT_SCHEMA_VERSION);
    }

    #[test]
    fn the_snapshot_survives_a_json_round_trip() {
        let snap = build_snapshot(&source());
        let text = serde_json::to_string(&snap).unwrap();
        let back: AppSnapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(back, snap);
        assert!(text.contains("\"stale_registration\":true"));
    }
}

//! Building the app snapshot from a [`SystemInfoSource`]. Pure logic: all system access goes
//! through the trait, so it is tested with [`crate::FakeSource`] on every platform.

use crate::model::{
    AppFlags, AppSnapshot, FolderOrigin, InstalledApp, RawUninstallEntry, RegistrySource, Shortcut,
    VersionInfo, SNAPSHOT_SCHEMA_VERSION,
};
use crate::parse::{
    derive_install_folder, expand_env, is_absolute, is_too_broad, normalize_folder, parent_folder,
    path_from_command, path_key,
};
use crate::source::SystemInfoSource;

/// At most this many shortcuts are listed per app.
const MAX_SHORTCUTS_PER_APP: usize = 10;

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

/// Shortcuts after the one-time work of normalizing their targets.
struct PreparedShortcut<'a> {
    shortcut: &'a Shortcut,
    /// Comparison key of the target path.
    target_key: String,
    /// The target path, when it is an absolute path.
    target: String,
}

fn prepare_shortcuts<'a>(
    shortcuts: &'a [Shortcut],
    env: &dyn Fn(&str) -> Option<String>,
) -> Vec<PreparedShortcut<'a>> {
    shortcuts
        .iter()
        .filter_map(|shortcut| {
            let target = expand_env(shortcut.target.as_deref()?, env);
            is_absolute(&target).then(|| PreparedShortcut {
                shortcut,
                target_key: path_key(&target),
                target,
            })
        })
        .collect()
}

fn is_uninstaller(path: &str) -> bool {
    let name = path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    name.contains("unins") || name.contains("uninstall") || name.contains("setup")
}

fn inside(path_key_text: &str, folder_key: &str) -> bool {
    path_key_text.starts_with(&format!("{folder_key}\\"))
}

/// Read known folders, uninstall entries and shortcuts from `source` and turn them into a
/// snapshot. Entries are never merged or dropped; apps are sorted by name.
///
/// Install folders come from the registry first. An app with no registry folder may take the
/// folder of the target of a shortcut with exactly its name (marked as such). Shortcuts that
/// point into an app's folder are attached to it, and the first plausible program in the
/// folder becomes its `main_exe`, whose version resource is then read.
pub fn build_snapshot(source: &dyn SystemInfoSource) -> AppSnapshot {
    let known_folders = source.known_folders();
    let report = source.uninstall_entries();
    let shortcut_report = source.shortcuts();
    let env = |name: &str| source.env_var(name);
    let prepared = prepare_shortcuts(&shortcut_report.items, &env);

    let mut apps: Vec<InstalledApp> = report
        .items
        .iter()
        .map(|entry| {
            let name = non_empty(&entry.display_name);
            let mut derived = derive_install_folder(entry, &known_folders, &env);

            // Fall back to a shortcut named exactly like the app.
            if derived.is_none() {
                if let Some(wanted) = name.as_deref().map(str::to_lowercase) {
                    derived = prepared
                        .iter()
                        .filter(|p| {
                            p.shortcut.name.to_lowercase() == wanted && !is_uninstaller(&p.target)
                        })
                        .filter_map(|p| parent_folder(&p.target).and_then(|f| normalize_folder(&f)))
                        .find(|folder| {
                            !is_too_broad(folder, &known_folders) && source.path_exists(folder)
                        })
                        .map(|folder| (folder, FolderOrigin::Shortcut));
                }
            }

            let folder_key = derived.as_ref().map(|(folder, _)| path_key(folder));
            let attached: Vec<&PreparedShortcut<'_>> = folder_key
                .as_deref()
                .map(|key| {
                    prepared
                        .iter()
                        .filter(|p| inside(&p.target_key, key))
                        .collect()
                })
                .unwrap_or_default();

            // The main program: the icon's file, else the first shortcut target that is a program.
            let is_exe = |path: &str| path.to_ascii_lowercase().ends_with(".exe");
            let from_icon = entry
                .display_icon
                .as_deref()
                .and_then(path_from_command)
                .map(|icon| expand_env(&icon, &env))
                .filter(|icon| is_absolute(icon) && is_exe(icon) && !is_uninstaller(icon));
            let main_exe = from_icon
                .into_iter()
                .chain(attached.iter().map(|p| p.target.clone()))
                .find(|exe| {
                    is_exe(exe)
                        && !is_uninstaller(exe)
                        && folder_key
                            .as_deref()
                            .is_some_and(|key| inside(&path_key(exe), key))
                        && source.path_exists(exe)
                });
            let version_info = main_exe
                .as_deref()
                .and_then(|exe| source.version_info(exe))
                .filter(|info| *info != VersionInfo::default());

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
                main_exe,
                version_info,
                shortcuts: attached
                    .iter()
                    .map(|p| p.shortcut.lnk.clone())
                    .take(MAX_SHORTCUTS_PER_APP)
                    .collect(),
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

    let mut problems = report.problems;
    problems.extend(shortcut_report.problems);
    AppSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        created_by: format!("dc-winmeta {}", env!("CARGO_PKG_VERSION")),
        known_folders,
        apps,
        problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KnownFolder, Problem};
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

    #[test]
    fn shortcuts_are_attached_find_folders_and_name_the_main_program() {
        let mut fake = FakeSource::new()
            .with_existing(r"C:\Program Files\Good")
            .with_existing(r"C:\Program Files\Good\good.exe")
            .with_existing(r"D:\Games\Fun")
            .with_existing(r"D:\Games\Fun\fun.exe");
        fake.folders = vec![folder("ProgramFiles", r"C:\Program Files")];
        let mut good = entry("Good", Some("Good App"));
        good.install_location = Some(r"C:\Program Files\Good".into());
        let lonely = entry("Fun", Some("Fun Game"));
        fake.entries = vec![good, lonely];
        let link = |name: &str, target: &str| Shortcut {
            lnk: format!(r"C:\Start\{name}.lnk"),
            name: name.into(),
            target: Some(target.into()),
            arguments: None,
            working_dir: None,
        };
        fake.shortcuts = vec![
            link("Good App", r"C:\Program Files\Good\good.exe"),
            link("Uninstall Good", r"C:\Program Files\Good\unins000.exe"),
            link("Fun Game", r"D:\Games\Fun\fun.exe"),
        ];
        fake.versions.insert(
            r"C:\Program Files\Good\good.exe".into(),
            VersionInfo {
                company: Some("Good Inc".into()),
                ..VersionInfo::default()
            },
        );

        let snap = build_snapshot(&fake);
        let good = app(&snap, "Good");
        assert_eq!(good.shortcuts.len(), 2);
        assert_eq!(
            good.main_exe.as_deref(),
            Some(r"C:\Program Files\Good\good.exe")
        );
        assert_eq!(
            good.version_info
                .as_ref()
                .and_then(|v| v.company.as_deref()),
            Some("Good Inc")
        );

        let fun = app(&snap, "Fun");
        assert_eq!(fun.install_folder.as_deref(), Some(r"D:\Games\Fun"));
        assert_eq!(fun.folder_origin, Some(FolderOrigin::Shortcut));
        assert!(!fun.flags.no_install_folder && !fun.flags.stale_registration);
        assert_eq!(fun.main_exe.as_deref(), Some(r"D:\Games\Fun\fun.exe"));
    }

    #[test]
    fn a_shortcut_with_another_name_does_not_invent_a_folder() {
        let mut fake = FakeSource::new().with_existing(r"D:\Tools\x");
        fake.entries = vec![entry("A", Some("Some App"))];
        fake.shortcuts = vec![Shortcut {
            lnk: r"C:\Start\Other.lnk".into(),
            name: "Other".into(),
            target: Some(r"D:\Tools\x\x.exe".into()),
            arguments: None,
            working_dir: None,
        }];
        let snap = build_snapshot(&fake);
        assert!(app(&snap, "A").flags.no_install_folder);
    }
}

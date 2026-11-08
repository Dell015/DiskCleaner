//! Tests of the real Windows source. They only read the live system: known folders and the
//! registry. They compile to nothing on other platforms.
#![allow(missing_docs)]
#![cfg(windows)]

use dc_winmeta::{build_snapshot, SystemInfoSource, WindowsSource};

#[test]
fn known_folders_resolve_to_absolute_existing_paths() {
    let folders = WindowsSource.known_folders();
    for name in [
        "LocalAppData",
        "RoamingAppData",
        "ProgramFiles",
        "Windows",
        "UserProfile",
        "Temp",
    ] {
        let folder = folders.iter().find(|f| f.name == name).unwrap();
        let path = folder
            .path
            .as_deref()
            .unwrap_or_else(|| panic!("{name} unresolved"));
        assert!(dc_winmeta::is_absolute(path), "{name}: {path}");
        assert!(
            WindowsSource.path_exists(path),
            "{name}: {path} does not exist"
        );
    }
}

#[test]
fn the_registry_lists_installed_programs() {
    let report = WindowsSource.uninstall_entries();
    assert!(!report.items.is_empty(), "no uninstall entries found");
    assert!(
        report.items.iter().any(|e| e.display_name.is_some()),
        "no entry has a display name"
    );
}

#[test]
fn a_snapshot_of_this_machine_is_consistent() {
    let snapshot = build_snapshot(&WindowsSource);
    assert!(!snapshot.apps.is_empty());
    for app in &snapshot.apps {
        assert_eq!(
            app.flags.no_install_folder,
            app.install_folder.is_none(),
            "{}",
            app.key
        );
        if app.flags.stale_registration {
            assert!(app.install_folder.is_some(), "{}", app.key);
        }
        if let Some(folder) = &app.install_folder {
            assert!(dc_winmeta::is_absolute(folder), "{}: {folder}", app.key);
        }
    }
    let text = serde_json::to_string(&snapshot).unwrap();
    assert!(serde_json::from_str::<dc_winmeta::AppSnapshot>(&text).is_ok());
}

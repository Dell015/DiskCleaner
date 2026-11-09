//! Things read from this computer: the protected list (with the real Windows folders) and the
//! installed apps. On other systems these fall back to the built-in list and "no apps".

use dc_protected::ProtectedList;
use dc_winmeta::AppSnapshot;

/// The built-in protected list plus, on Windows, the real Windows and System folders.
#[must_use]
pub fn protected_list() -> ProtectedList {
    let list = ProtectedList::builtin();
    #[cfg(windows)]
    let list = {
        use dc_winmeta::SystemInfoSource;
        let mut list = list;
        for folder in dc_winmeta::WindowsSource.known_folders() {
            if matches!(folder.name.as_str(), "Windows" | "System") {
                if let Some(path) = folder.path {
                    list = list.with_extra(
                        &path,
                        "known_system_folder",
                        "Windows system folder on this computer",
                    );
                }
            }
        }
        list
    };
    list
}

/// Installed apps from the Windows registry (read-only). `None` on other systems.
#[must_use]
pub fn installed_apps(verify_signatures: bool) -> Option<AppSnapshot> {
    #[cfg(windows)]
    {
        Some(dc_winmeta::build_snapshot_with(
            &dc_winmeta::WindowsSource,
            &dc_winmeta::BuildOptions { verify_signatures },
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = verify_signatures;
        None
    }
}

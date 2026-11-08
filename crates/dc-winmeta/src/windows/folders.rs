//! Known folders via `SHGetKnownFolderPath` (not environment variables alone).

use std::ffi::c_void;

use windows_sys::core::{GUID, PWSTR};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_CommonStartMenu, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads,
    FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, FOLDERID_Profile, FOLDERID_ProgramData,
    FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_RoamingAppData, FOLDERID_StartMenu,
    FOLDERID_System, FOLDERID_Windows, SHGetKnownFolderPath,
};

use crate::model::KnownFolder;

/// Upper bound on a path length in UTF-16 units; Windows paths never exceed 32,767.
const MAX_PATH_UNITS: usize = 32_768;

fn resolve(id: &GUID) -> Option<String> {
    let mut raw: PWSTR = std::ptr::null_mut();
    // SAFETY: `id` points to a valid GUID; flags 0 and a null token select the current user's
    // default; `raw` is a writable PWSTR that receives a COM-allocated string.
    let result = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut raw) };
    if raw.is_null() {
        return None;
    }
    let text = if result >= 0 {
        // SAFETY: on success `raw` points to a NUL-terminated UTF-16 string. The length is
        // capped so a broken string cannot run past `MAX_PATH_UNITS` units, and the slice is
        // only read before the buffer is freed below.
        let units = unsafe {
            let mut length = 0usize;
            while length < MAX_PATH_UNITS && *raw.add(length) != 0 {
                length += 1;
            }
            std::slice::from_raw_parts(raw, length)
        };
        Some(String::from_utf16_lossy(units))
    } else {
        None
    };
    // SAFETY: `raw` was allocated by `SHGetKnownFolderPath`, which requires the caller to
    // free it with `CoTaskMemFree`, exactly once, even on failure.
    unsafe { CoTaskMemFree(raw as *const c_void) };
    text.filter(|t| !t.is_empty())
}

/// The folders listed in the requirements, in a fixed order. Unresolved ones keep `path: None`.
pub fn known_folders() -> Vec<KnownFolder> {
    let table: [(&str, &GUID); 14] = [
        ("LocalAppData", &FOLDERID_LocalAppData),
        ("RoamingAppData", &FOLDERID_RoamingAppData),
        ("LocalAppDataLow", &FOLDERID_LocalAppDataLow),
        ("ProgramData", &FOLDERID_ProgramData),
        ("ProgramFiles", &FOLDERID_ProgramFiles),
        ("ProgramFilesX86", &FOLDERID_ProgramFilesX86),
        ("Windows", &FOLDERID_Windows),
        ("System", &FOLDERID_System),
        ("Downloads", &FOLDERID_Downloads),
        ("Desktop", &FOLDERID_Desktop),
        ("Documents", &FOLDERID_Documents),
        ("UserProfile", &FOLDERID_Profile),
        ("StartMenu", &FOLDERID_StartMenu),
        ("CommonStartMenu", &FOLDERID_CommonStartMenu),
    ];
    let mut folders: Vec<KnownFolder> = table
        .iter()
        .map(|(name, id)| KnownFolder {
            name: (*name).to_owned(),
            path: resolve(id),
        })
        .collect();
    folders.push(KnownFolder {
        name: "Temp".to_owned(),
        path: std::env::temp_dir().to_str().map(str::to_owned),
    });
    folders
}

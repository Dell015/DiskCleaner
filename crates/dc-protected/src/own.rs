//! Which places count as the person's own, where they may remove things themselves ("Your call"
//! in B9). Text only, no disk access. Outside these places only DiskCleaner's own findings can
//! be removed. This is used by both the rules that decide the group (`dc-api`) and the removal
//! engine's last check (`dc-quarantine`), so the two cannot disagree.

use crate::normalize::normalize;

/// Top folders of a drive that belong to Windows or to installed programs: nothing below them is
/// ever the person's own.
pub const SYSTEM_TOP: [&str; 17] = [
    "windows",
    "windows.old",
    "program files",
    "program files (x86)",
    "programdata",
    "system volume information",
    "recovery",
    "boot",
    "efi",
    "perflogs",
    "config.msi",
    "msocache",
    "documents and settings",
    "intel",
    "amd",
    "nvidia",
    "drivers",
];

/// Profiles that are not a person's: Windows' templates and shared links.
const NOT_A_PERSON: [&str; 4] = ["default", "default user", "all users", "defaultapppool"];

/// `None` if `path` is in one of the person's own places; otherwise why not, in plain words.
/// Anything that cannot be understood is refused.
#[must_use]
pub fn not_your_own(path: &str) -> Option<&'static str> {
    let Ok(n) = normalize(path) else {
        return Some("its path cannot be understood");
    };
    let parts = &n.parts;
    let Some(first) = parts.first() else {
        return Some("it is a whole drive");
    };
    if SYSTEM_TOP.contains(&first.as_str()) || first.starts_with('$') {
        return Some("it is in a Windows or program folder");
    }
    if first == "users" {
        match parts.len() {
            1 => return Some("it holds every user profile"),
            2 => return Some("it is a whole user profile"),
            _ => {}
        }
        if NOT_A_PERSON.contains(&parts[1].as_str()) {
            return Some("it is in a profile Windows keeps for itself");
        }
        let folder = parts[2].as_str();
        if folder.starts_with('.') || folder == "appdata" || folder.starts_with("ntuser") {
            return Some("it holds settings of apps and Windows for this user");
        }
        if folder.starts_with("onedrive") {
            return Some("it is in OneDrive: removing it here would also delete it from OneDrive");
        }
    }
    if parts.iter().any(|p| p == "appdata") {
        return Some("it holds settings of apps");
    }
    if parts.iter().any(|p| p.starts_with("onedrive")) {
        return Some("it is in OneDrive: removing it here would also delete it from OneDrive");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn your_own_places_and_everything_else() {
        for own in [
            r"C:\Users\me\Downloads\a.zip",
            r"C:\Users\me\Saved Games\x",
            r"C:\Tools\thing",
            r"E:\Movies\old",
            r"E:\Users\backup-of-me\x",
        ] {
            assert_eq!(not_your_own(own), None, "{own}");
        }
        for never in [
            r"C:\Program Files\SomeTool",
            r"C:\Program Files (x86)\Game\x",
            r"C:\ProgramData\Thing",
            r"C:\Windows\System32",
            r"C:\$Recycle.Bin\x",
            r"C:\Users",
            r"C:\Users\me",
            r"C:\Users\Default\Desktop",
            r"C:\Users\me\AppData\Local\x",
            r"C:\Users\me\.ssh",
            r"C:\Users\me\NTUSER.DAT",
            r"C:\Users\me\OneDrive - Work\a",
            r"E:\stuff\AppData\x",
            r"C:\",
            r"\\server\share\x",
            "relative",
            "/tmp/x",
        ] {
            assert!(not_your_own(never).is_some(), "{never}");
        }
    }
}

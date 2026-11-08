//! Pure text parsers for registry values. Registry data is untrusted: quotes, arguments,
//! icon indexes, environment variables and plain garbage all occur. Nothing here touches the
//! disk, and nothing panics on any input (property-tested).

use crate::model::{FolderOrigin, KnownFolder, RawUninstallEntry};

/// A comparison key for a Windows path: lower case, backslashes, no trailing separator
/// (except a drive root, which keeps it).
pub fn path_key(path: &str) -> String {
    let mut key = path.trim().replace('/', "\\").to_lowercase();
    while key.ends_with('\\') && key.len() > 3 {
        key.pop();
    }
    if key.len() == 2 && key.ends_with(':') {
        key.push('\\');
    }
    key
}

/// Replace `%NAME%` with the value from `lookup`. Unknown names are left as written.
pub fn expand_env(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match (!name.is_empty()).then(|| lookup(name)).flatten() {
                    Some(value) => {
                        out.push_str(&value);
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push('%');
                        rest = after;
                    }
                }
            }
            None => {
                out.push('%');
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drop a trailing `,<number>` icon index.
fn strip_icon_index(text: &str) -> &str {
    match text.rsplit_once(',') {
        Some((head, tail)) if tail.trim().parse::<i32>().is_ok() => head.trim_end(),
        _ => text,
    }
}

/// The file path at the start of a command line or icon reference: handles quotes, arguments
/// and `,index` suffixes. Returns `None` when nothing usable is there.
pub fn path_from_command(text: &str) -> Option<String> {
    let text = text.trim().trim_matches('\0').trim();
    if text.is_empty() {
        return None;
    }
    let found = if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        strip_icon_index(quoted[..end].trim()).to_owned()
    } else {
        let lower = text.to_ascii_lowercase();
        let mut best: Option<usize> = None;
        for ext in [".exe", ".ico", ".dll"] {
            for (at, _) in lower.match_indices(ext) {
                let end = at + ext.len();
                let boundary = lower[end..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || c == ',');
                if boundary {
                    best = Some(best.map_or(end, |b| b.min(end)));
                    break;
                }
            }
        }
        match best {
            Some(end) => text[..end].to_owned(),
            None => strip_icon_index(text).to_owned(),
        }
    };
    let found = found.trim().to_owned();
    (!found.is_empty()).then_some(found)
}

/// Whether the text is an absolute drive path (`C:\...`) or UNC path (`\\server\...`).
pub fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    let unc = bytes.len() > 2 && bytes[0] == b'\\' && bytes[1] == b'\\';
    drive || unc
}

/// The folder containing a file path, or `None` if there is none.
pub fn parent_folder(path: &str) -> Option<String> {
    let path = path.trim().replace('/', "\\");
    let (head, _) = path.trim_end_matches('\\').rsplit_once('\\')?;
    let mut head = head.to_owned();
    if head.len() == 2 && head.ends_with(':') {
        head.push('\\');
    }
    (!head.is_empty()).then_some(head)
}

/// Clean a folder path read from the registry: trim, drop quotes, use backslashes, remove a
/// trailing separator. Returns `None` unless the result is an absolute path with no illegal
/// characters.
pub fn normalize_folder(text: &str) -> Option<String> {
    let text = text
        .trim()
        .trim_matches('\0')
        .trim()
        .trim_matches('"')
        .trim();
    if text.is_empty()
        || text
            .chars()
            .any(|c| c.is_control() || "<>|\"*?".contains(c))
    {
        return None;
    }
    let mut folder = text.replace('/', "\\");
    while folder.ends_with('\\') && folder.len() > 3 {
        folder.pop();
    }
    is_absolute(&folder).then_some(folder)
}

/// Folder names that hold installer leftovers (cached setup files), never the app itself.
const INSTALLER_CACHES: [&str; 2] = ["package cache", "installshield installation information"];

/// A folder is too broad (or too generic) to be one app's install folder: a drive root, a known
/// folder itself, anything inside the Windows or System folders, or an installer cache.
pub fn is_too_broad(folder: &str, known: &[KnownFolder]) -> bool {
    let key = path_key(folder);
    if key.len() <= 3 || key.split('\\').any(|part| INSTALLER_CACHES.contains(&part)) {
        return true;
    }
    for entry in known {
        let Some(path) = &entry.path else { continue };
        let other = path_key(path);
        if key == other {
            return true;
        }
        let system_area = matches!(entry.name.as_str(), "Windows" | "System");
        if system_area && key.starts_with(&format!("{other}\\")) {
            return true;
        }
    }
    false
}

/// Programs that appear in uninstall commands but say nothing about where an app lives.
const GENERIC_LAUNCHERS: [&str; 6] = [
    "msiexec.exe",
    "rundll32.exe",
    "cmd.exe",
    "regsvr32.exe",
    "powershell.exe",
    "wscript.exe",
];

fn file_name_lower(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Work out an app's install folder: `InstallLocation`, then the folder of `DisplayIcon`, then
/// the folder of the program in `UninstallString`. Candidates that are not absolute, are too
/// broad, or belong to generic launchers are skipped. The origin is returned so a guess is
/// never presented as a fact.
pub fn derive_install_folder(
    entry: &RawUninstallEntry,
    known: &[KnownFolder],
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<(String, FolderOrigin)> {
    let accept = |folder: Option<String>| {
        folder
            .and_then(|f| normalize_folder(&f))
            .filter(|f| !is_too_broad(f, known))
    };

    if let Some(location) = &entry.install_location {
        if let Some(folder) = accept(Some(expand_env(location, env))) {
            return Some((folder, FolderOrigin::InstallLocation));
        }
    }
    let from_command = |text: &Option<String>, skip_launchers: bool| {
        let file = expand_env(&path_from_command(text.as_deref()?)?, env);
        if !is_absolute(&file)
            || (skip_launchers && GENERIC_LAUNCHERS.contains(&file_name_lower(&file).as_str()))
        {
            return None;
        }
        accept(parent_folder(&file))
    };
    if let Some(folder) = from_command(&entry.display_icon, false) {
        return Some((folder, FolderOrigin::DisplayIcon));
    }
    if let Some(folder) = from_command(&entry.uninstall_string, true) {
        return Some((folder, FolderOrigin::UninstallString));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn env(name: &str) -> Option<String> {
        match name.to_ascii_lowercase().as_str() {
            "programfiles" => Some(r"C:\Program Files".into()),
            "localappdata" => Some(r"C:\Users\Test\AppData\Local".into()),
            _ => None,
        }
    }

    fn known() -> Vec<KnownFolder> {
        [
            ("Windows", r"C:\Windows"),
            ("ProgramFiles", r"C:\Program Files"),
            ("LocalAppData", r"C:\Users\Test\AppData\Local"),
        ]
        .into_iter()
        .map(|(name, path)| KnownFolder {
            name: name.into(),
            path: Some(path.into()),
        })
        .collect()
    }

    #[test]
    fn path_keys_ignore_case_separators_and_trailing_slash() {
        assert_eq!(path_key(r"C:/Program Files/A/"), r"c:\program files\a");
        assert_eq!(path_key("C:"), r"c:\");
        assert_eq!(path_key(r"C:\"), r"c:\");
    }

    #[test]
    fn environment_variables_expand_and_unknown_ones_stay() {
        assert_eq!(expand_env(r"%ProgramFiles%\X", &env), r"C:\Program Files\X");
        assert_eq!(expand_env(r"%NOPE%\X", &env), r"%NOPE%\X");
        assert_eq!(expand_env("100%", &env), "100%");
        assert_eq!(expand_env("%%", &env), "%%");
        assert_eq!(
            expand_env("a%b%c%programfiles%", &env),
            r"a%b%cC:\Program Files"
        );
    }

    #[test]
    fn command_lines_yield_the_program_path() {
        let cases = [
            (
                r#""C:\Program Files\A\unins000.exe" /SILENT"#,
                r"C:\Program Files\A\unins000.exe",
            ),
            (
                r"C:\Program Files\A\unins000.exe /S",
                r"C:\Program Files\A\unins000.exe",
            ),
            (
                r"C:\Program Files\A\app.exe,0",
                r"C:\Program Files\A\app.exe",
            ),
            (r#""C:\A\app.exe",-12"#, r"C:\A\app.exe"),
            (r"C:\A\icon.ico", r"C:\A\icon.ico"),
            (r"C:\A\app.exe", r"C:\A\app.exe"),
        ];
        for (input, want) in cases {
            assert_eq!(path_from_command(input).as_deref(), Some(want), "{input}");
        }
        assert_eq!(path_from_command(""), None);
        assert_eq!(path_from_command("   "), None);
        assert_eq!(path_from_command(r#""unterminated"#), None);
    }

    #[test]
    fn absolute_paths_are_recognised() {
        assert!(is_absolute(r"C:\x"));
        assert!(is_absolute(r"\\server\share"));
        assert!(!is_absolute(r"x\y"));
        assert!(!is_absolute(r"%ProgramFiles%\x"));
        assert!(!is_absolute("C:"));
    }

    #[test]
    fn parent_and_normalize_behave() {
        assert_eq!(parent_folder(r"C:\A\b.exe").as_deref(), Some(r"C:\A"));
        assert_eq!(parent_folder(r"C:\b.exe").as_deref(), Some(r"C:\"));
        assert_eq!(parent_folder("b.exe"), None);
        assert_eq!(
            normalize_folder(r#" "C:/A/B/" "#).as_deref(),
            Some(r"C:\A\B")
        );
        assert_eq!(normalize_folder("relative"), None);
        assert_eq!(normalize_folder("C:\\A|B"), None);
        assert_eq!(normalize_folder(""), None);
    }

    #[test]
    fn broad_folders_are_rejected() {
        let known = known();
        assert!(is_too_broad(r"C:\", &known));
        assert!(is_too_broad(r"c:\program files", &known));
        assert!(is_too_broad(r"C:\Windows\Installer\{ABC}", &known));
        assert!(is_too_broad(r"C:\ProgramData\Package Cache\{GUID}", &known));
        assert!(is_too_broad(
            r"C:\Program Files (x86)\InstallShield Installation Information\{G}",
            &known
        ));
        assert!(!is_too_broad(r"C:\Program Files\App", &known));
    }

    fn entry() -> RawUninstallEntry {
        RawUninstallEntry {
            key: "k".into(),
            display_name: Some("App".into()),
            ..RawUninstallEntry::default()
        }
    }

    #[test]
    fn install_folder_prefers_install_location() {
        let mut e = entry();
        e.install_location = Some(r"%ProgramFiles%\App\".into());
        e.display_icon = Some(r"C:\Other\x.exe,0".into());
        let got = derive_install_folder(&e, &known(), &env).unwrap();
        assert_eq!(
            got,
            (
                r"C:\Program Files\App".to_owned(),
                FolderOrigin::InstallLocation
            )
        );
    }

    #[test]
    fn install_folder_falls_back_to_icon_then_uninstaller() {
        let mut e = entry();
        e.install_location = Some(String::new());
        e.display_icon = Some(r"C:\Program Files\App\app.exe,0".into());
        let got = derive_install_folder(&e, &known(), &env).unwrap();
        assert_eq!(got.1, FolderOrigin::DisplayIcon);

        let mut e = entry();
        e.uninstall_string = Some(r#""C:\Program Files\App\unins000.exe" /S"#.into());
        let got = derive_install_folder(&e, &known(), &env).unwrap();
        assert_eq!(
            got,
            (
                r"C:\Program Files\App".to_owned(),
                FolderOrigin::UninstallString
            )
        );
    }

    #[test]
    fn useless_candidates_give_no_folder() {
        let mut e = entry();
        e.install_location = Some(r"C:\Program Files".into());
        e.display_icon = Some(r"C:\Windows\Installer\{G}\icon.ico".into());
        e.uninstall_string = Some("MsiExec.exe /X{GUID}".into());
        assert_eq!(derive_install_folder(&e, &known(), &env), None);

        let mut e = entry();
        e.uninstall_string = Some(r"C:\Windows\System32\msiexec.exe /X{G}".into());
        assert_eq!(derive_install_folder(&e, &known(), &env), None);
        let mut e = entry();
        e.uninstall_string = Some(r"C:\Tools\msiexec.exe /X{G}".into());
        assert_eq!(derive_install_folder(&e, &known(), &env), None);
    }

    proptest! {
        #[test]
        fn parsers_never_panic(text in ".*") {
            let _ = path_key(&text);
            let _ = expand_env(&text, &env);
            let _ = path_from_command(&text);
            let _ = is_absolute(&text);
            let _ = parent_folder(&text);
            let _ = normalize_folder(&text);
            let mut e = entry();
            e.install_location = Some(text.clone());
            e.display_icon = Some(text.clone());
            e.uninstall_string = Some(text);
            let _ = derive_install_folder(&e, &known(), &env);
        }
    }
}

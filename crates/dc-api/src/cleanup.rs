//! Which group a picked path is in for removing (design: `docs/phases/b9-cleaning/design.md`,
//! section 2): Suggested (a Safe or Probably safe finding), Your call (the person's own places),
//! or Never. Decided from the latest scan; the removal engine then checks the disk again.
//! Read-only.

use serde::Serialize;

use crate::inspect::Inspector;

/// The group of a picked path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "group", rename_all = "snake_case")]
pub enum Eligibility {
    /// A finding marked Safe.
    Safe {
        /// Title of the finding.
        title: String,
        /// Reason id.
        reason: String,
    },
    /// A finding marked Probably safe (needs its own tick).
    ProbablySafe {
        /// Title of the finding.
        title: String,
        /// Reason id.
        reason: String,
    },
    /// The person's own files (needs the stronger confirmation).
    YourCall,
    /// Cannot be removed.
    Never {
        /// Why, in plain words.
        why: String,
    },
}

const NEVER_REASONS: [&str; 2] = ["browser_profile_data", "wallet_or_vault"];
/// Top folders of a drive that belong to Windows or to installed programs.
const SYSTEM_TOP: [&str; 11] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
    "efi",
    "$windows.~bt",
    "$windows.~ws",
];

fn never(why: &str) -> Eligibility {
    Eligibility::Never {
        why: why.to_owned(),
    }
}

/// The names from the scanned drive or folder down to `i` (the scanned root first).
fn chain(ins: &Inspector, i: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut at = Some(i);
    while let Some(a) = at {
        if a == 0 {
            break;
        }
        out.push(a);
        at = ins.parent(a);
    }
    out.reverse();
    out
}

/// The group of `path` in the scan `ins`.
#[must_use]
pub fn eligibility(ins: &Inspector, path: &str) -> Eligibility {
    let Some(i) = ins.find(path) else {
        return never("it is not in the latest scan");
    };
    let links = ins.flags(i) & (crate::view::flag::LINK | crate::view::flag::CLOUD);
    if links != 0 {
        return never("it is a link or a cloud-only file");
    }
    let path_chain = chain(ins, i);
    if path_chain.len() <= 1 {
        return never("it is the whole scanned drive or folder");
    }
    // What the checks say comes first: refusals and "never" verdicts win over everything.
    if let Some((m, _)) = ins.mark(i) {
        if m.source == "refused" {
            return never(&format!("DiskCleaner refuses it: {}", m.detail));
        }
        if NEVER_REASONS.contains(&m.reason.as_str()) {
            return never("it holds browser logins or keys; losing it can lose them for good");
        }
        if m.offered {
            return match m.risk.as_str() {
                "safe" => Eligibility::Safe {
                    title: m.title.clone(),
                    reason: m.reason.clone(),
                },
                "probably_safe" => Eligibility::ProbablySafe {
                    title: m.title.clone(),
                    reason: m.reason.clone(),
                },
                _ => never("its verdict is Don't touch"),
            };
        }
    }
    if let Some((o, _, _)) = ins.owner(i) {
        return match o.kind.as_str() {
            "windows" => never("it belongs to Windows"),
            _ if ins.path(i).to_lowercase().get(2..).is_some_and(|rest| rest.starts_with("\\program files\\") || rest.starts_with("\\program files (x86)\\")) || o.folder.as_deref().is_some_and(|f| {
                let p = ins.path(i).to_lowercase();
                let f = f.to_lowercase();
                let f = f.trim_end_matches('\\');
                p == f || p.starts_with(&format!("{f}\\"))
            }) => never(&format!("it is part of the installed app \"{}\": remove the app with its own uninstaller", o.name)),
            _ => never(&format!("it belongs to \"{}\" (app data); removing it can break or reset that app", o.name)),
        };
    }
    // The person's own places.
    let names: Vec<String> = path_chain
        .iter()
        .skip(1)
        .map(|&a| ins.name(a).to_lowercase())
        .collect();
    let root_is_drive = {
        let r = ins.name(path_chain[0]);
        r.len() <= 3 && r.as_bytes().get(1) == Some(&b':')
    };
    // Below the scanned root: the first name is the top folder when the root is a drive.
    let first = names.first().map(String::as_str).unwrap_or("");
    if root_is_drive && SYSTEM_TOP.contains(&first) {
        return never("it is in a Windows or program folder");
    }
    let full = ins.path(i).to_lowercase().replace('/', "\\");
    let users_at = full.find("\\users\\");
    if let Some(at) = users_at {
        // ...\Users\<name>\<folder>\...
        let rest: Vec<&str> = full[at + 7..].split('\\').collect();
        match rest.as_slice() {
            [] | [_] => return never("it is a whole user profile"),
            [_, folder, ..] => {
                if folder.starts_with('.') || *folder == "appdata" || folder.starts_with("ntuser") {
                    return never("it holds settings of apps and Windows for this user");
                }
                if folder.starts_with("onedrive") {
                    return never(
                        "it is in OneDrive: removing it here would also delete it from OneDrive",
                    );
                }
            }
        }
    }
    if full.contains("\\appdata\\") || full.contains("\\onedrive") {
        return never("it holds settings or synced files of apps");
    }
    Eligibility::YourCall
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ins() -> Inspector {
        // name, parent, kind
        let nodes: &[(&str, i64, u8)] = &[
            ("This PC", -1, 1),
            ("C:\\", 0, 1),
            ("Users", 1, 1),
            ("me", 2, 1),
            ("Downloads", 3, 1),
            ("a.zip", 4, 0),
            ("AppData", 3, 1),
            ("Local", 6, 1),
            ("Temp", 7, 1),
            ("Windows", 1, 1),
            ("Program Files", 1, 1),
            ("Game", 10, 1),
            ("OneDrive", 3, 1),
            ("doc.docx", 12, 0),
            (".vscode", 3, 1),
            ("Tools", 1, 1),
            ("Chrome", 7, 1),
            ("Profile", 16, 1),
            ("Saved Games", 3, 1),
        ];
        let col = |f: &dyn Fn(&(&str, i64, u8)) -> serde_json::Value| {
            serde_json::Value::Array(nodes.iter().map(f).collect())
        };
        let v = json!({
            "schema": "dc-view", "schema_version": 1, "roots": ["C:\\"],
            "nodes": { "name": col(&|n| json!(n.0)), "parent": col(&|n| json!(n.1)), "kind": col(&|n| json!(n.2)),
                       "size": col(&|_| json!(1)), "mtime": col(&|_| json!(0)), "flags": col(&|_| json!(0)) },
            "owners": [{ "name": "Game", "kind": "app", "folder": "C:\\Program Files\\Game" }, { "name": "Windows", "kind": "windows" }, { "name": "Chrome", "kind": "app" }],
            "assignments": [[11, 0, 2], [9, 1, 2], [16, 2, 1]],
            "marks": [
                { "node": 8, "risk": "safe", "source": "rule", "title": "Temp", "reason": "temp_files", "detail": "", "offered": true, "bytes": 1 },
                { "node": 17, "risk": "dont_touch", "source": "rule", "title": "Profile", "reason": "browser_profile_data", "detail": "", "offered": false, "bytes": 1 }
            ]
        });
        Inspector::from_json(&v.to_string()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn group(path: &str) -> String {
        match eligibility(&ins(), path) {
            Eligibility::Safe { .. } => "safe".into(),
            Eligibility::ProbablySafe { .. } => "probably_safe".into(),
            Eligibility::YourCall => "your_call".into(),
            Eligibility::Never { why } => format!("never: {why}"),
        }
    }

    #[test]
    fn groups_follow_the_design() {
        assert_eq!(group(r"C:\Users\me\AppData\Local\Temp"), "safe");
        assert_eq!(group(r"C:\Users\me\Downloads\a.zip"), "your_call");
        assert_eq!(group(r"C:\Users\me\Downloads"), "your_call");
        assert_eq!(group(r"C:\Users\me\Saved Games"), "your_call");
        assert_eq!(group(r"C:\Tools"), "your_call");
        assert!(group(r"C:\Windows").starts_with("never: it belongs to Windows"));
        assert!(group(r"C:\Program Files\Game").contains("uninstaller"));
        assert!(group(r"C:\Program Files").starts_with("never"));
        assert!(group(r"C:\Users\me\OneDrive\doc.docx").contains("OneDrive"));
        assert!(group(r"C:\Users\me\.vscode").starts_with("never"));
        assert!(group(r"C:\Users\me\AppData").starts_with("never"));
        assert!(group(r"C:\Users\me").contains("whole user profile"));
        assert!(group(r"C:\Users\me\AppData\Local\Chrome").contains("app data"));
        assert!(group(r"C:\Users\me\AppData\Local\Chrome\Profile").contains("browser logins"));
        assert!(group(r"C:\").starts_with("never"));
        assert!(group(r"D:\nope").contains("not in the latest scan"));
    }
}

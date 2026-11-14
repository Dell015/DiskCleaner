//! Which group a picked path is in for removing (design: `docs/phases/b9-cleaning/design.md`,
//! section 2): Suggested (a Safe or Probably safe finding), Your call (the person's own places),
//! or Never. Decided from the latest scan; the removal engine then checks the disk again.
//! Read-only.

use std::sync::OnceLock;

use dc_protected::{not_your_own, FakeResolver, ProtectedList};
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

/// The compiled-in protected list, checked on the text of a path only (the removal engine checks
/// it again on the disk). The engine refuses everything on it, so it is Never here too.
fn protected(path: &str) -> Option<String> {
    static LIST: OnceLock<ProtectedList> = OnceLock::new();
    let verdict = LIST
        .get_or_init(ProtectedList::builtin)
        .check(path, &FakeResolver::new());
    verdict.protected.then(|| {
        verdict.reasons.first().map_or_else(
            || "DiskCleaner protects it".to_owned(),
            |r| format!("DiskCleaner protects it: {}", r.description),
        )
    })
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
    if let Some(why) = protected(&ins.path(i)) {
        return never(&why);
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
    // The person's own places: the same rule the removal engine checks last, on the full path
    // (not relative to the scanned folder: scanning C:\Program Files itself changes nothing).
    match not_your_own(&ins.path(i)) {
        Some(why) => never(why),
        None => Eligibility::YourCall,
    }
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
            ("Documents", 3, 1),
            ("cv.docx", 19, 0),
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
        assert!(group(r"C:\Windows").starts_with("never: DiskCleaner protects it"));
        assert!(group(r"C:\Program Files\Game").contains("uninstaller"));
        assert!(group(r"C:\Program Files").starts_with("never"));
        assert!(group(r"C:\Users\me\OneDrive\doc.docx").contains("OneDrive"));
        assert!(group(r"C:\Users\me\.vscode").starts_with("never"));
        assert!(group(r"C:\Users\me\AppData").starts_with("never"));
        assert!(group(r"C:\Users\me").starts_with("never"));
        assert!(group(r"C:\Users\me\AppData\Local\Chrome").contains("app data"));
        assert!(group(r"C:\Users\me\AppData\Local\Chrome\Profile").contains("browser logins"));
        assert!(group(r"C:\").starts_with("never"));
        assert!(group(r"D:\nope").contains("not in the latest scan"));
        // Finding 7: your Documents, Desktop, Pictures, Music and Videos stay protected.
        assert!(group(r"C:\Users\me\Documents\cv.docx").contains("your documents"));
    }

    /// A scan of `root` (a folder, not a drive) holding `child`, with no owner and no finding.
    fn folder_scan(root: &str, child: &str) -> Inspector {
        let v = json!({
            "schema": "dc-view", "schema_version": 1, "roots": [root],
            "nodes": { "name": ["This PC", root, child], "parent": [-1, 0, 1], "kind": [1, 1, 1],
                       "size": [1, 1, 1], "mtime": [0, 0, 0], "flags": [0, 0, 0] },
            "owners": [], "assignments": [], "marks": []
        });
        Inspector::from_json(&v.to_string()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn program_folders_scanned_as_the_root_are_still_never_your_own() {
        // Finding 3 of the B9 review.
        for (root, child) in [
            (r"C:\Program Files", "SomeTool"),
            (r"C:\Program Files (x86)", "Game"),
            (r"C:\ProgramData", "Thing"),
            (r"C:\Windows", "Temp"),
            (r"D:\Program Files", "Tool"),
        ] {
            let ins = folder_scan(root, child);
            let path = format!("{root}\\{child}");
            assert!(
                matches!(eligibility(&ins, &path), Eligibility::Never { .. }),
                "{path}"
            );
        }
        let ins = folder_scan(r"E:\Mess", "old");
        assert_eq!(eligibility(&ins, r"E:\Mess\old"), Eligibility::YourCall);
    }

    #[test]
    fn the_rules_and_the_removal_engine_agree_on_your_own_files() {
        // Anything "your call" here passes the engine's text checks: the protected list and
        // `not_your_own` (finding 7: no more "offered, then always skipped").
        let list = ProtectedList::builtin();
        let ins = ins();
        let mut checked = 0;
        for i in 1..ins.len() {
            let path = ins.path(i as u32);
            if eligibility(&ins, &path) == Eligibility::YourCall {
                checked += 1;
                assert!(!list.check(&path, &FakeResolver::new()).protected, "{path}");
                assert_eq!(not_your_own(&path), None, "{path}");
            }
        }
        assert!(checked >= 3);
    }
}

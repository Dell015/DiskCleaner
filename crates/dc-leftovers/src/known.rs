//! The built-in list of well-known apps (data, in `packs/known_apps.toml`).
//!
//! The list can only add evidence. It is validated when loaded; a list with any problem is
//! refused as a whole and the program then simply has no list (no extra findings).

use std::collections::{HashMap, HashSet};

use dc_attrib::normalize;
use serde::Deserialize;

/// Folder names shorter than this (normalized) never match.
pub const MIN_NAME_LEN: usize = 4;

/// The text of the built-in list.
pub const BUILTIN_TEXT: &str = include_str!("../packs/known_apps.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    schema: u32,
    #[serde(default)]
    app: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    name: String,
    names: Vec<String>,
    #[serde(default)]
    app_prefixes: Vec<String>,
    #[serde(default)]
    report_only: bool,
    examples_match: Vec<String>,
    examples_no_match: Vec<String>,
}

/// One well-known app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownApp {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Normalized folder names that belong to the app.
    pub names: Vec<String>,
    /// Normalized prefixes of installed app names that mean "this app is still installed".
    pub app_prefixes: Vec<String>,
    /// Shown but never offered (can hold games or other wanted data).
    pub report_only: bool,
}

/// The validated list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownApps {
    apps: Vec<KnownApp>,
    by_name: HashMap<String, usize>,
}

fn last_component(path: &str) -> &str {
    path.trim_end_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or("")
}

impl KnownApps {
    /// No list at all (no extra evidence).
    #[must_use]
    pub fn none() -> KnownApps {
        KnownApps::default()
    }

    /// The built-in list. If it were ever invalid (a test makes sure it is not), it is empty.
    #[must_use]
    pub fn builtin() -> KnownApps {
        KnownApps::from_text(BUILTIN_TEXT).unwrap_or_default()
    }

    /// Parse and validate a list. All problems are returned together.
    pub fn from_text(text: &str) -> Result<KnownApps, Vec<String>> {
        let file: PackFile = toml::from_str(text).map_err(|e| vec![format!("not valid: {e}")])?;
        let mut problems: Vec<String> = Vec::new();
        if file.schema != 1 {
            problems.push(format!("unsupported schema {}", file.schema));
        }
        let mut apps: Vec<KnownApp> = Vec::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        let mut ids: HashSet<String> = HashSet::new();
        for entry in &file.app {
            if entry.id.trim().is_empty() || entry.name.trim().is_empty() {
                problems.push("an entry has an empty id or name".to_owned());
                continue;
            }
            if !ids.insert(entry.id.clone()) {
                problems.push(format!("{}: duplicate id", entry.id));
            }
            let mut names: Vec<String> = Vec::new();
            for raw in &entry.names {
                let n = normalize(raw);
                if n.chars().count() < MIN_NAME_LEN {
                    problems.push(format!(
                        "{}: name {raw:?} is too short or generic",
                        entry.id
                    ));
                } else if !names.contains(&n) {
                    names.push(n);
                }
            }
            if names.is_empty() {
                problems.push(format!("{}: no usable names", entry.id));
            }
            let index = apps.len();
            for n in &names {
                if let Some(other) = by_name.insert(n.clone(), index) {
                    if other != index {
                        problems.push(format!("{}: name {n:?} belongs to two entries", entry.id));
                    }
                }
            }
            let prefixes: Vec<String> = entry
                .app_prefixes
                .iter()
                .map(|p| normalize(p))
                .filter(|p| !p.is_empty())
                .collect();
            if prefixes.is_empty() {
                problems.push(format!(
                    "{}: app_prefixes needed to tell if it is installed",
                    entry.id
                ));
            }
            if entry.examples_match.is_empty() || entry.examples_no_match.is_empty() {
                problems.push(format!("{}: examples are required", entry.id));
            }
            for example in &entry.examples_match {
                if !names.contains(&normalize(last_component(example))) {
                    problems.push(format!("{}: example {example} does not match", entry.id));
                }
            }
            for example in &entry.examples_no_match {
                if names.contains(&normalize(last_component(example))) {
                    problems.push(format!("{}: example {example} must not match", entry.id));
                }
            }
            apps.push(KnownApp {
                id: entry.id.clone(),
                name: entry.name.clone(),
                names,
                app_prefixes: prefixes,
                report_only: entry.report_only,
            });
        }
        if problems.is_empty() {
            Ok(KnownApps { apps, by_name })
        } else {
            Err(problems)
        }
    }

    /// The entries.
    #[must_use]
    pub fn apps(&self) -> &[KnownApp] {
        &self.apps
    }

    /// The entry for a folder name (exact match after normalization), if any.
    #[must_use]
    pub fn for_folder(&self, folder_name: &str) -> Option<&KnownApp> {
        let n = normalize(folder_name);
        self.by_name.get(&n).and_then(|&i| self.apps.get(i))
    }

    /// Is the app installed, judging by the normalized names of the installed apps?
    #[must_use]
    pub fn is_installed(&self, app: &KnownApp, installed: &[String]) -> bool {
        installed.iter().any(|name| {
            app.names.contains(name)
                || app
                    .app_prefixes
                    .iter()
                    .any(|p| name == p || name.starts_with(&format!("{p} ")))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_list_is_valid() {
        let list = KnownApps::from_text(BUILTIN_TEXT);
        assert!(list.is_ok(), "{:?}", list.err());
        let list = KnownApps::builtin();
        assert!(list.apps().len() >= 25);
        assert!(list.for_folder("McAfee").is_some());
        assert!(list.for_folder("mcafee").is_some());
        assert!(list.for_folder("McAfee Tools Extra").is_none());
    }

    #[test]
    fn installed_means_exact_or_prefix() {
        let list = KnownApps::builtin();
        let Some(mcafee) = list.for_folder("McAfee") else {
            return;
        };
        assert!(list.is_installed(mcafee, &["mcafee livesafe".to_owned()]));
        assert!(list.is_installed(mcafee, &["mcafee".to_owned()]));
        assert!(!list.is_installed(mcafee, &["mcafeex tools".to_owned()]));
        assert!(!list.is_installed(mcafee, &["norton".to_owned()]));
    }

    #[test]
    fn bad_lists_are_refused() {
        let bad = r#"schema = 1
[[app]]
id = "x"
name = "X"
names = ["ab"]
examples_match = ['C:\a\ab']
examples_no_match = ['C:\a\cd']
"#;
        let problems = KnownApps::from_text(bad).err().unwrap_or_default();
        assert!(problems.iter().any(|p| p.contains("too short")));
        assert!(problems.iter().any(|p| p.contains("app_prefixes")));
        assert!(KnownApps::from_text("schema = 2").is_err());
        assert!(KnownApps::from_text("nonsense").is_err());
    }
}

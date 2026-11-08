//! Labelling scanned directories with the app that owns them. A plain, case-insensitive match
//! of directory paths against install folders; the full app-to-files graph is Phase B3.

use std::path::Path;

use dc_core::{AppLabel, Kind, Tree};

use crate::model::{AppSnapshot, InstalledApp};
use crate::parse::path_key;

/// Apps that can own a folder: they have an install folder and are real apps, not updates or
/// nameless components.
fn eligible(app: &InstalledApp) -> bool {
    app.install_folder.is_some() && !app.flags.hidden_entry && !app.flags.update_or_patch
}

/// The scan root's path with any `\\?\` prefix removed, as text.
fn root_text(root: &Path) -> String {
    let text = root.to_string_lossy().into_owned();
    text.strip_prefix(r"\\?\")
        .map(str::to_owned)
        .unwrap_or(text)
}

/// Find the directories of `tree` (scanned from `root`) that are an app's install folder.
///
/// - A directory whose path equals an app's install folder is labelled with that app (every
///   app that shares the folder gets a label).
/// - If the scan root itself lies inside an app's folder, the root is labelled with the
///   app whose folder is the longest match.
///
/// Sizes are never touched; this only adds labels.
pub fn app_labels(tree: &Tree, root: &Path, snapshot: &AppSnapshot) -> Vec<AppLabel> {
    let apps: Vec<(&InstalledApp, String)> = snapshot
        .apps
        .iter()
        .filter(|app| eligible(app))
        .filter_map(|app| Some((app, path_key(app.install_folder.as_deref()?))))
        .collect();
    let root_text = root_text(root);
    let label = |node: u32, app: &InstalledApp| AppLabel {
        node,
        app: app.name.clone(),
        key: app.key.clone(),
    };

    let mut labels = Vec::new();
    for node in tree.nodes().iter().filter(|n| n.kind == Kind::Dir) {
        let relative = tree.relative_path(node.id).replace('/', "\\");
        let full = if relative.is_empty() {
            root_text.clone()
        } else {
            format!("{}\\{relative}", root_text.trim_end_matches('\\'))
        };
        let key = path_key(&full);
        for (app, folder) in &apps {
            if *folder == key {
                labels.push(label(node.id, app));
            }
        }
        if relative.is_empty() && !labels.iter().any(|l| l.node == node.id) {
            let owner = apps
                .iter()
                .filter(|(_, folder)| key.starts_with(&format!("{folder}\\")))
                .max_by_key(|(_, folder)| folder.len());
            if let Some((app, _)) = owner {
                labels.push(label(node.id, app));
            }
        }
    }
    labels.sort_by(|a, b| (a.node, &a.app, &a.key).cmp(&(b.node, &b.app, &b.key)));
    labels
}

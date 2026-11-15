//! Reading launchers' own records of installed games (read-only): files are opened for reading
//! only, at most `MAX_FILE` bytes each, and only in the launchers' known record folders.

use std::fs;
use std::path::Path;

use super::registry::{read_hoyoplay_games, steam_path};
use crate::launchers::{
    epic_item, riot_name, riot_settings, steam_libraries, steam_manifest, LauncherGame,
};
use crate::model::Problem;
use crate::source::SourceReport;

/// Launcher record files are small; anything bigger is not one.
const MAX_FILE: u64 = 1024 * 1024;
/// Safety valve: at most this many records per launcher.
const MAX_GAMES: usize = 5000;

fn read_small(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Files in `dir` whose names end with `suffix` (no links followed, sorted).
fn files_ending(dir: &Path, suffix: &str) -> Vec<std::path::PathBuf> {
    let mut out: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(suffix)
        })
        .map(|e| e.path())
        .take(MAX_GAMES)
        .collect();
    out.sort();
    out
}

fn steam(out: &mut Vec<LauncherGame>) {
    let Some(root) = steam_path() else { return };
    let root = Path::new(&root);
    let list = read_small(&root.join(r"steamapps\libraryfolders.vdf"))
        .or_else(|| read_small(&root.join(r"config\libraryfolders.vdf")));
    let mut libraries = list.as_deref().map(steam_libraries).unwrap_or_default();
    if libraries.is_empty() {
        libraries.push(root.to_string_lossy().into_owned());
    }
    for lib in libraries {
        let apps = Path::new(&lib).join("steamapps");
        for manifest in files_ending(&apps, ".acf") {
            let Some((id, name, dir)) = read_small(&manifest).as_deref().and_then(steam_manifest)
            else {
                continue;
            };
            out.push(LauncherGame {
                launcher: "steam".into(),
                id,
                name: Some(name),
                folder: apps.join("common").join(dir).to_string_lossy().into_owned(),
            });
        }
    }
}

fn epic(program_data: &Path, out: &mut Vec<LauncherGame>) {
    let dir = program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests");
    for item in files_ending(&dir, ".item") {
        if let Some((id, name, folder)) = read_small(&item).as_deref().and_then(epic_item) {
            out.push(LauncherGame {
                launcher: "epic".into(),
                id,
                name,
                folder,
            });
        }
    }
}

fn riot(program_data: &Path, out: &mut Vec<LauncherGame>) {
    let dir = program_data.join(r"Riot Games\Metadata");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten().take(MAX_GAMES) {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned(); // "valorant.live"
        let settings = entry.path().join(format!("{id}.product_settings.yaml"));
        if let Some(folder) = read_small(&settings).as_deref().and_then(riot_settings) {
            let product = id.split('.').next().unwrap_or_default();
            if product == "riot_client" {
                continue; // the launcher itself
            }
            out.push(LauncherGame {
                launcher: "riot".into(),
                name: Some(riot_name(product)),
                id,
                folder,
            });
        }
    }
}

/// Every game the launchers recorded on this PC.
pub fn read_launcher_games(program_data: Option<String>) -> SourceReport<Vec<LauncherGame>> {
    let mut items = Vec::new();
    steam(&mut items);
    for (id, folder) in read_hoyoplay_games() {
        items.push(LauncherGame {
            launcher: "hoyoplay".into(),
            id,
            name: None,
            folder,
        });
    }
    let mut problems = Vec::new();
    match program_data {
        Some(pd) => {
            epic(Path::new(&pd), &mut items);
            riot(Path::new(&pd), &mut items);
        }
        None => problems.push(Problem {
            source: "launchers".into(),
            message: "the ProgramData folder is not known; Epic and Riot games were not read"
                .into(),
        }),
    }
    SourceReport { items, problems }
}

//! Games that launchers installed, read from each launcher's own record (read-only): Steam's
//! library list and per-game manifests, Epic's `.item` manifests, Riot's product settings and
//! HoYoPlay's registry values. Windows' list of installed programs often misses where a game
//! really is (HoYoPlay records its own folder for Genshin Impact; Epic games are not listed at
//! all), so these records say where each game's files are.

use serde::{Deserialize, Serialize};

use crate::model::{AppFlags, InstalledApp, RegistrySource};

/// A game a launcher says it installed, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherGame {
    /// "steam", "epic", "riot" or "hoyoplay".
    pub launcher: String,
    /// The launcher's id for it (Steam app id, Epic app name, Riot product, HoYoPlay game id).
    pub id: String,
    /// Its name, when the record has one.
    pub name: Option<String>,
    /// The folder with its files.
    pub folder: String,
}

/// The launcher's name for people.
#[must_use]
pub fn launcher_name(id: &str) -> &'static str {
    match id {
        "steam" => "Steam",
        "epic" => "Epic Games",
        "riot" => "Riot Client",
        "hoyoplay" => "HoYoPlay",
        _ => "a game launcher",
    }
}

/// Quoted `"key" "value"` pairs of a Steam `.vdf` or `.acf` file, in order (with `\\` unescaped).
fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = Vec::new();
        let mut rest = line.trim();
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let mut end = None;
            let mut escaped = false;
            for (k, c) in after.char_indices() {
                match c {
                    '\\' if !escaped => escaped = true,
                    '"' if !escaped => {
                        end = Some(k);
                        break;
                    }
                    _ => escaped = false,
                }
            }
            let Some(end) = end else { break };
            parts.push(after[..end].replace("\\\\", "\\").replace("\\\"", "\""));
            rest = &after[end + 1..];
        }
        if let [k, v] = parts.as_slice() {
            out.push((k.to_ascii_lowercase(), v.clone()));
        }
    }
    out
}

/// The library folders listed in Steam's `libraryfolders.vdf`.
#[must_use]
pub fn steam_libraries(vdf: &str) -> Vec<String> {
    vdf_pairs(vdf)
        .into_iter()
        .filter(|(k, _)| k == "path")
        .map(|(_, v)| v.replace('/', "\\"))
        .collect()
}

/// (app id, name, install folder name) from a Steam `appmanifest_<id>.acf`.
#[must_use]
pub fn steam_manifest(acf: &str) -> Option<(String, String, String)> {
    let pairs = vdf_pairs(acf);
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let dir =
        get("installdir").filter(|d| !d.is_empty() && !d.contains(['\\', '/']) && d != "..")?;
    Some((
        get("appid")?,
        get("name").unwrap_or_else(|| dir.clone()),
        dir,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicItem {
    app_name: Option<String>,
    display_name: Option<String>,
    install_location: Option<String>,
    #[serde(rename = "bIsIncompleteInstall", default)]
    incomplete: bool,
}

/// (app name, display name, install folder) from an Epic `.item` manifest; nothing for an
/// unfinished install.
#[must_use]
pub fn epic_item(json: &str) -> Option<(String, Option<String>, String)> {
    let item: EpicItem = serde_json::from_str(json).ok()?;
    if item.incomplete {
        return None;
    }
    let folder = item.install_location.filter(|f| !f.trim().is_empty())?;
    Some((
        item.app_name.unwrap_or_default(),
        item.display_name,
        folder.replace('/', "\\"),
    ))
}

/// The game folder in a Riot `<product>.product_settings.yaml`.
#[must_use]
pub fn riot_settings(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("product_install_full_path:")?
            .trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'').trim();
        (!value.is_empty()).then(|| value.replace('/', "\\").trim_end_matches('\\').to_owned())
    })
}

/// Riot's product codes as people know them.
#[must_use]
pub fn riot_name(product: &str) -> String {
    match product {
        "valorant" => "VALORANT".into(),
        "league_of_legends" => "League of Legends".into(),
        "bacon" => "Legends of Runeterra".into(),
        "lion" => "2XKO".into(),
        other => other.replace('_', " "),
    }
}

/// Lower-case letters and digits only, for comparing names.
fn plain(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Does the launcher record `game` describe the installed app `app`?
fn same_game(app: &InstalledApp, game: &LauncherGame) -> bool {
    let id = game.id.to_ascii_lowercase();
    let key = app.key.to_ascii_lowercase();
    let uninstall = app
        .uninstall_command
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let by_record = match game.launcher.as_str() {
        "steam" => key == format!("steam app {id}"),
        "hoyoplay" => uninstall.contains(&format!("--uninstall_game={id}")) || key.starts_with(&id),
        "riot" => {
            let product = id.split('.').next().unwrap_or_default();
            key.contains(&id) || uninstall.contains(&format!("--uninstall-product={product}"))
        }
        _ => false,
    };
    by_record
        || game
            .name
            .as_deref()
            .is_some_and(|n| !plain(n).is_empty() && plain(n) == plain(&app.name))
}

/// Add what launchers recorded to the installed apps: a game Windows lists gets the launcher's
/// folder too; a game Windows does not list (Epic games) becomes an app of its own.
pub fn merge_launcher_games(apps: &mut Vec<InstalledApp>, games: &[LauncherGame]) {
    for game in games {
        if let Some(app) = apps.iter_mut().find(|a| same_game(a, game)) {
            if !app
                .launcher_folders
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&game.folder))
            {
                app.launcher_folders.push(game.folder.clone());
            }
            app.launcher.get_or_insert_with(|| game.launcher.clone());
            continue;
        }
        let name = game.name.clone().unwrap_or_else(|| game.id.clone());
        apps.push(InstalledApp {
            key: format!("launcher:{}:{}", game.launcher, game.id),
            source: RegistrySource::User,
            name,
            publisher: None,
            version: None,
            install_folder: None,
            folder_origin: None,
            display_icon: None,
            uninstall_command: None,
            estimated_size_kb: None,
            install_date: None,
            flags: AppFlags {
                no_install_folder: true,
                ..AppFlags::default()
            },
            main_exe: None,
            version_info: None,
            shortcuts: Vec::new(),
            signature: None,
            launcher_folders: vec![game.folder.clone()],
            launcher: Some(game.launcher.clone()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_library_list_and_manifests() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"325282089"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        assert_eq!(
            steam_libraries(vdf),
            [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );
        let acf = r#""AppState"
{
	"appid"		"1091500"
	"name"		"Cyberpunk 2077"
	"installdir"		"Cyberpunk 2077"
}"#;
        assert_eq!(
            steam_manifest(acf),
            Some((
                "1091500".into(),
                "Cyberpunk 2077".into(),
                "Cyberpunk 2077".into()
            ))
        );
        // A folder name that tries to leave the library is refused.
        assert_eq!(
            steam_manifest(r#""appid" "1" "installdir" "..\..\Windows""#),
            None
        );
        assert_eq!(steam_manifest(r#""appid" "1""#), None);
    }

    #[test]
    fn epic_riot_and_names() {
        let item = r#"{"AppName":"Fortnite","DisplayName":"Fortnite","InstallLocation":"D:/Epic Games/Fortnite","bIsIncompleteInstall":false}"#;
        assert_eq!(
            epic_item(item),
            Some((
                "Fortnite".into(),
                Some("Fortnite".into()),
                r"D:\Epic Games\Fortnite".into()
            ))
        );
        assert_eq!(
            epic_item(r#"{"InstallLocation":"D:/x","bIsIncompleteInstall":true}"#),
            None
        );
        assert_eq!(epic_item("not json"), None);
        let yaml = "product_install_full_path: \"C:/Riot Games/VALORANT/live\"\nproduct_install_root: \"C:/Riot Games\"\n";
        assert_eq!(
            riot_settings(yaml),
            Some(r"C:\Riot Games\VALORANT\live".into())
        );
        assert_eq!(riot_name("valorant"), "VALORANT");
        assert_eq!(riot_name("new_game"), "new game");
    }

    fn registered(key: &str, name: &str, uninstall: Option<&str>) -> InstalledApp {
        InstalledApp {
            key: key.into(),
            source: RegistrySource::Machine64,
            name: name.into(),
            publisher: None,
            version: None,
            install_folder: Some(r"C:\HoYoPlay".into()),
            folder_origin: None,
            display_icon: None,
            uninstall_command: uninstall.map(Into::into),
            estimated_size_kb: None,
            install_date: None,
            flags: AppFlags::default(),
            main_exe: None,
            version_info: None,
            shortcuts: Vec::new(),
            signature: None,
            launcher_folders: Vec::new(),
            launcher: None,
        }
    }

    #[test]
    fn launcher_records_join_the_apps_windows_lists_or_become_apps() {
        // The owner's PC: Genshin Impact is registered with HoYoPlay's folder.
        let mut apps = vec![
            registered(
                "hk4e_global_1_0_VYTpXlbWo8_production",
                "Genshin Impact",
                Some(r"C:\HoYoPlay\launcher.exe --uninstall_game=hk4e_global"),
            ),
            registered("Steam App 570", "Dota 2", None),
        ];
        let games = [
            LauncherGame {
                launcher: "hoyoplay".into(),
                id: "hk4e_global".into(),
                name: None,
                folder: r"C:\Games\Genshin Impact game".into(),
            },
            LauncherGame {
                launcher: "steam".into(),
                id: "570".into(),
                name: Some("Dota 2".into()),
                folder: r"D:\SteamLibrary\steamapps\common\dota 2 beta".into(),
            },
            LauncherGame {
                launcher: "epic".into(),
                id: "Fortnite".into(),
                name: Some("Fortnite".into()),
                folder: r"D:\Epic Games\Fortnite".into(),
            },
        ];
        merge_launcher_games(&mut apps, &games);
        merge_launcher_games(&mut apps, &games); // twice: nothing doubles
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0].launcher_folders, [r"C:\Games\Genshin Impact game"]);
        assert_eq!(apps[0].launcher.as_deref(), Some("hoyoplay"));
        assert_eq!(apps[1].launcher_folders.len(), 1);
        assert_eq!(apps[2].name, "Fortnite");
        assert_eq!(apps[2].key, "launcher:epic:Fortnite");
        assert_eq!(apps[2].launcher_folders, [r"D:\Epic Games\Fortnite"]);
    }
}

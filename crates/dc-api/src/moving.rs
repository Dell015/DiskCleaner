//! How an app can be moved to another drive (B11, D141; `docs/phases/b11-move-apps/design.md`
//! section 3): moved by Rhizome with a link (apps), moved without a link and then found again by
//! its launcher (HoYoPlay, Riot, Battle.net games), only through the launcher's own button
//! (Steam, Microsoft Store), or not at all. Decided from what Windows lists now and the latest
//! scan; the move engine (`dc_quarantine::mover`) checks the disk again. Read-only.

use serde::Serialize;

use crate::cleanup::{holds_hard_link, protected};
use crate::inspect::Inspector;

/// What Windows lists for the app right now.
#[derive(Debug, Clone, Default)]
pub struct AppNow {
    /// Display name.
    pub name: String,
    /// Publisher.
    pub publisher: Option<String>,
    /// Its registered install folder.
    pub folder: Option<String>,
    /// Folders a game launcher's own record names for it.
    pub launcher_folders: Vec<String>,
    /// The launcher that installed it ("steam", "epic", "riot", "hoyoplay", ...).
    pub launcher: Option<String>,
}

/// How this app can be moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum MoveHow {
    /// Rhizome moves the folder. `link`: a junction is left at the old place (apps); without one
    /// the person tells the launcher the new place (`steps`).
    Move {
        /// Leave a junction.
        link: bool,
        /// The folder to move.
        folder: String,
        /// Warnings to show before starting.
        warnings: Vec<String>,
        /// What the person does after the move (launcher games), in order.
        steps: Vec<String>,
    },
    /// Only the launcher or Windows can move it; these are its steps.
    Guide {
        /// Who moves it.
        by: String,
        /// The steps, in order.
        steps: Vec<String>,
    },
    /// Not moved.
    Never {
        /// Why, in plain words.
        why: String,
    },
}

fn never(why: impl Into<String>) -> MoveHow {
    MoveHow::Never { why: why.into() }
}

/// `C:\x\` and `c:/X` compare equal.
fn key(path: &str) -> String {
    let p = path
        .trim()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase();
    p.trim_end_matches('\\').to_owned()
}

/// Is `inner` the same as, or inside, `outer`?
fn within(inner: &str, outer: &str) -> bool {
    let (i, o) = (key(inner), key(outer));
    i == o || i.strip_prefix(&o).is_some_and(|r| r.starts_with('\\'))
}

/// Does a service or start-up command run a program inside `folder`? The program is the first
/// part of the command: a quoted path, or the text up to `.exe`.
#[must_use]
pub fn command_runs_from(command: &str, folder: &str) -> bool {
    let c = command.trim();
    let program = if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next().unwrap_or("")
    } else {
        let lower = c.to_lowercase();
        lower.find(".exe").map_or(c, |end| &c[..end + 4])
    };
    let program = program.trim_start_matches(r"\??\");
    !program.is_empty() && within(program, folder) && key(program) != key(folder)
}

/// Office and Visual Studio install shared parts all over Windows; Riot Vanguard is an
/// anti-cheat driver. Never moved (D141, plan).
fn never_by_name(name: &str, publisher: Option<&str>) -> Option<&'static str> {
    let n = name.to_lowercase();
    let microsoft = publisher.is_some_and(|p| p.to_lowercase().contains("microsoft"));
    if n.contains("vanguard") {
        return Some("Riot Vanguard is an anti-cheat driver; it stays where Riot installed it");
    }
    if microsoft
        && (n.starts_with("microsoft office")
            || n.starts_with("microsoft 365")
            || n.starts_with("office "))
    {
        return Some("Office installs shared parts all over Windows; use its installer to change where it is");
    }
    if microsoft && n.contains("visual studio") && !n.contains("visual studio code") {
        return Some(
            "Visual Studio installs shared parts all over Windows; use the Visual Studio Installer",
        );
    }
    None
}

/// Steps for the launchers Rhizome does not move games for, or moves and then guides.
fn launcher_steps(launcher: &str, game: &str) -> Option<(MoveHowKind, Vec<String>)> {
    let s = |v: &[&str]| {
        v.iter()
            .map(|x| (*x).replace("{game}", game))
            .collect::<Vec<_>>()
    };
    Some(match launcher {
        "steam" => (
            MoveHowKind::Guide("Steam"),
            s(&[
                "Open Steam and go to your Library.",
                "Right-click {game}, then Properties.",
                "Open Installed Files, then Move install folder.",
                "Pick the other drive's Steam library (or add one there), then Move.",
            ]),
        ),
        "store" => (
            MoveHowKind::Guide("Windows"),
            s(&[
                "Open Settings, then Apps, then Installed apps.",
                "Find {game}, click the three dots, then Move.",
                "Pick the other drive, then Move.",
            ]),
        ),
        "hoyoplay" => (
            MoveHowKind::Move,
            s(&[
                "Open HoYoPlay and pick {game}.",
                "Open the menu next to the Start button, then Locate game (or Game settings, Locate).",
                "Pick the folder Rhizome moved it to, then let HoYoPlay check the files.",
                "Start {game}. If it runs, come back and press It works.",
            ]),
        ),
        "riot" => (
            MoveHowKind::Move,
            s(&[
                "Open the Riot Client and pick {game}.",
                "Click Install, then \"Already installed?\" (or the folder icon) and pick the folder Rhizome moved it to.",
                "Let the Riot Client check the files. Riot Vanguard stays where it is.",
                "Start {game}. If it runs, come back and press It works.",
            ]),
        ),
        "ubisoft" => (
            MoveHowKind::Move,
            s(&[
                "Open Ubisoft Connect and go to Games, then pick {game}.",
                "Under the Download button, click Locate installed game, and pick the folder Rhizome moved it to.",
                "Let Ubisoft Connect check the files, then start {game}. If it runs, come back and press It works.",
            ]),
        ),
        "battlenet" => (
            MoveHowKind::Move,
            s(&[
                "Open Battle.net and pick {game}.",
                "Under the Install button, click Locate the game, and pick the folder Rhizome moved it to.",
                "Start {game}. If it runs, come back and press It works.",
            ]),
        ),
        "epic" | "ea" | "gog" => (
            MoveHowKind::Guide(match launcher {
                "epic" => "Epic Games Launcher",
                "ea" => "the EA app",
                _ => "GOG Galaxy",
            }),
            s(&[
                "This launcher cannot move installed games.",
                "To have {game} on the other drive: uninstall it from the launcher, then install it again and pick the other drive.",
                "Your saves usually stay (most games keep them in Documents or in the cloud); check the game's support page first.",
            ]),
        ),
        _ => return None,
    })
}

enum MoveHowKind {
    Move,
    Guide(&'static str),
}

/// How the app `app` (as Windows lists it now) can move, with `folder` being the folder the page
/// asked for. `background` is every background program (label, command) from Windows now.
#[must_use]
pub fn how_to_move(
    ins: &Inspector,
    app: &AppNow,
    folder: &str,
    all_apps: &[AppNow],
    background: &[(String, String)],
) -> MoveHow {
    if let Some(why) = never_by_name(&app.name, app.publisher.as_deref()) {
        return never(why);
    }
    let f = key(folder);
    if f.contains(r"\windowsapps\") || f.ends_with(r"\windowsapps") {
        let (_, steps) =
            launcher_steps("store", &app.name).unwrap_or((MoveHowKind::Move, Vec::new()));
        return MoveHow::Guide {
            by: "Windows".into(),
            steps,
        };
    }
    // The folder must be one Windows (or the app's launcher) names for this app right now.
    let registered = app
        .folder
        .iter()
        .chain(&app.launcher_folders)
        .any(|r| key(r) == f);
    if !registered {
        return never("it is not this app's registered folder as Windows lists it now; scan again");
    }
    let game = &app.name;
    if let Some(l) = app.launcher.as_deref() {
        if let Some((kind, steps)) = launcher_steps(l, game) {
            if let MoveHowKind::Guide(by) = kind {
                return MoveHow::Guide {
                    by: by.into(),
                    steps,
                };
            }
            return checked(ins, app, folder, all_apps, background, false, steps);
        }
    }
    checked(ins, app, folder, all_apps, background, true, Vec::new())
}

fn checked(
    ins: &Inspector,
    app: &AppNow,
    folder: &str,
    all_apps: &[AppNow],
    background: &[(String, String)],
    link: bool,
    steps: Vec<String>,
) -> MoveHow {
    // A drive's top, a user's folder, a known system place.
    let folder_key = key(folder);
    let parts: Vec<&str> = folder_key.split('\\').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return never("a drive's top folder cannot be moved");
    }
    if parts.len() <= 3 && parts.get(1) == Some(&"users") {
        return never("a user's own folder is not an app folder");
    }
    if let Some(why) = protected(folder) {
        return never(why);
    }
    // The latest scan must know the folder, with no file with two names inside.
    let Some(node) = ins.find(folder) else {
        return never("the latest scan did not see this folder; scan again");
    };
    if !ins.is_dir(node) {
        return never("it is not a folder");
    }
    if holds_hard_link(ins, node) {
        return never("it holds a file with more than one name (a hard link); those are not moved");
    }
    // Another app's folder inside (shared folders like "Program Files\Common Files" vendors).
    let fk = key(folder);
    for other in all_apps {
        if other.name == app.name {
            continue;
        }
        for of in other.folder.iter().chain(&other.launcher_folders) {
            let ok = key(of);
            if ok != fk && within(&ok, &fk) {
                return never(format!(
                    "{} is installed inside it; move that one first, or neither",
                    other.name
                ));
            }
        }
    }
    let mut warnings = vec![format!(
        "Rhizome has not seen {} moved before. Most apps keep working through the link; a few updaters do not. If it does not start, press Move it back.",
        app.name
    )];
    if !link {
        warnings[0] = format!(
            "{}'s launcher must be told the new place after the move (the steps are shown).",
            app.name
        );
    }
    let running: Vec<&str> = background
        .iter()
        .filter(|(_, cmd)| command_runs_from(cmd, folder))
        .map(|(label, _)| label.as_str())
        .collect();
    if !running.is_empty() {
        warnings.push(format!(
            "Parts of it run in the background ({}). Restart the PC after the move, before you test it.",
            running.join(", ")
        ));
    }
    MoveHow::Move {
        link,
        folder: folder.to_owned(),
        warnings,
        steps,
    }
}

/// Why the new place `dest` may not be used (text only; the move engine checks the disk), or
/// `None`. It must be a full path on a drive with a letter, below the drive's top, and not on
/// the protected list (a drive's top folder is protected only as itself).
#[must_use]
pub fn dest_refusal(dest: &str) -> Option<String> {
    let k = key(dest);
    let parts: Vec<&str> = k.split('\\').filter(|p| !p.is_empty()).collect();
    let drive_ok = parts.first().is_some_and(|d| {
        d.len() == 2 && d.ends_with(':') && d.starts_with(|c: char| c.is_ascii_alphabetic())
    });
    if !drive_ok || parts.len() < 2 || parts.iter().any(|p| *p == "." || *p == "..") {
        return Some(
            "the new place must be a full folder path on a drive, like D:\\Apps\\Name".into(),
        );
    }
    if parts[1..]
        .iter()
        .any(|p| p.contains(['<', '>', ':', '"', '|', '?', '*']))
    {
        return Some("the new place has characters Windows does not allow in names".into());
    }
    if let Some(why) = protected(dest) {
        return Some(format!("the new place is not allowed: {why}"));
    }
    if parts.get(1) == Some(&"users") || parts.get(1) == Some(&"windows") {
        return Some("pick a folder outside Users and Windows, like D:\\Apps".into());
    }
    None
}

/// The default new place: `<drive>:\Games\<name>` for games, `<drive>:\Apps\<name>` for others,
/// with characters Windows does not allow in names replaced.
#[must_use]
pub fn default_destination(drive: char, app: &str, folder: &str, game: bool) -> String {
    let leaf = folder
        .trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .filter(|l| !l.is_empty() && !l.ends_with(':'))
        .unwrap_or(app);
    let clean: String = leaf
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.').to_owned();
    let name = if clean.is_empty() {
        "App".to_owned()
    } else {
        clean
    };
    format!(
        "{}:\\{}\\{}",
        drive.to_ascii_uppercase(),
        if game { "Games" } else { "Apps" },
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_commands_are_matched_on_their_program() {
        let f = r"C:\Users\me\AppData\Local\Discord";
        assert!(command_runs_from(
            r#""C:\Users\me\AppData\Local\Discord\Update.exe" --processStart Discord.exe"#,
            f
        ));
        assert!(command_runs_from(
            r"c:\users\me\appdata\local\discord\app-1\Discord.exe -bg",
            f
        ));
        assert!(!command_runs_from(
            r#""C:\Users\me\AppData\Local\DiscordPTB\Update.exe""#,
            f
        ));
        assert!(!command_runs_from(
            r"C:\Windows\System32\svchost.exe -k netsvcs",
            f
        ));
        assert!(command_runs_from(r"\??\C:\Tools\X\drv.exe", r"C:\Tools\X"));
        assert!(!command_runs_from("", f));
    }

    #[test]
    fn office_visual_studio_and_vanguard_are_never_moved() {
        assert!(never_by_name(
            "Microsoft Office Professional Plus 2021",
            Some("Microsoft Corporation")
        )
        .is_some());
        assert!(never_by_name(
            "Microsoft 365 Apps for enterprise",
            Some("Microsoft Corporation")
        )
        .is_some());
        assert!(never_by_name(
            "Visual Studio Community 2022",
            Some("Microsoft Corporation")
        )
        .is_some());
        assert!(never_by_name("Riot Vanguard", Some("Riot Games, Inc.")).is_some());
        assert!(never_by_name(
            "Microsoft Visual Studio Code (User)",
            Some("Microsoft Corporation")
        )
        .is_none());
        assert!(never_by_name("OfficeSuite", Some("MobiSystems")).is_none());
        assert!(never_by_name("Discord", Some("Discord Inc.")).is_none());
    }

    #[test]
    fn the_default_place_is_apps_or_games_on_the_other_drive() {
        assert_eq!(
            default_destination(
                'd',
                "Tool",
                r"C:\Users\me\AppData\Local\Programs\Tool\",
                false
            ),
            r"D:\Apps\Tool"
        );
        assert_eq!(
            default_destination('E', "Genshin Impact", r"C:\Games\Genshin Impact game", true),
            r"E:\Games\Genshin Impact game"
        );
        assert_eq!(
            default_destination('D', "X", "C:/odd/na?me", false),
            r"D:\Apps\na_me"
        );
    }

    #[test]
    fn new_places_must_be_plain_and_unprotected() {
        assert_eq!(dest_refusal(r"D:\Apps\Tool"), None);
        assert_eq!(dest_refusal(r"E:\Games\Genshin Impact game"), None);
        for bad in [
            r"D:\",
            "D:",
            r"Apps\Tool",
            r"D:\Apps\..\Windows",
            r"D:\Windows\X",
            r"D:\Users\me\X",
            r"D:\a?b",
            r"\\server\share\x",
        ] {
            assert!(dest_refusal(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn launchers_get_their_own_way() {
        assert!(matches!(
            launcher_steps("steam", "G"),
            Some((MoveHowKind::Guide("Steam"), _))
        ));
        assert!(matches!(
            launcher_steps("hoyoplay", "G"),
            Some((MoveHowKind::Move, _))
        ));
        assert!(matches!(
            launcher_steps("riot", "G"),
            Some((MoveHowKind::Move, _))
        ));
        assert!(matches!(
            launcher_steps("epic", "G"),
            Some((MoveHowKind::Guide(_), _))
        ));
        assert!(launcher_steps("unknown", "G").is_none());
    }

    fn ins(hardlink: bool) -> Inspector {
        use serde_json::json;
        let nodes: &[(&str, i64, u8)] = &[
            ("This PC", -1, 1),
            ("C:\\", 0, 1),
            ("Program Files", 1, 1),
            ("Tool", 2, 1),
            ("tool.exe", 3, 0),
            ("Suite", 2, 1),
            ("Part", 5, 1),
            ("Games", 1, 1),
            ("Genshin", 7, 1),
            ("Users", 1, 1),
            ("me", 9, 1),
        ];
        let col = |f: &dyn Fn(&(&str, i64, u8)) -> serde_json::Value| {
            serde_json::Value::Array(nodes.iter().map(f).collect())
        };
        let flags = |n: &(&str, i64, u8)| {
            json!(if hardlink && n.0 == "tool.exe" {
                crate::view::flag::HARDLINK
            } else {
                0
            })
        };
        let v = json!({
            "schema": "dc-view", "schema_version": 1, "roots": ["C:\\"],
            "nodes": { "name": col(&|n| json!(n.0)), "parent": col(&|n| json!(n.1)), "kind": col(&|n| json!(n.2)),
                       "size": col(&|_| json!(1)), "mtime": col(&|_| json!(0)), "flags": col(&flags) },
            "owners": [], "assignments": [], "marks": []
        });
        Inspector::from_json(&v.to_string()).unwrap()
    }

    fn app(name: &str, folder: &str) -> AppNow {
        AppNow {
            name: name.into(),
            folder: Some(folder.into()),
            ..AppNow::default()
        }
    }

    #[test]
    fn apps_move_with_a_link_and_launcher_games_with_steps() {
        let i = ins(false);
        let tool = app("Tool", r"C:\Program Files\Tool");
        let how = how_to_move(&i, &tool, r"C:\Program Files\Tool", &[], &[]);
        assert!(
            matches!(&how, MoveHow::Move { link: true, steps, .. } if steps.is_empty()),
            "{how:?}"
        );
        // Something of it runs in the background: a warning.
        let bg = vec![(
            "service: ToolSvc".to_owned(),
            r#""C:\Program Files\Tool\svc.exe""#.to_owned(),
        )];
        let MoveHow::Move { warnings, .. } =
            how_to_move(&i, &tool, r"C:\Program Files\Tool", &[], &bg)
        else {
            panic!()
        };
        assert!(warnings
            .iter()
            .any(|w| w.contains("ToolSvc") && w.contains("Restart")));
        // A HoYoPlay game: no link, the launcher's steps.
        let g = AppNow {
            launcher: Some("hoyoplay".into()),
            ..app("Genshin", r"C:\Games\Genshin")
        };
        assert!(
            matches!(how_to_move(&i, &g, r"C:\Games\Genshin", &[], &[]), MoveHow::Move { link: false, ref steps, .. } if steps.len() == 4)
        );
        // Steam: only Steam's own button.
        let s = AppNow {
            launcher: Some("steam".into()),
            ..app("Genshin", r"C:\Games\Genshin")
        };
        assert!(matches!(
            how_to_move(&i, &s, r"C:\Games\Genshin", &[], &[]),
            MoveHow::Guide { .. }
        ));
    }

    #[test]
    fn refusals() {
        let i = ins(false);
        let never = |h: MoveHow| match h {
            MoveHow::Never { why } => why,
            other => panic!("{other:?}"),
        };
        // Not its registered folder.
        assert!(never(how_to_move(
            &i,
            &app("Tool", r"C:\Program Files\Tool"),
            r"C:\Program Files",
            &[],
            &[]
        ))
        .contains("registered"));
        // Another app inside.
        let suite = app("Suite", r"C:\Program Files\Suite");
        let part = app("Part", r"C:\Program Files\Suite\Part");
        assert!(never(how_to_move(
            &i,
            &suite,
            r"C:\Program Files\Suite",
            &[part],
            &[]
        ))
        .contains("Part"));
        // A user's folder, a drive top, a hard link inside, not in the scan.
        assert!(never(how_to_move(
            &i,
            &app("X", r"C:\Users\me"),
            r"C:\Users\me",
            &[],
            &[]
        ))
        .contains("user"));
        assert!(never(how_to_move(&i, &app("X", r"C:\"), r"C:\", &[], &[])).contains("top"));
        let linked = ins(true);
        assert!(never(how_to_move(
            &linked,
            &app("Tool", r"C:\Program Files\Tool"),
            r"C:\Program Files\Tool",
            &[],
            &[]
        ))
        .contains("hard link"));
        assert!(never(how_to_move(&i, &app("Y", r"D:\Y"), r"D:\Y", &[], &[])).contains("scan"));
        // Store apps: Windows' own Move.
        assert!(matches!(
            how_to_move(
                &i,
                &app("S", r"C:\Program Files\WindowsApps\S"),
                r"C:\Program Files\WindowsApps\S",
                &[],
                &[]
            ),
            MoveHow::Guide { .. }
        ));
    }
}

//! When programs were last used, from Windows' own record of programs started from Explorer and
//! the Start menu ("UserAssist", in the current user's registry). Read-only: this module only
//! decodes what the source read; it never writes anything.
//!
//! The record is not complete (programs started by other programs are missing), so an app with
//! no entry is "not known", never "never used".

use serde::{Deserialize, Serialize};

use crate::model::{InstalledApp, KnownFolder};
use crate::parse::path_key;

/// One value as read from the registry: the encoded name and the raw data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawUsage {
    /// The value name as stored (ROT13-encoded).
    pub name: String,
    /// The value's bytes.
    pub data: Vec<u8>,
}

/// A program and when it was last started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramUse {
    /// Full path of the program or shortcut.
    pub path: String,
    /// How often it was started (as Windows counted it).
    pub runs: u32,
    /// Last start, Unix seconds.
    pub last_run_unix: Option<i64>,
}

/// Known-folder ids that start UserAssist paths, and the known folder (plus a sub-path) each
/// stands for.
const FOLDER_IDS: [(&str, &str, &str); 17] = [
    ("{F1B32785-6FBA-4FCF-9D55-7B8E7F157091}", "LocalAppData", ""),
    (
        "{3EB685DB-65F9-4CF6-A03A-E3EF65729F3D}",
        "RoamingAppData",
        "",
    ),
    (
        "{A520A1A4-1780-4FF6-BD18-167343C5AF16}",
        "LocalAppDataLow",
        "",
    ),
    ("{62AB5D82-FDC1-4DC3-A9DD-070D1D495D97}", "ProgramData", ""),
    ("{905E63B6-C1BF-494E-B29C-65B732D3D21A}", "ProgramFiles", ""),
    ("{6D809377-6AF0-444B-8957-A3773F02200E}", "ProgramFiles", ""),
    (
        "{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}",
        "ProgramFilesX86",
        "",
    ),
    ("{F38BF404-1D43-42F2-9305-67DE0B28FC23}", "Windows", ""),
    ("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}", "System", ""),
    ("{374DE290-123F-4565-9164-39C4925E467B}", "Downloads", ""),
    ("{B4BFCC3A-DB2C-424C-B029-7FE99A87C641}", "Desktop", ""),
    ("{FDD39AD0-238F-46AF-ADB4-6C85480369C7}", "Documents", ""),
    ("{5E6C858F-0E22-4760-9AFE-EA3317B67173}", "Profile", ""),
    ("{625B53C3-AB48-4EC1-BA1F-A1EF4146FC19}", "StartMenu", ""),
    (
        "{A4115719-D62E-491D-AA7C-E74B8BE3B067}",
        "CommonStartMenu",
        "",
    ),
    (
        "{A77F5D77-2E2B-44C3-A6A2-ABA601054A51}",
        "StartMenu",
        r"\Programs",
    ),
    (
        "{0139D44E-6AFE-49F2-8690-3DAFCAE6FFB8}",
        "CommonStartMenu",
        r"\Programs",
    ),
];

/// Seconds between 1601-01-01 (Windows file times) and 1970-01-01.
const FILETIME_TO_UNIX: i64 = 11_644_473_600;

/// Undo ROT13 (letters only), which is how Windows stores these names.
#[must_use]
pub fn rot13(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='m' | 'A'..='M' => char::from(c as u8 + 13),
            'n'..='z' | 'N'..='Z' => char::from(c as u8 - 13),
            _ => c,
        })
        .collect()
}

/// A Windows file time (100 ns since 1601) as Unix seconds; 0 and nonsense give `None`.
#[must_use]
pub fn filetime_to_unix(filetime: u64) -> Option<i64> {
    if filetime == 0 {
        return None;
    }
    let secs = i64::try_from(filetime / 10_000_000).ok()? - FILETIME_TO_UNIX;
    // Before 2000 or after 2200: not a real time.
    (946_684_800..7_258_118_400).contains(&secs).then_some(secs)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
}

/// Run count and last run from a value's data (Windows 7 and later: 72 bytes; XP: 16 bytes).
#[must_use]
pub fn parse_count(data: &[u8]) -> Option<(u32, Option<i64>)> {
    match data.len() {
        72.. => Some((
            u32_at(data, 4)?,
            u64_at(data, 60).and_then(filetime_to_unix),
        )),
        16..=71 => {
            // XP counted from 5.
            let runs = u32_at(data, 4)?.saturating_sub(5);
            Some((runs, u64_at(data, 8).and_then(filetime_to_unix)))
        }
        _ => None,
    }
}

/// Turn a decoded name into a full path, replacing a leading known-folder id.
fn expand_name(name: &str, folders: &[KnownFolder]) -> Option<String> {
    if let Some(rest) = name.strip_prefix('{') {
        let close = rest.find('}')?;
        let id = &name[..close + 2];
        let tail = &name[close + 2..];
        let (_, folder_name, sub) = FOLDER_IDS
            .iter()
            .find(|(gid, _, _)| gid.eq_ignore_ascii_case(id))?;
        let base = folders
            .iter()
            .find(|f| f.name == *folder_name)?
            .path
            .as_deref()?;
        return Some(format!("{}{sub}{tail}", base.trim_end_matches('\\')));
    }
    // A plain full path ("C:\...").
    let bytes = name.as_bytes();
    (bytes.len() > 3 && bytes[1] == b':' && bytes[2] == b'\\').then(|| name.to_owned())
}

/// Decode raw values into programs with their last use. Values that are not programs or
/// shortcuts, or whose folder id is unknown, are left out. Several entries for one path keep
/// the latest time and the sum of runs.
#[must_use]
pub fn resolve_usage(raw: &[RawUsage], folders: &[KnownFolder]) -> Vec<ProgramUse> {
    let mut out: Vec<ProgramUse> = Vec::new();
    for value in raw {
        let name = rot13(&value.name);
        let lower = name.to_lowercase();
        if !(lower.ends_with(".exe") || lower.ends_with(".lnk")) {
            continue;
        }
        let Some(path) = expand_name(&name, folders) else {
            continue;
        };
        let Some((runs, last)) = parse_count(&value.data) else {
            continue;
        };
        if runs == 0 && last.is_none() {
            continue;
        }
        let key = path_key(&path);
        if let Some(seen) = out.iter_mut().find(|p| path_key(&p.path) == key) {
            seen.runs = seen.runs.saturating_add(runs);
            seen.last_run_unix = seen.last_run_unix.max(last);
        } else {
            out.push(ProgramUse {
                path,
                runs,
                last_run_unix: last,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn inside(path_key_: &str, folder: &str) -> bool {
    let folder = path_key(folder);
    path_key_ == folder || path_key_.starts_with(&format!("{}\\", folder.trim_end_matches('\\')))
}

/// When an app was last used: the latest start of a program inside its install folder, of its
/// main program, or of one of its shortcuts. Returns (last run, total runs).
#[must_use]
pub fn last_use(app: &InstalledApp, uses: &[ProgramUse]) -> Option<(Option<i64>, u32)> {
    let main = app.main_exe.as_deref().map(path_key);
    let shortcuts: Vec<String> = app.shortcuts.iter().map(|s| path_key(s)).collect();
    let mut found: Option<(Option<i64>, u32)> = None;
    for u in uses {
        let key = path_key(&u.path);
        let hit = main.as_deref() == Some(key.as_str())
            || shortcuts.contains(&key)
            || app
                .install_folder
                .as_deref()
                .is_some_and(|f| key.ends_with(".exe") && inside(&key, f));
        if hit {
            let (last, runs) = found.unwrap_or((None, 0));
            found = Some((last.max(u.last_run_unix), runs.saturating_add(u.runs)));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders() -> Vec<KnownFolder> {
        [
            ("ProgramFiles", r"C:\Program Files"),
            ("ProgramFilesX86", r"C:\Program Files (x86)"),
            ("Downloads", r"C:\Users\bob\Downloads"),
            (
                "StartMenu",
                r"C:\Users\bob\AppData\Roaming\Microsoft\Windows\Start Menu",
            ),
        ]
        .iter()
        .map(|(n, p)| KnownFolder {
            name: (*n).to_owned(),
            path: Some((*p).to_owned()),
        })
        .collect()
    }

    fn data(runs: u32, filetime: u64) -> Vec<u8> {
        let mut d = vec![0u8; 72];
        d[4..8].copy_from_slice(&runs.to_le_bytes());
        d[60..68].copy_from_slice(&filetime.to_le_bytes());
        d
    }

    /// 2025-01-01T00:00:00Z as a Windows file time.
    const JAN_2025: u64 = (1_735_689_600 + 11_644_473_600) * 10_000_000;

    #[test]
    fn rot13_round_trips_and_leaves_other_characters() {
        assert_eq!(rot13("P:\\Hfref\\obo.rkr"), "C:\\Users\\bob.exe");
        assert_eq!(
            rot13(&rot13("{7C5A40EF}\\Steam 2.exe")),
            "{7C5A40EF}\\Steam 2.exe"
        );
    }

    #[test]
    fn file_times_convert_and_nonsense_is_dropped() {
        assert_eq!(filetime_to_unix(JAN_2025), Some(1_735_689_600));
        assert_eq!(filetime_to_unix(0), None);
        assert_eq!(filetime_to_unix(1), None);
        assert_eq!(filetime_to_unix(u64::MAX), None);
    }

    #[test]
    fn counts_parse_in_both_layouts_and_short_data_is_ignored() {
        assert_eq!(
            parse_count(&data(7, JAN_2025)),
            Some((7, Some(1_735_689_600)))
        );
        let mut xp = vec![0u8; 16];
        xp[4..8].copy_from_slice(&12u32.to_le_bytes());
        xp[8..16].copy_from_slice(&JAN_2025.to_le_bytes());
        assert_eq!(parse_count(&xp), Some((7, Some(1_735_689_600))));
        assert_eq!(parse_count(&[1, 2, 3]), None);
    }

    #[test]
    fn names_with_folder_ids_become_full_paths() {
        let raw = vec![
            RawUsage {
                name: rot13(r"{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}\Steam\steam.exe"),
                data: data(3, JAN_2025),
            },
            RawUsage {
                name: rot13(r"{a77f5d77-2e2b-44c3-a6a2-aba601054a51}\Discord Inc\Discord.lnk"),
                data: data(2, JAN_2025),
            },
            RawUsage {
                name: rot13(r"D:\Games\Tool\tool.exe"),
                data: data(1, 0),
            },
            // Not programs, unknown folder ids, never run: left out.
            RawUsage {
                name: rot13("UEME_CTLSESSION"),
                data: data(9, JAN_2025),
            },
            RawUsage {
                name: rot13(r"{00000000-0000-0000-0000-000000000000}\x.exe"),
                data: data(1, JAN_2025),
            },
            RawUsage {
                name: rot13(r"C:\never.exe"),
                data: data(0, 0),
            },
        ];
        let got = resolve_usage(&raw, &folders());
        let paths: Vec<&str> = got.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                r"C:\Program Files (x86)\Steam\steam.exe",
                r"C:\Users\bob\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Discord Inc\Discord.lnk",
                r"D:\Games\Tool\tool.exe",
            ]
        );
        assert_eq!(got[2].last_run_unix, None);
    }

    #[test]
    fn odd_names_never_panic() {
        for name in [
            "{",
            "}",
            "{}",
            "{abc",
            "",
            "\u{1F600}.exe",
            "{é}\\x.exe",
            "C:",
            "Z:\\",
        ] {
            let _ = resolve_usage(
                &[RawUsage {
                    name: name.to_owned(),
                    data: data(1, JAN_2025),
                }],
                &folders(),
            );
        }
    }

    fn app(name: &str, folder: Option<&str>, shortcuts: &[&str]) -> InstalledApp {
        InstalledApp {
            key: name.to_owned(),
            source: crate::model::RegistrySource::Machine64,
            name: name.to_owned(),
            publisher: None,
            version: None,
            install_folder: folder.map(str::to_owned),
            folder_origin: None,
            display_icon: None,
            uninstall_command: None,
            estimated_size_kb: None,
            install_date: None,
            flags: crate::model::AppFlags::default(),
            main_exe: None,
            version_info: None,
            shortcuts: shortcuts.iter().map(|s| (*s).to_owned()).collect(),
            signature: None,
        }
    }

    #[test]
    fn an_app_is_used_when_a_program_inside_it_or_its_shortcut_ran() {
        let app = app(
            "Steam",
            Some(r"C:\Program Files (x86)\Steam"),
            &[r"C:\Users\bob\Desktop\Steam.lnk"],
        );
        let uses = vec![
            ProgramUse {
                path: r"c:\program files (x86)\steam\steam.exe".into(),
                runs: 3,
                last_run_unix: Some(100),
            },
            ProgramUse {
                path: r"C:\Users\bob\Desktop\Steam.lnk".into(),
                runs: 2,
                last_run_unix: Some(200),
            },
            ProgramUse {
                path: r"C:\Program Files (x86)\SteamOther\x.exe".into(),
                runs: 9,
                last_run_unix: Some(900),
            },
        ];
        assert_eq!(last_use(&app, &uses), Some((Some(200), 5)));
        let other = super::tests::app("Other", None, &[]);
        assert_eq!(last_use(&other, &uses), None);
    }
}

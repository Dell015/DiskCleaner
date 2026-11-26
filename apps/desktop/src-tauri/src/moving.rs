//! Moving an app to another drive (B11, D141). Only in a build made with `--features moving`
//! (off by default until the owner's tests on his PC pass, as removing was, D135); otherwise
//! every command says it is not available. What may move is decided again here from Windows'
//! list right now and the latest scan (`dc_api::moving`); the move engine
//! (`dc_quarantine::mover`) checks the disk again before every step.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::{LastScan, Running};

/// What the page asks to move.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "moving"), allow(dead_code))]
pub struct Ask {
    /// The app's name as the scan shows it.
    name: String,
    publisher: Option<String>,
    /// Its folder as the scan shows it.
    folder: String,
    /// The new place (empty: only list the drives and the default places).
    #[serde(default)]
    dest: String,
    /// The Library sorts it under Games (only changes the default folder name).
    #[serde(default)]
    game: bool,
}

/// A drive the app could go to.
#[derive(Serialize, Clone)]
pub struct Target {
    drive: String,
    filesystem: String,
    free: u64,
    total: u64,
    removable: bool,
    /// The default new place on it.
    dest: String,
    /// Why it cannot be used ("" if it can).
    why: String,
}

/// What moving this app would be.
#[derive(Serialize)]
pub struct Options {
    how: dc_api::moving::MoveHow,
    targets: Vec<Target>,
}

/// What the checks say about one new place.
#[derive(Serialize)]
pub struct Checked {
    ok: bool,
    why: String,
    bytes: u64,
    files: u64,
    free: u64,
    removable: bool,
}

#[cfg(not(feature = "moving"))]
const OFF: &str = "Moving apps is not in this build of Rhizome yet.";

/// Is moving in this build?
#[tauri::command]
pub fn moving_available() -> bool {
    cfg!(feature = "moving")
}

#[cfg(feature = "moving")]
mod on {
    use std::path::{Path, PathBuf};

    use dc_quarantine::{Mover, Places, ProtectedGuard, RealOps};

    /// Off Windows (development only): one holding folder per top folder.
    struct DataPlaces(PathBuf);
    impl Places for DataPlaces {
        fn holding_root(&self, _item: &Path) -> Option<PathBuf> {
            Some(self.0.clone())
        }
    }

    pub fn with_mover<T>(run: impl FnOnce(&Mover<'_>) -> T) -> Result<T, String> {
        let data = dc_api::datadir::default_dir().ok_or("Rhizome's data folder cannot be found")?;
        let guard = ProtectedGuard::default();
        let places: Box<dyn Places> = if cfg!(windows) {
            let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
            Box::new(dc_quarantine::WindowsPlaces::new(Path::new(&local)))
        } else {
            Box::new(DataPlaces(data.join("Holding")))
        };
        let mover = Mover::new(data, places.as_ref(), &guard, &RealOps);
        Ok(run(&mover))
    }

    /// The latest scan, if it is recent enough to act on.
    pub fn fresh(
        last: &crate::LastScan,
    ) -> Result<std::sync::Arc<dc_api::inspect::Inspector>, String> {
        let ins = last
            .inspector
            .lock()
            .map_err(|_| "the app is in a bad state; restart it".to_owned())?
            .clone()
            .ok_or_else(|| {
                "The latest scan is still being prepared (or there is none). Try again in a moment."
                    .to_owned()
            })?;
        let age = last
            .read_at
            .lock()
            .ok()
            .and_then(|at| *at)
            .map_or(crate::SCAN_FRESH_FOR, |at| at.elapsed());
        if age >= crate::SCAN_FRESH_FOR {
            return Err(format!(
                "The latest scan is {} minutes old. Scan again before moving, so Rhizome judges what is on the disk now.",
                age.as_secs() / 60
            ));
        }
        Ok(ins)
    }

    /// How this app moves, decided again from Windows' list right now.
    pub fn how(
        ins: &dc_api::inspect::Inspector,
        name: &str,
        publisher: Option<&str>,
        folder: &str,
    ) -> dc_api::moving::MoveHow {
        let all = dc_api::system::apps_now();
        let same: Vec<_> = all
            .iter()
            .filter(|a| {
                a.name == name && (publisher.is_none() || a.publisher.as_deref() == publisher)
            })
            .collect();
        let Some(app) = same.first() else {
            return dc_api::moving::MoveHow::Never {
                why: format!("Windows does not list {name} as installed now"),
            };
        };
        let bg = dc_api::system::background_programs();
        dc_api::moving::how_to_move(ins, app, folder, &all, &bg)
    }
}

/// The ways to move an app, and the drives it could go to.
#[tauri::command]
pub async fn move_options(ask: Ask, last: State<'_, LastScan>) -> Result<Options, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = (ask, last);
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        use dc_quarantine::MoveOps;
        let ins = on::fresh(&last)?;
        tauri::async_runtime::spawn_blocking(move || {
            let how = on::how(&ins, &ask.name, ask.publisher.as_deref(), &ask.folder);
            let from = dc_quarantine::RealOps.drive(std::path::Path::new(&ask.folder));
            let mut targets = Vec::new();
            for letter in dc_quarantine::drive_letters() {
                let root = format!("{letter}:\\");
                let root_path = std::path::Path::new(&root);
                if from.as_deref() == Some(&letter.to_string()) {
                    continue;
                }
                let Ok(v) = dc_quarantine::RealOps.volume(root_path) else {
                    continue;
                };
                if v.network {
                    continue;
                }
                let why = if !v.filesystem.eq_ignore_ascii_case("NTFS") {
                    format!(
                        "formatted {}; apps can only go to an NTFS drive",
                        if v.filesystem.is_empty() {
                            "in an unknown way"
                        } else {
                            &v.filesystem
                        }
                    )
                } else {
                    String::new()
                };
                targets.push(Target {
                    drive: letter.to_string(),
                    filesystem: v.filesystem,
                    free: v.free,
                    total: v.total,
                    removable: v.removable,
                    dest: dc_api::moving::default_destination(
                        letter,
                        &ask.name,
                        &ask.folder,
                        ask.game,
                    ),
                    why,
                });
            }
            Ok(Options { how, targets })
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Every check for one new place, without changing anything.
#[tauri::command]
pub async fn move_check(ask: Ask, last: State<'_, LastScan>) -> Result<Checked, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = (ask, last);
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        let ins = on::fresh(&last)?;
        tauri::async_runtime::spawn_blocking(move || {
            let dc_api::moving::MoveHow::Move { link, folder, .. } =
                on::how(&ins, &ask.name, ask.publisher.as_deref(), &ask.folder)
            else {
                return Err("Rhizome does not move this app itself.".to_owned());
            };
            let req = dc_quarantine::MoveRequest {
                app: ask.name,
                source: folder.into(),
                dest: ask.dest.into(),
                link,
            };
            if let Some(why) = dc_api::moving::dest_refusal(&req.dest.to_string_lossy()) {
                return Ok(Checked {
                    ok: false,
                    why,
                    bytes: 0,
                    files: 0,
                    free: 0,
                    removable: false,
                });
            }
            on::with_mover(|m| match m.check(&req) {
                Ok(p) => Checked {
                    ok: true,
                    why: String::new(),
                    bytes: p.bytes,
                    files: p.files,
                    free: p.free,
                    removable: p.removable,
                },
                Err(why) => Checked {
                    ok: false,
                    why,
                    bytes: 0,
                    files: 0,
                    free: 0,
                    removable: false,
                },
            })
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Move it. `on_progress` gets (step, bytes done, bytes in all); step is "copy" or "check".
#[tauri::command]
pub async fn move_start(
    ask: Ask,
    on_progress: Channel<(String, u64, u64)>,
    last: State<'_, LastScan>,
    running: State<'_, Running>,
) -> Result<serde_json::Value, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = (ask, on_progress, last, running);
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        let ins = on::fresh(&last)?;
        // One disk job at a time: a duplicate search stops.
        if let Ok(mut slot) = running.dupes.lock() {
            if let Some(c) = slot.take() {
                c.cancel();
            }
        }
        tauri::async_runtime::spawn_blocking(move || {
            // Decided again here: the page only says which app and where to.
            let dc_api::moving::MoveHow::Move { link, folder, .. } =
                on::how(&ins, &ask.name, ask.publisher.as_deref(), &ask.folder)
            else {
                return Err("Rhizome does not move this app itself.".to_owned());
            };
            let req = dc_quarantine::MoveRequest {
                app: ask.name,
                source: folder.into(),
                dest: ask.dest.into(),
                link,
            };
            if let Some(why) = dc_api::moving::dest_refusal(&req.dest.to_string_lossy()) {
                return Err(why);
            }
            let info = on::with_mover(|m| {
                m.start(&req, |step, done, total| {
                    let _ = on_progress.send((step.to_owned(), done, total));
                })
            })??;
            serde_json::to_value(info).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Every move, newest first.
#[tauri::command]
pub fn list_moves() -> Result<serde_json::Value, String> {
    #[cfg(not(feature = "moving"))]
    {
        Ok(serde_json::Value::Array(Vec::new()))
    }
    #[cfg(feature = "moving")]
    {
        let list = on::with_mover(|m| m.moves())?;
        serde_json::to_value(list).map_err(|e| e.to_string())
    }
}

/// "It works": delete the old copy.
#[tauri::command]
pub async fn move_confirm(id: String) -> Result<serde_json::Value, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = id;
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let info = on::with_mover(|m| m.confirm(&id))??;
            serde_json::to_value(info).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Put it back where it was.
#[tauri::command]
pub async fn move_back(
    id: String,
    on_progress: Channel<(String, u64, u64)>,
) -> Result<serde_json::Value, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = (id, on_progress);
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        tauri::async_runtime::spawn_blocking(move || {
            let info = on::with_mover(|m| {
                m.move_back(&id, |step, done, total| {
                    let _ = on_progress.send((step.to_owned(), done, total));
                })
            })??;
            serde_json::to_value(info).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Finish a move Rhizome stopped in the middle of. Returns what was done, in words.
#[tauri::command]
pub async fn move_recover(id: String) -> Result<String, String> {
    #[cfg(not(feature = "moving"))]
    {
        let _ = id;
        Err(OFF.to_owned())
    }
    #[cfg(feature = "moving")]
    {
        tauri::async_runtime::spawn_blocking(move || on::with_mover(|m| m.recover(&id))?)
            .await
            .map_err(|e| e.to_string())?
    }
}

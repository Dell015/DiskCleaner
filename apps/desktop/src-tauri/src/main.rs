//! The DiskCleaner desktop window.
//!
//! It shows the graph view (the web app in `apps/desktop`) and gives it five commands:
//! start a live scan, stop it, ask whether a path is a whole drive, suggest folders, and save
//! the finished scan as a view file.
//! Nothing is ever moved, changed or deleted. The only write is "save scan": it creates one new
//! file the user picked in a save dialog, and never replaces an existing file. There is no
//! file-system or shell plugin; the only plugin is the dialogs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dc_api::live::{run_keep, LiveEvent, LiveOptions, LiveResult};
use dc_scan::CancelToken;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

/// The scan that is running, if any (only one at a time).
#[derive(Default)]
struct Running(Mutex<Option<CancelToken>>);

/// The last finished scan, kept so it can be saved.
#[derive(Default)]
struct LastScan {
    result: Arc<Mutex<Option<LiveResult>>>,
}

/// The newest scan's number: a scan thread keeps its result only if no newer scan started.
static NEWEST_SCAN: AtomicU64 = AtomicU64::new(0);

/// Start scanning `path`; events arrive on `on_event` until `finished` or `failed`.
/// A new scan stops the previous one.
#[tauri::command]
fn start_scan(
    path: String,
    allow_drive_root: bool,
    with_apps: bool,
    on_event: Channel<LiveEvent>,
    running: State<'_, Running>,
    last: State<'_, LastScan>,
) -> Result<(), String> {
    // A new scan replaces what could be saved.
    let generation = NEWEST_SCAN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut slot) = last.result.lock() {
        *slot = None;
    }
    let keep = Arc::clone(&last.result);
    let cancel = CancelToken::new();
    {
        let mut slot = running
            .0
            .lock()
            .map_err(|_| "the app is in a bad state; restart it".to_owned())?;
        if let Some(old) = slot.take() {
            old.cancel();
        }
        *slot = Some(cancel.clone());
    }
    let mut opts = LiveOptions::new(PathBuf::from(path));
    opts.allow_drive_root = allow_drive_root;
    opts.cancel = cancel;
    if with_apps {
        // Read-only registry read, on its own thread while the disk is read.
        opts.apps = Some(Box::new(|| dc_api::system::installed_apps(false)));
    }
    std::thread::spawn(move || {
        // If the window went away, sending fails; the scan result is simply dropped.
        let result = run_keep(opts, |event| {
            let _ = on_event.send(event);
        });
        if let Ok(result) = result {
            if let Ok(mut slot) = keep.lock() {
                // Keep it unless a newer scan has started meanwhile.
                if NEWEST_SCAN.load(Ordering::SeqCst) == generation {
                    *slot = Some(result);
                }
            }
        }
    });
    Ok(())
}

/// What was saved.
#[derive(Serialize)]
struct Saved {
    path: String,
    entries: usize,
    bytes: u64,
}

/// Save the last finished scan as a view file at `path` (picked in a save dialog). Only a new
/// `.json` file is created; an existing file is never replaced.
#[tauri::command]
fn save_scan(path: String, last: State<'_, LastScan>) -> Result<Saved, String> {
    let target = PathBuf::from(&path);
    if !target
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        return Err("The file name must end in .json.".to_owned());
    }
    let view = {
        let slot = last
            .result
            .lock()
            .map_err(|_| "the app is in a bad state; restart it".to_owned())?;
        let Some(kept) = slot.as_ref() else {
            return Err("There is no finished scan to save yet.".to_owned());
        };
        dc_api::view::build(&kept.scan, kept.apps.as_ref()).map_err(|e| e.to_string())?
    };
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                "That file already exists. Pick a new name: DiskCleaner never replaces files."
                    .to_owned()
            } else {
                format!("The file could not be created: {e}")
            }
        })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &view)
        .map_err(|e| format!("The file could not be written: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("The file could not be written: {e}"))?;
    Ok(Saved {
        path,
        entries: view.nodes.name.len().saturating_sub(1),
        bytes: view.nodes.size.iter().sum(),
    })
}

/// Stop the running scan (what was read so far stays on screen).
#[tauri::command]
fn cancel_scan(running: State<'_, Running>) {
    if let Ok(mut slot) = running.0.lock() {
        if let Some(c) = slot.take() {
            c.cancel();
        }
    }
}

/// Is this a whole drive (or file-system root)? Scanning one needs the user's explicit yes.
#[tauri::command]
fn is_drive_root(path: String) -> bool {
    std::fs::canonicalize(&path)
        .map(|p| p.parent().is_none())
        .unwrap_or(false)
}

/// A place worth scanning.
#[derive(Serialize)]
struct Suggestion {
    label: &'static str,
    path: String,
}

/// Folders that usually hold the most: your profile, ProgramData, Program Files.
#[tauri::command]
fn suggested_folders() -> Vec<Suggestion> {
    let mut out = Vec::new();
    for (label, var) in [
        ("Your profile", "USERPROFILE"),
        ("ProgramData", "ProgramData"),
        ("Program Files", "ProgramFiles"),
        ("Program Files (x86)", "ProgramFiles(x86)"),
        ("Home", "HOME"),
    ] {
        if let Ok(path) = std::env::var(var) {
            if std::path::Path::new(&path).is_dir()
                && !out.iter().any(|s: &Suggestion| s.path == path)
            {
                out.push(Suggestion { label, path });
            }
        }
    }
    out
}

/// Debug builds only: the page reports how a smoke test went (see `DC_SMOKE_SCAN`).
#[cfg(debug_assertions)]
#[tauri::command]
fn smoke_report(report: String) {
    println!("SMOKE {report}");
}

#[cfg(not(debug_assertions))]
#[tauri::command]
fn smoke_report(_report: String) {}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Running::default())
        .manage(LastScan::default())
        .setup(|app| {
            smoke(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_scan,
            cancel_scan,
            is_drive_root,
            suggested_folders,
            save_scan,
            smoke_report
        ])
        .run(tauri::generate_context!());
    if let Err(e) = app {
        eprintln!("DiskCleaner could not start: {e}");
        std::process::exit(1);
    }
}

/// Debug builds only: with `DC_SMOKE_SCAN=<folder>`, scan that folder on start and print what the
/// page saw (so the whole path, Rust scanner to graph, can be checked without clicking).
#[cfg(debug_assertions)]
fn smoke(app: &tauri::App) {
    use tauri::Manager;
    let Ok(folder) = std::env::var("DC_SMOKE_SCAN") else {
        return;
    };
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        let arg = format!("{folder:?}");
        // With DC_SMOKE_SAVE=<new file.json>, the finished scan is also saved there.
        if let Ok(save) = std::env::var("DC_SMOKE_SAVE") {
            let _ = window.eval(format!("window.__smokeSave = {save:?}"));
        }
        let _ = window.eval(format!("window.__dc && window.__dc.scanFolder({arg})"));
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = window.eval(
                "(() => { const s = window.__dc.state; const inv = window.__TAURI_INTERNALS__.invoke; if (s.live && s.live.finished && !s.live.active && window.__smokeSave && !window.__saved) { window.__saved = 1; inv('save_scan', { path: window.__smokeSave }).then((r) => inv('smoke_report', { report: 'SAVED ' + JSON.stringify(r) }), (e) => inv('smoke_report', { report: 'SAVE FAILED ' + e })); } const l = s.live; window.__TAURI_INTERNALS__.invoke('smoke_report', { report: JSON.stringify({ entries: s.tree ? s.tree.n : 0, shown: l ? l.partial.items.length : 0, active: l ? l.active : null, finished: l ? l.finished : null, marks: s.tree ? s.tree.marks.length : 0, visible: s.vis ? s.vis.items.length : 0, error: document.getElementById('error').hidden ? null : document.getElementById('errorText').textContent }) }); })()",
            );
        }
    });
}

#[cfg(not(debug_assertions))]
fn smoke(_app: &tauri::App) {}

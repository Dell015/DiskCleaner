//! The DiskCleaner desktop window.
//!
//! It shows the views (the web app in `apps/desktop`) and gives them these commands: start a
//! live scan, stop it, ask whether a path is a whole drive, suggest folders, save the finished
//! scan as a view file, look for duplicate files in the finished scan (reads files only), and
//! the AI-assistant settings (allow access, show what was asked, copy the report).
//!
//! Started with `--mcp`, it opens no window and serves the latest scan to an AI app over MCP
//! (read-only; see `dc_api::mcp`).
//!
//! Nothing scanned is ever moved, changed or deleted. The writes are: "save scan" (one new file
//! the user picked in a save dialog, never replacing an existing file), and DiskCleaner's own
//! data folder (latest scan, its report, the AI-access switch, the AI log). There is no
//! file-system or shell plugin; the only plugin is the dialogs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dc_api::dupes::{find_duplicates, DupEvent, DupOptions};
use dc_api::live::{run_keep, LiveEvent, LiveOptions, LiveResult};
use dc_scan::CancelToken;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

/// The disk jobs that are running: a scan and a duplicate search (one of each at a time; a new
/// scan stops a duplicate search, so the disk is read by one job at a time).
#[derive(Default)]
struct Running {
    scan: Mutex<Option<CancelToken>>,
    dupes: Mutex<Option<CancelToken>>,
}

/// The last finished scan, kept so it can be saved.
#[derive(Default)]
struct LastScan {
    result: Arc<Mutex<Option<Arc<LiveResult>>>>,
}

/// The newest scan's number: a scan thread keeps its result only if no newer scan started.
static NEWEST_SCAN: AtomicU64 = AtomicU64::new(0);

/// What to scan and how.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanRequest {
    path: String,
    allow_drive_root: bool,
    with_apps: bool,
    /// Read Windows' record of started programs ("last used").
    with_usage: bool,
    /// Keep the latest scan and its report in DiskCleaner's data folder (for the person and
    /// for their AI assistant, if they allow it).
    keep_report: bool,
}

/// Start scanning; events arrive on `on_event` until `finished` or `failed`.
/// A new scan stops the previous one.
#[tauri::command]
fn start_scan(
    options: ScanRequest,
    on_event: Channel<LiveEvent>,
    running: State<'_, Running>,
    last: State<'_, LastScan>,
) -> Result<(), String> {
    // One disk job at a time: a new scan stops a duplicate search.
    if let Ok(mut slot) = running.dupes.lock() {
        if let Some(c) = slot.take() {
            c.cancel();
        }
    }
    // A new scan replaces what could be saved.
    let generation = NEWEST_SCAN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut slot) = last.result.lock() {
        *slot = None;
    }
    let keep = Arc::clone(&last.result);
    let cancel = CancelToken::new();
    {
        let mut slot = running
            .scan
            .lock()
            .map_err(|_| "the app is in a bad state; restart it".to_owned())?;
        if let Some(old) = slot.take() {
            old.cancel();
        }
        *slot = Some(cancel.clone());
    }
    let ScanRequest {
        path,
        allow_drive_root,
        with_apps,
        with_usage,
        keep_report,
    } = options;
    let mut opts = LiveOptions::new(PathBuf::from(path));
    opts.allow_drive_root = allow_drive_root;
    opts.cancel = cancel;
    if with_apps {
        // Read-only registry read, on its own thread while the disk is read.
        // "Last used" reads Windows' record of started programs, unless switched off in Settings.
        opts.apps = Some(Box::new(move || {
            dc_api::system::installed_apps(false, with_usage)
        }));
    }
    std::thread::spawn(move || {
        // If the window went away, sending fails; the scan result is simply dropped.
        let result = run_keep(opts, |event| {
            let _ = on_event.send(event);
        });
        if let Ok(result) = result {
            let result = Arc::new(result);
            if let Ok(mut slot) = keep.lock() {
                // Keep it unless a newer scan has started meanwhile.
                if NEWEST_SCAN.load(Ordering::SeqCst) == generation {
                    *slot = Some(Arc::clone(&result));
                }
            }
            // The scan report (and the scan behind it), for the person and their AI assistant.
            // A stopped scan is incomplete, so it does not replace the last good one.
            if keep_report && result.scan.status != dc_core::ScanStatus::Cancelled {
                let _ = write_latest(&result);
            }
        }
    });
    Ok(())
}

/// Write the latest scan and its report into DiskCleaner's data folder.
fn write_latest(result: &LiveResult) -> Result<(), String> {
    let dir = dc_api::datadir::default_dir().ok_or("no data folder")?;
    let view =
        dc_api::view::build(&result.scan, result.apps.as_ref()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&view).map_err(|e| e.to_string())?;
    let ins = dc_api::inspect::Inspector::from_json(&json).map_err(|e| e.to_string())?;
    let report = dc_api::report::markdown(&ins);
    dc_api::datadir::save_latest(&dir, &json, &report).map_err(|e| e.to_string())
}

/// What the AI-assistant settings show.
#[derive(Serialize)]
struct AiStatus {
    /// AI assistants may read the latest scan.
    enabled: bool,
    /// DiskCleaner's data folder.
    folder: String,
    /// When the latest report was written (Unix seconds), if there is one.
    report_time: Option<u64>,
    /// This program, for the AI app's settings (it serves MCP when started with `--mcp`).
    program: String,
    /// The latest questions, newest last.
    log: Vec<serde_json::Value>,
}

/// The AI-assistant settings and the latest questions asked.
#[tauri::command]
fn ai_status() -> Result<AiStatus, String> {
    let dir = dc_api::datadir::default_dir().ok_or("no data folder")?;
    let report_time = std::fs::metadata(dir.join(dc_api::datadir::LATEST_REPORT))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Ok(AiStatus {
        enabled: dc_api::datadir::access(&dir).enabled,
        folder: dir.to_string_lossy().into_owned(),
        report_time,
        program: std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        log: dc_api::datadir::read_log(&dir, 30),
    })
}

/// Allow or stop AI assistants reading the latest scan.
#[tauri::command]
fn set_ai_access(enabled: bool) -> Result<(), String> {
    let dir = dc_api::datadir::default_dir().ok_or("no data folder")?;
    dc_api::datadir::set_access(&dir, enabled).map_err(|e| e.to_string())
}

/// The latest scan report (Markdown), to copy into any AI chat.
#[tauri::command]
fn latest_report() -> Result<String, String> {
    let dir = dc_api::datadir::default_dir().ok_or("no data folder")?;
    std::fs::read_to_string(dir.join(dc_api::datadir::LATEST_REPORT))
        .map_err(|_| "There is no report yet: scan a folder first.".to_owned())
}

/// Look for identical files in the last finished scan (only below `within`, if given). Files are
/// read, never changed. A new search stops the previous one.
#[tauri::command]
fn find_dupes(
    within: Option<String>,
    on_event: Channel<DupEvent>,
    last: State<'_, LastScan>,
    running: State<'_, Running>,
) -> Result<(), String> {
    // Only the finished scan is checked (its numbers are final). The scan is shared, not
    // locked, so a new scan or a save can start meanwhile.
    let kept = last
        .result
        .lock()
        .map_err(|_| "the app is in a bad state; restart it".to_owned())?
        .clone()
        .ok_or_else(|| "There is no finished scan yet. Scan a folder first.".to_owned())?;
    let cancel = CancelToken::new();
    {
        let mut slot = running
            .dupes
            .lock()
            .map_err(|_| "the app is in a bad state; restart it".to_owned())?;
        if let Some(old) = slot.take() {
            old.cancel();
        }
        *slot = Some(cancel.clone());
    }
    std::thread::spawn(move || {
        let opts = DupOptions {
            within,
            cancel,
            ..DupOptions::default()
        };
        let result = find_duplicates(&kept.scan, &opts, |e| {
            let _ = on_event.send(e);
        });
        if result.is_err() {
            let _ = on_event.send(DupEvent::Finished {
                status: "cancelled",
                groups: 0,
                wasted: 0,
                unreadable: 0,
            });
        }
    });
    Ok(())
}

/// Stop the duplicate search.
#[tauri::command]
fn cancel_dupes(running: State<'_, Running>) {
    if let Ok(mut slot) = running.dupes.lock() {
        if let Some(c) = slot.take() {
            c.cancel();
        }
    }
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
    let kept = last
        .result
        .lock()
        .map_err(|_| "the app is in a bad state; restart it".to_owned())?
        .clone()
        .ok_or_else(|| "There is no finished scan to save yet.".to_owned())?;
    let view = dc_api::view::build(&kept.scan, kept.apps.as_ref()).map_err(|e| e.to_string())?;
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
    if let Ok(mut slot) = running.scan.lock() {
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
    // Started by an AI app as its MCP server: answer on standard input and output, no window.
    if std::env::args().any(|a| a == "--mcp") {
        let code = match dc_api::datadir::default_dir() {
            Some(dir) => match dc_api::datadir::serve_stdio(dir) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("DiskCleaner MCP server stopped: {e}");
                    1
                }
            },
            None => {
                eprintln!("DiskCleaner cannot find its data folder");
                1
            }
        };
        std::process::exit(code);
    }
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
            find_dupes,
            cancel_dupes,
            ai_status,
            set_ai_access,
            latest_report,
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
                "(() => { const s = window.__dc.state; const inv = window.__TAURI_INTERNALS__.invoke; if (s.live && s.live.finished && !s.live.active && window.__smokeSave && !window.__saved) { window.__saved = 1; inv('save_scan', { path: window.__smokeSave }).then((r) => inv('smoke_report', { report: 'SAVED ' + JSON.stringify(r) }), (e) => inv('smoke_report', { report: 'SAVE FAILED ' + e })); } if (s.live && s.live.finished && !s.live.active && !window.__dupsAsked) { window.__dupsAsked = 1; window.__dc.findDups(-1); } const d = s.dups; inv('smoke_report', { report: 'DUPS ' + JSON.stringify({ status: d.status, groups: d.groups.length, total: d.total, found: d.groups.map((g) => g.nodes.filter((n) => n >= 0).length) }) }); const l = s.live; window.__TAURI_INTERNALS__.invoke('smoke_report', { report: JSON.stringify({ entries: s.tree ? s.tree.n : 0, shown: l ? l.partial.items.length : 0, active: l ? l.active : null, finished: l ? l.finished : null, marks: s.tree ? s.tree.marks.length : 0, visible: s.vis ? s.vis.items.length : 0, error: document.getElementById('error').hidden ? null : document.getElementById('errorText').textContent }) }); })()",
            );
        }
    });
}

#[cfg(not(debug_assertions))]
fn smoke(_app: &tauri::App) {}

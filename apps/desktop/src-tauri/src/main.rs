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

mod moving;
mod viewer;

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
    /// The same scan with its checks, indexed for questions (built just after it finishes).
    inspector: Arc<Mutex<Option<Arc<dc_api::inspect::Inspector>>>>,
    /// When that scan started reading the disk (removing needs a recent scan).
    read_at: Arc<Mutex<Option<std::time::Instant>>>,
}

/// Removing is decided from the latest scan; older than this, the disk may have changed too much
/// (an app installed again, files moved), so a new scan is needed first.
#[cfg_attr(not(any(feature = "cleaning", feature = "moving")), allow(dead_code))]
const SCAN_FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(60 * 60);

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
    let keep_ins = Arc::clone(&last.inspector);
    if let Ok(mut slot) = keep_ins.lock() {
        *slot = None;
    }
    let keep_at = Arc::clone(&last.read_at);
    let started = std::time::Instant::now();
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
            // A stopped scan is incomplete: no checks, no report, nothing to remove from.
            if result.scan.status != dc_core::ScanStatus::Cancelled {
                if let Ok((json, ins)) = index(&result) {
                    if let Ok(mut slot) = keep_ins.lock() {
                        if NEWEST_SCAN.load(Ordering::SeqCst) == generation {
                            *slot = Some(Arc::clone(&ins));
                            if let Ok(mut at) = keep_at.lock() {
                                *at = Some(started);
                            }
                        }
                    }
                    // The scan report (and the scan behind it), for the person and their AI.
                    if keep_report {
                        let _ = write_latest(&json, &ins);
                    }
                }
            }
        }
    });
    Ok(())
}

/// Write the latest scan and its report into DiskCleaner's data folder.
fn write_latest(json: &str, ins: &dc_api::inspect::Inspector) -> Result<(), String> {
    let dir = dc_api::datadir::default_dir().ok_or("no data folder")?;
    let report = dc_api::report::markdown(ins);
    dc_api::datadir::save_latest(&dir, json, &report).map_err(|e| e.to_string())
}

/// The finished scan with its checks: as view-file text and indexed for questions.
fn index(result: &LiveResult) -> Result<(String, Arc<dc_api::inspect::Inspector>), String> {
    let view =
        dc_api::view::build(&result.scan, result.apps.as_ref()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&view).map_err(|e| e.to_string())?;
    let ins = dc_api::inspect::Inspector::from_json(&json).map_err(|e| e.to_string())?;
    Ok((json, Arc::new(ins)))
}

/// Removing (B9). Only in a build made with `--features cleaning`; otherwise every command says
/// it is not available. The removal engine (`dc-quarantine`) checks each item on the disk again.
mod cleaning {
    use super::{LastScan, Running, SCAN_FRESH_FOR};
    use serde::{Deserialize, Serialize};
    use tauri::ipc::Channel;
    use tauri::State;

    /// One picked item and the person's consent for it.
    #[derive(Deserialize)]
    #[cfg_attr(not(feature = "cleaning"), allow(dead_code))]
    #[serde(rename_all = "camelCase")]
    pub struct Pick {
        path: String,
        /// "I checked this" (Probably safe items).
        checked: bool,
        /// "I understand these are my own files" (Your call items).
        understood: bool,
        /// A folder this app left behind after it was uninstalled from Rhizome (D133): judged by
        /// `left_behind_eligibility` (it must have been the app's, and the app must be gone).
        #[serde(default)]
        left_behind_of: Option<String>,
    }

    /// What happened.
    #[derive(Serialize, Default)]
    pub struct Removal {
        /// Cleanup id ("" if nothing moved).
        id: String,
        /// (path, bytes) of what is now held.
        held: Vec<(String, u64)>,
        /// (path, why) of what was not removed (or not completely).
        skipped: Vec<(String, String)>,
    }

    #[cfg(feature = "cleaning")]
    mod on {
        use std::path::{Path, PathBuf};

        use dc_quarantine::{Engine, Places, ProtectedGuard, RealOps};

        /// Off Windows (development only): one holding folder in the data folder.
        struct DataPlaces(PathBuf);
        impl Places for DataPlaces {
            fn holding_root(&self, _item: &Path) -> Option<PathBuf> {
                Some(self.0.clone())
            }
        }

        pub fn with_engine<T>(run: impl FnOnce(&Engine<'_>) -> T) -> Result<T, String> {
            let data =
                dc_api::datadir::default_dir().ok_or("Rhizome's data folder cannot be found")?;
            let guard = ProtectedGuard::default();
            let places: Box<dyn Places> = if cfg!(windows) {
                let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
                Box::new(dc_quarantine::WindowsPlaces::new(Path::new(&local)))
            } else {
                Box::new(DataPlaces(data.join("Holding")))
            };
            let engine = Engine::new(data, places.as_ref(), &guard, &RealOps);
            Ok(run(&engine))
        }
    }

    /// Is removing in this build?
    #[tauri::command]
    pub fn cleaning_available() -> bool {
        cfg!(feature = "cleaning")
    }

    #[cfg(not(feature = "cleaning"))]
    const OFF: &str = "Removing is not in this build of Rhizome yet.";

    /// Remove picked items: each is grouped again from the latest scan, then the engine checks
    /// the disk and moves it into its holding folder. `on_progress` gets (done, total).
    #[tauri::command]
    pub async fn remove_items(
        picks: Vec<Pick>,
        on_progress: Channel<(usize, usize)>,
        last: State<'_, LastScan>,
        running: State<'_, Running>,
    ) -> Result<Removal, String> {
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
            .map_or(SCAN_FRESH_FOR, |at| at.elapsed());
        if age >= SCAN_FRESH_FOR {
            return Err(format!(
                "The latest scan is {} minutes old. Scan again before removing, so Rhizome judges what is on the disk now.",
                age.as_secs() / 60
            ));
        }
        // One disk job at a time.
        if let Ok(mut slot) = running.dupes.lock() {
            if let Some(c) = slot.take() {
                c.cancel();
            }
        }
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = (picks, on_progress, ins);
            Err(OFF.to_owned())
        }
        #[cfg(feature = "cleaning")]
        {
            use dc_api::cleanup::{eligibility, left_behind_eligibility, Eligibility};
            use dc_quarantine::{Group, Item};
            tauri::async_runtime::spawn_blocking(move || {
                let mut out = Removal::default();
                let mut items = Vec::new();
                // Windows' list right now, only when something left behind is picked.
                let installed: Vec<String> = if picks.iter().any(|p| p.left_behind_of.is_some()) {
                    listed_now().into_iter().map(|(n, _, _)| n).collect()
                } else {
                    Vec::new()
                };
                for p in picks {
                    // Rust decides the group; the page only brings the person's ticks.
                    let judged = match &p.left_behind_of {
                        Some(app) => left_behind_eligibility(
                            &ins,
                            &p.path,
                            app,
                            installed.iter().any(|n| n == app),
                        ),
                        None => eligibility(&ins, &p.path),
                    };
                    let (group, title, reason) = match judged {
                        Eligibility::Never { why } => {
                            out.skipped.push((p.path, why));
                            continue;
                        }
                        Eligibility::Safe { title, reason } => (Group::Safe, title, reason),
                        Eligibility::ProbablySafe { title, reason } => {
                            (Group::ProbablySafe { checked: p.checked }, title, reason)
                        }
                        Eligibility::YourCall => (
                            Group::YourCall {
                                understood: p.understood,
                            },
                            "Your own files".to_owned(),
                            "your_call".to_owned(),
                        ),
                    };
                    // Left behind: the uninstaller changed the folder after the scan, so its
                    // time from the scan says nothing (the app being gone is checked above).
                    let newest = if p.left_behind_of.is_some() {
                        0
                    } else {
                        ins.find(&p.path).map_or(0, |i| ins.changed(i))
                    };
                    items.push(Item {
                        path: std::path::PathBuf::from(&p.path),
                        group,
                        title,
                        reason,
                        newest_at_scan: newest,
                    });
                }
                let outcome = on::with_engine(|e| {
                    e.remove(&items, |done, total| {
                        let _ = on_progress.send((done, total));
                    })
                })?;
                out.id = outcome.id;
                out.held = outcome
                    .held
                    .iter()
                    .map(|h| {
                        (
                            h.record.original.to_string_lossy().into_owned(),
                            h.record.bytes,
                        )
                    })
                    .collect();
                out.skipped.extend(
                    outcome
                        .skipped
                        .into_iter()
                        .map(|s| (s.path.to_string_lossy().into_owned(), s.why)),
                );
                Ok(out)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }

    /// Every cleanup, newest first (with each held item).
    #[tauri::command]
    pub fn list_cleanups() -> Result<serde_json::Value, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            Err(OFF.to_owned())
        }
        #[cfg(feature = "cleaning")]
        {
            let list = on::with_engine(|e| e.cleanups())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
    }

    /// Holding folders used before that cannot be found now (a drive that is not connected, or
    /// whose letter changed): their removed items are not listed until they are back.
    #[tauri::command]
    pub fn missing_holding() -> Result<Vec<String>, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            Ok(Vec::new())
        }
        #[cfg(feature = "cleaning")]
        {
            on::with_engine(|e| {
                e.missing_roots()
                    .into_iter()
                    .map(|r| r.to_string_lossy().into_owned())
                    .collect()
            })
        }
    }

    /// Put back a cleanup's held items (all, or the (holding folder, number) pairs given).
    #[tauri::command]
    pub async fn restore_items(
        id: String,
        only: Option<Vec<(String, u32)>>,
    ) -> Result<serde_json::Value, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = (id, only);
            Err(OFF.to_owned())
        }
        #[cfg(feature = "cleaning")]
        {
            tauri::async_runtime::spawn_blocking(move || {
                let only: Option<Vec<(std::path::PathBuf, u32)>> = only.map(|v| {
                    v.into_iter()
                        .map(|(r, n)| (std::path::PathBuf::from(r), n))
                        .collect()
                });
                let r = on::with_engine(|e| e.restore(&id, only.as_deref()))?;
                serde_json::to_value(r).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }

    /// Delete a cleanup's held items for good. Returns the bytes freed.
    #[tauri::command]
    pub async fn empty_cleanup(id: String) -> Result<u64, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = id;
            Err(OFF.to_owned())
        }
        #[cfg(feature = "cleaning")]
        {
            tauri::async_runtime::spawn_blocking(move || on::with_engine(|e| e.empty(&id))?)
                .await
                .map_err(|e| e.to_string())?
        }
    }

    #[cfg(feature = "cleaning")]
    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
    }

    /// A cleanup whose retention time is over.
    #[derive(Serialize)]
    pub struct Expired {
        id: String,
        /// When it was made (Unix seconds).
        created: i64,
        /// Bytes still held.
        bytes: u64,
    }

    /// Cleanups older than `days` that still hold something. Never one with an unknown date or
    /// a date after now (a wrong clock). Nothing is deleted here.
    #[tauri::command]
    pub fn expired_cleanups(days: u32) -> Result<Vec<Expired>, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = days;
            Ok(Vec::new())
        }
        #[cfg(feature = "cleaning")]
        {
            on::with_engine(|e| {
                e.expired(days.clamp(1, 365), now_secs())
                    .into_iter()
                    .map(|c| Expired {
                        bytes: c.held_bytes(),
                        id: c.id,
                        created: c.created,
                    })
                    .collect()
            })
        }
    }

    /// Empty the cleanups `ids`, each only if it is still older than `days`. Returns (id, bytes
    /// freed) for each one emptied.
    #[tauri::command]
    pub async fn empty_expired(days: u32, ids: Vec<String>) -> Result<Emptied, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = (days, ids);
            Ok(Emptied::default())
        }
        #[cfg(feature = "cleaning")]
        {
            tauri::async_runtime::spawn_blocking(move || {
                on::with_engine(|e| {
                    let mut out = Emptied::default();
                    for c in e
                        .expired(days.clamp(1, 365), now_secs())
                        .into_iter()
                        .filter(|c| ids.contains(&c.id))
                    {
                        match e.empty(&c.id) {
                            Ok(bytes) => out.emptied.push((c.id, bytes)),
                            // Shown on screen (the items stay in the Removed tab).
                            Err(why) => out.failed.push((c.id, why)),
                        }
                    }
                    out
                })
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }

    /// What emptying old cleanups did: emptied (id, bytes freed) and failed (id, why).
    #[derive(Serialize, Default)]
    pub struct Emptied {
        emptied: Vec<(String, u64)>,
        failed: Vec<(String, String)>,
    }

    /// Installed apps as Windows lists them right now (the ones Settings shows): a light read of
    /// the uninstall entries, cheap enough to repeat while an uninstaller runs.
    fn listed_now() -> Vec<(String, Option<String>, Option<String>)> {
        dc_api::system::listed_apps()
    }

    /// Is each of these apps (by name) still installed? Read-only; used to notice when an
    /// uninstaller has finished.
    #[tauri::command]
    pub async fn apps_installed(names: Vec<String>) -> Result<Vec<bool>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            let now = listed_now();
            names
                .iter()
                .map(|n| now.iter().any(|(name, _, _)| name == n))
                .collect()
        })
        .await
        .map_err(|e| e.to_string())
    }

    /// Is each of these folders still there? Read-only (a link counts as there; nothing is
    /// followed). Used to notice when a launcher has removed a game.
    #[tauri::command]
    pub async fn paths_exist(paths: Vec<String>) -> Result<Vec<bool>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            paths
                .iter()
                .map(|p| std::fs::symlink_metadata(p).is_ok())
                .collect()
        })
        .await
        .map_err(|e| e.to_string())
    }

    /// Size (bytes, files) of each folder now, or `None` when it is gone. Read-only; links are
    /// counted as themselves and never followed. Used to show what an uninstalled app left
    /// behind without scanning the whole drive again.
    #[tauri::command]
    pub async fn folder_sizes(paths: Vec<String>) -> Result<Vec<Option<(u64, u64)>>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            paths
                .iter()
                .map(|p| {
                    let top = std::fs::symlink_metadata(p).ok()?;
                    if !top.is_dir() {
                        return Some((top.len(), 1));
                    }
                    let (mut bytes, mut files) = (0u64, 0u64);
                    let mut stack = vec![std::path::PathBuf::from(p)];
                    // A bound, so a huge folder cannot hang the page.
                    let mut budget = 2_000_000u32;
                    while let Some(dir) = stack.pop() {
                        let Ok(read) = std::fs::read_dir(&dir) else {
                            continue;
                        };
                        for entry in read.flatten() {
                            budget = budget.saturating_sub(1);
                            if budget == 0 {
                                return Some((bytes, files));
                            }
                            let Ok(meta) = entry.metadata() else { continue };
                            if meta.is_symlink() {
                                continue;
                            }
                            if meta.is_dir() {
                                stack.push(entry.path());
                            } else {
                                bytes += meta.len();
                                files += 1;
                            }
                        }
                    }
                    Some((bytes, files))
                })
                .collect()
        })
        .await
        .map_err(|e| e.to_string())
    }

    /// What starting an uninstall did.
    #[derive(Serialize)]
    pub struct Uninstalling {
        /// "uninstaller": the app's own uninstaller was started; "settings": Windows' Installed
        /// apps page was opened instead (the recorded command could not be started safely).
        how: &'static str,
        /// Why the Installed apps page was opened instead.
        why: Option<String>,
    }

    /// Start an installed app's own uninstaller, with its own window (D133). The app is found
    /// again in Windows' list by name (and publisher); the command Windows records for it is
    /// split into one program and its arguments and never given to a shell. Anything doubtful
    /// opens Windows' own Installed apps page instead.
    #[tauri::command]
    pub async fn uninstall_app(
        name: String,
        publisher: Option<String>,
    ) -> Result<Uninstalling, String> {
        #[cfg(not(feature = "cleaning"))]
        {
            let _ = (name, publisher);
            Err(OFF.to_owned())
        }
        #[cfg(feature = "cleaning")]
        {
            use dc_quarantine::uninstall;
            tauri::async_runtime::spawn_blocking(move || {
                if !cfg!(windows) {
                    return Err("Uninstalling only works on Windows.".to_owned());
                }
                let now = listed_now();
                let same: Vec<_> = now
                    .iter()
                    .filter(|(n, p, _)| *n == name && (publisher.is_none() || *p == publisher))
                    .collect();
                if same.is_empty() {
                    return Err(format!("Windows no longer lists {name} as installed."));
                }
                let settings = |why: String| -> Result<Uninstalling, String> {
                    uninstall::open_installed_apps().map_err(|e| {
                        format!("{why}, and Windows' Installed apps page could not be opened: {e}")
                    })?;
                    Ok(Uninstalling {
                        how: "settings",
                        why: Some(why),
                    })
                };
                // Two entries of the same name with different commands: do not guess.
                if same.iter().any(|(_, _, c)| *c != same[0].2) {
                    return settings(format!("Windows lists more than one {name}"));
                }
                let Some(recorded) = same[0].2.as_deref() else {
                    return settings(format!("Windows records no uninstaller for {name}"));
                };
                let cmd = match uninstall::parse(
                    recorded,
                    &|p: &std::path::Path| p.is_file(),
                    &uninstall::system_dir(),
                ) {
                    Ok(c) => c,
                    Err(why) => return settings(why),
                };
                uninstall::start(&cmd).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        "Windows' permission question was answered No, so nothing was started."
                            .to_owned()
                    } else {
                        format!("The uninstaller could not be started: {e}")
                    }
                })?;
                Ok(Uninstalling {
                    how: "uninstaller",
                    why: None,
                })
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
}

/// App icons for the page: one PNG data URL (or nothing) per icon location, in order. Reads the
/// app's own program or icon file; nothing is run or changed.
#[tauri::command]
async fn app_icons(locations: Vec<String>) -> Result<Vec<Option<String>>, String> {
    if locations.len() > 2000 {
        return Err("too many icons at once".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        locations
            .iter()
            .map(|l| dc_api::icon::data_url(l))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
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
                "That file already exists. Pick a new name: Rhizome never replaces files."
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
                    eprintln!("Rhizome MCP server stopped: {e}");
                    1
                }
            },
            None => {
                eprintln!("Rhizome cannot find its data folder");
                1
            }
        };
        std::process::exit(code);
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Running::default())
        .manage(LastScan::default())
        .manage(viewer::Viewable::default())
        .register_uri_scheme_protocol(viewer::SCHEME, viewer::serve)
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
            app_icons,
            cleaning::cleaning_available,
            cleaning::apps_installed,
            cleaning::paths_exist,
            cleaning::folder_sizes,
            cleaning::uninstall_app,
            cleaning::remove_items,
            cleaning::list_cleanups,
            cleaning::restore_items,
            cleaning::empty_cleanup,
            cleaning::empty_expired,
            cleaning::expired_cleanups,
            cleaning::missing_holding,
            moving::moving_available,
            moving::move_options,
            moving::move_check,
            moving::move_start,
            moving::list_moves,
            moving::move_confirm,
            moving::move_back,
            moving::move_recover,
            viewer::view_open,
            smoke_report
        ])
        .run(tauri::generate_context!());
    if let Err(e) = app {
        eprintln!("Rhizome could not start: {e}");
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
        if let Ok(paths) = std::env::var("DC_SMOKE_REMOVE") {
            let list: Vec<&str> = paths.split(';').collect();
            let _ = window.eval(format!(
                "window.__smokeRemove = {}",
                serde_json::to_string(&list).unwrap_or_default()
            ));
        }
        if let Ok(save) = std::env::var("DC_SMOKE_SAVE") {
            let _ = window.eval(format!("window.__smokeSave = {save:?}"));
        }
        let _ = window.eval(format!("window.__dc && window.__dc.scanFolder({arg})"));
        for _ in 0..60 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = window.eval(
                "(() => { const s = window.__dc.state; const inv = window.__TAURI_INTERNALS__.invoke; if (s.live && s.live.finished && !s.live.active && window.__smokeSave && !window.__saved) { window.__saved = 1; inv('save_scan', { path: window.__smokeSave }).then((r) => inv('smoke_report', { report: 'SAVED ' + JSON.stringify(r) }), (e) => inv('smoke_report', { report: 'SAVE FAILED ' + e })); } if (s.live && s.live.finished && !s.live.active && !window.__dupsAsked) { window.__dupsAsked = 1; window.__dc.findDups(-1); } if (window.__smokeRemove && s.live && s.live.finished && !s.live.active && !window.__removeAsked) { window.__removeAsked = 1; setTimeout(() => window.__dcRemove(window.__smokeRemove).then((r) => inv('smoke_report', { report: 'REMOVE ' + JSON.stringify(r) }), (e) => inv('smoke_report', { report: 'REMOVE FAILED ' + e })), 1500); } const d = s.dups; inv('smoke_report', { report: 'DUPS ' + JSON.stringify({ status: d.status, groups: d.groups.length, total: d.total, found: d.groups.map((g) => g.nodes.filter((n) => n >= 0).length) }) }); const l = s.live; window.__TAURI_INTERNALS__.invoke('smoke_report', { report: JSON.stringify({ entries: s.tree ? s.tree.n : 0, shown: l ? l.partial.items.length : 0, active: l ? l.active : null, finished: l ? l.finished : null, marks: s.tree ? s.tree.marks.length : 0, visible: s.vis ? s.vis.items.length : 0, error: document.getElementById('error').hidden ? null : document.getElementById('errorText').textContent }) }); })()",
            );
        }
    });
}

#[cfg(not(debug_assertions))]
fn smoke(_app: &tauri::App) {}

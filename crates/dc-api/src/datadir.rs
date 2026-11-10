//! DiskCleaner's own data folder: the latest scan and its report (for the person and for their
//! AI assistant), whether AI assistants may read them, and a log of what they asked.
//!
//! This is the only module of `dc-api` that writes files, and it only ever writes inside this
//! folder (never anywhere that was scanned). Files are replaced or appended to, never deleted.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::inspect::Inspector;

/// The app's identifier (the folder name, as the desktop app's settings use).
pub const APP_ID: &str = "io.github.dell015.diskcleaner";
/// The latest scan, as a view file.
pub const LATEST_SCAN: &str = "latest-scan.dcview.json";
/// The latest scan's report.
pub const LATEST_REPORT: &str = "latest-report.md";
/// Whether AI assistants may read the scan.
pub const ACCESS: &str = "ai-access.json";
/// What AI assistants asked (one JSON object per line).
pub const LOG: &str = "ai-log.jsonl";
/// The log starts over when it grows past this.
const LOG_LIMIT: u64 = 512 * 1024;

/// The data folder: `%APPDATA%\<id>` on Windows, `~/Library/Application Support/<id>` on macOS,
/// `$XDG_DATA_HOME/<id>` (or `~/.local/share/<id>`) elsewhere. `DISKCLEANER_DATA` overrides it.
#[must_use]
pub fn default_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DISKCLEANER_DATA").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
    };
    base.map(|b| b.join(APP_ID))
}

/// Whether AI assistants may read the scan.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Access {
    /// Switched on in DiskCleaner's settings.
    pub enabled: bool,
}

/// Read the access setting (off when missing or unreadable: fail closed).
#[must_use]
pub fn access(dir: &Path) -> Access {
    fs::read_to_string(dir.join(ACCESS))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write the access setting.
pub fn set_access(dir: &Path, enabled: bool) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join(ACCESS),
        serde_json::to_string(&Access { enabled }).unwrap_or_default(),
    )
}

/// Save the latest scan (view file text) and its report, replacing the previous ones.
pub fn save_latest(dir: &Path, view_json: &str, report_md: &str) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    // Written in place (renaming is reserved for the quarantine crate); a reader that meets a
    // half-written file waits a moment and reads again.
    fs::write(dir.join(LATEST_SCAN), view_json)?;
    fs::write(dir.join(LATEST_REPORT), report_md)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Append one line to the log (starting it over when it has grown too big).
pub fn log(dir: &Path, tool: &str, args: &Value, ok: bool) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG);
    if fs::metadata(&path).is_ok_and(|m| m.len() > LOG_LIMIT) {
        fs::write(&path, "")?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "{}",
        json!({ "time": now(), "tool": tool, "args": args, "ok": ok })
    )
}

/// The last `n` log lines, newest last.
#[must_use]
pub fn read_log(dir: &Path, n: usize) -> Vec<Value> {
    let text = fs::read_to_string(dir.join(LOG)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// The MCP server's view of the data folder: the latest scan when access is on, re-read when the
/// file changes; every question logged.
pub struct FolderSource {
    dir: PathBuf,
    cached: Option<(SystemTime, Arc<Inspector>)>,
}

impl FolderSource {
    /// Serve from `dir`.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, cached: None }
    }
}

impl crate::mcp::Source for FolderSource {
    fn scan(&mut self) -> Result<Arc<Inspector>, String> {
        if !access(&self.dir).enabled {
            return Err("DiskCleaner does not allow AI assistants to read the scan right now. The user can switch it on in DiskCleaner: Settings, AI assistant.".to_owned());
        }
        let path = self.dir.join(LATEST_SCAN);
        let modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|_| {
                "There is no scan yet. The user should scan a drive or folder in DiskCleaner first."
                    .to_owned()
            })?;
        if let Some((when, ins)) = &self.cached {
            if *when == modified {
                return Ok(Arc::clone(ins));
            }
        }
        let read = || -> Result<Inspector, String> {
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("The latest scan could not be read: {e}"))?;
            Inspector::from_json(&text).map_err(|e| {
                format!("The latest scan could not be used ({e}); scan again in DiskCleaner.")
            })
        };
        let ins = Arc::new(read().or_else(|_| {
            // DiskCleaner may be writing it right now.
            std::thread::sleep(std::time::Duration::from_millis(500));
            read()
        })?);
        self.cached = Some((modified, Arc::clone(&ins)));
        Ok(ins)
    }

    fn log(&mut self, tool: &str, args: &Value, ok: bool) {
        // The log is a courtesy for the person; failing to write it never stops an answer.
        let _ = log(&self.dir, tool, args, ok);
    }
}

/// Run the MCP server on standard input and output, serving the data folder.
pub fn serve_stdio(dir: PathBuf) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut source = FolderSource::new(dir);
    crate::mcp::serve(stdin.lock(), stdout.lock(), &mut source)
}

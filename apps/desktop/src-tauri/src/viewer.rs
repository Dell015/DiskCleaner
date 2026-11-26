//! Looking at your own files inside Rhizome (D143): videos, pictures, music, PDFs and text play
//! or show in a preview window. **Read-only**: files are only opened for reading.
//!
//! The page cannot read files by itself (no file-system access, D96). For each file the person
//! opens, [`view_open`] checks it (a plain file of a known kind, seen by the latest scan, not a
//! link, not cloud-only) and gives back a one-time address on Rhizome's own `rzview` scheme;
//! [`serve`] answers that address and nothing else, in pieces (so a long video can be skipped
//! through without reading it all). Addresses are forgotten when the app closes; at most
//! [`KEEP`] are kept.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, Runtime, State, UriSchemeContext};

use crate::LastScan;

/// The scheme the page loads previews from.
pub const SCHEME: &str = "rzview";
/// How many opened files are remembered.
const KEEP: usize = 64;
/// The most bytes sent in one answer (the player asks for the rest as it plays).
const PIECE: u64 = 4 * 1024 * 1024;
/// Text files show their first part only.
pub const TEXT_MAX: u64 = 512 * 1024;
/// Pictures and documents bigger than this are not shown (open them in their own app).
const WHOLE_MAX: u64 = 200 * 1024 * 1024;

/// Files opened for viewing: (token, path, content type).
#[derive(Default)]
pub struct Viewable(Mutex<VecDeque<(String, PathBuf, &'static str)>>);

/// What the page gets back.
#[derive(Serialize)]
pub struct Opened {
    /// The address to load.
    url: String,
    /// "video", "picture", "music", "pdf" or "text".
    kind: &'static str,
    /// The content type.
    mime: &'static str,
    /// Bytes.
    size: u64,
}

/// The kind and content type of a file, by its extension; `None`: not shown in Rhizome.
#[must_use]
pub fn kind_of(name: &str) -> Option<(&'static str, &'static str)> {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())?;
    Some(match ext.as_str() {
        "mp4" | "m4v" => ("video", "video/mp4"),
        "webm" => ("video", "video/webm"),
        "mov" => ("video", "video/quicktime"),
        "mkv" => ("video", "video/x-matroska"),
        "avi" => ("video", "video/x-msvideo"),
        "wmv" => ("video", "video/x-ms-wmv"),
        "jpg" | "jpeg" | "jfif" => ("picture", "image/jpeg"),
        "png" => ("picture", "image/png"),
        "gif" => ("picture", "image/gif"),
        "webp" => ("picture", "image/webp"),
        "bmp" => ("picture", "image/bmp"),
        "avif" => ("picture", "image/avif"),
        "ico" => ("picture", "image/x-icon"),
        "mp3" => ("music", "audio/mpeg"),
        "m4a" | "aac" => ("music", "audio/mp4"),
        "wav" => ("music", "audio/wav"),
        "ogg" | "oga" | "opus" => ("music", "audio/ogg"),
        "flac" => ("music", "audio/flac"),
        "pdf" => ("pdf", "application/pdf"),
        "txt" | "md" | "log" | "csv" | "json" | "xml" | "ini" | "cfg" | "yaml" | "yml" | "toml" => {
            ("text", "text/plain; charset=utf-8")
        }
        _ => return None,
    })
}

/// Why this file is not shown, or `None`. Links, cloud-only files and anything but a plain
/// file are refused (fail closed).
fn refusal(path: &Path) -> Option<String> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => return Some(format!("it cannot be read ({e})")),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Some("it is not a plain file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE: u32 = 0x400;
        const OFFLINE: u32 = 0x1000;
        const RECALL_ON_OPEN: u32 = 0x4_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
        let a = meta.file_attributes();
        if a & REPARSE != 0 {
            return Some("it is a link".into());
        }
        if a & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
            return Some(
                "it is only in the cloud (OneDrive or similar); open it in Explorer to download it first"
                    .into(),
            );
        }
    }
    None
}

/// Open a file for viewing: checks it and returns where the page can load it from.
#[tauri::command]
pub fn view_open(
    path: String,
    last: State<'_, LastScan>,
    viewable: State<'_, Viewable>,
) -> Result<Opened, String> {
    let p = PathBuf::from(&path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (kind, mime) = kind_of(&name).ok_or("Rhizome does not show this kind of file")?;
    // Only files the latest scan saw (the page cannot ask for anything else).
    let known = last
        .inspector
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .is_some_and(|ins| ins.find(&path).is_some_and(|i| !ins.is_dir(i)));
    if !known {
        return Err("Scan again: this file is not in the latest scan".into());
    }
    if let Some(why) = refusal(&p) {
        return Err(format!("It cannot be shown: {why}."));
    }
    let size = std::fs::metadata(&p).map_err(|e| e.to_string())?.len();
    if matches!(kind, "picture" | "pdf") && size > WHOLE_MAX {
        return Err("It is too big to show here; open it in its own app.".into());
    }
    let token = format!(
        "{:x}{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
        std::process::id()
    );
    let mut list = viewable
        .0
        .lock()
        .map_err(|_| "the app is in a bad state; restart it")?;
    list.push_back((token.clone(), p, mime));
    while list.len() > KEEP {
        list.pop_front();
    }
    // Windows (WebView2) serves custom schemes as http://<scheme>.localhost.
    let ext = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let url = if cfg!(windows) {
        format!("http://{SCHEME}.localhost/{token}.{ext}")
    } else {
        format!("{SCHEME}://localhost/{token}.{ext}")
    };
    Ok(Opened {
        url,
        kind,
        mime,
        size,
    })
}

fn answer(status: StatusCode, text: &str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Cow::Owned(text.as_bytes().to_vec()))
        .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
}

/// `bytes=a-b` or `bytes=a-` → (start, end inclusive) within `size`.
#[must_use]
pub fn range(value: &str, size: u64) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?;
    if spec.contains(',') || size == 0 {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let (start, end) = if a.is_empty() {
        // The last n bytes.
        let n: u64 = b.parse().ok()?;
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            size - 1
        } else {
            b.parse::<u64>().ok()?.min(size - 1)
        };
        (start, end)
    };
    (start <= end && start < size).then_some((start, end))
}

/// Answer an `rzview` address: only tokens [`view_open`] gave out; read-only, in pieces.
pub fn serve<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let token = request
        .uri()
        .path()
        .trim_start_matches('/')
        .split('.')
        .next()
        .unwrap_or("")
        .to_owned();
    let found = ctx
        .app_handle()
        .state::<Viewable>()
        .0
        .lock()
        .ok()
        .and_then(|l| {
            l.iter()
                .find(|(t, _, _)| *t == token)
                .map(|(_, p, m)| (p.clone(), *m))
        });
    let Some((path, mime)) = found else {
        return answer(StatusCode::NOT_FOUND, "not opened for viewing");
    };
    if refusal(&path).is_some() {
        return answer(StatusCode::FORBIDDEN, "it cannot be shown");
    }
    let Ok(mut file) = File::open(&path) else {
        return answer(StatusCode::NOT_FOUND, "it cannot be read");
    };
    let size = file.metadata().map_or(0, |m| m.len());
    let asked = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|v| range(v, size));
    let text = mime.starts_with("text/");
    let (start, end, partial) = match asked {
        Some(None) => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(Cow::Borrowed(&[][..]))
                .unwrap_or_else(|_| answer(StatusCode::RANGE_NOT_SATISFIABLE, ""));
        }
        Some(Some((s, e))) => (s, e.min(s + PIECE - 1), true),
        None if text => (0, size.min(TEXT_MAX).saturating_sub(1), false),
        None if size > WHOLE_MAX => (0, PIECE.min(size) - 1, true),
        None => (0, size.saturating_sub(1), false),
    };
    let len = if size == 0 { 0 } else { end - start + 1 };
    let mut buf = vec![0u8; usize::try_from(len).unwrap_or(0)];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut buf).is_err() {
        return answer(StatusCode::INTERNAL_SERVER_ERROR, "it could not be read");
    }
    let mut b = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, len.to_string())
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CACHE_CONTROL, "no-store");
    b = if partial {
        b.status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
    } else {
        b.status(StatusCode::OK)
    };
    b.body(Cow::Owned(buf))
        .unwrap_or_else(|_| answer(StatusCode::INTERNAL_SERVER_ERROR, "it could not be sent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_like_browsers_send_them() {
        assert_eq!(range("bytes=0-", 1000), Some((0, 999)));
        assert_eq!(range("bytes=100-199", 1000), Some((100, 199)));
        assert_eq!(range("bytes=900-5000", 1000), Some((900, 999)));
        assert_eq!(range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(range("bytes=1000-", 1000), None);
        assert_eq!(range("bytes=5-1", 1000), None);
        assert_eq!(range("bytes=0-1,5-9", 1000), None);
        assert_eq!(range("items=0-1", 1000), None);
        assert_eq!(range("bytes=0-", 0), None);
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(kind_of("clip.MP4").map(|k| k.0), Some("video"));
        assert_eq!(kind_of("a.jpeg").map(|k| k.0), Some("picture"));
        assert_eq!(kind_of("song.flac").map(|k| k.0), Some("music"));
        assert_eq!(kind_of("doc.pdf").map(|k| k.0), Some("pdf"));
        assert_eq!(kind_of("notes.txt").map(|k| k.0), Some("text"));
        assert_eq!(kind_of("setup.exe"), None);
        assert_eq!(
            kind_of("page.html"),
            None,
            "web pages could run scripts: never shown"
        );
        assert_eq!(
            kind_of("picture.svg"),
            None,
            "SVG can hold scripts: never shown"
        );
        assert_eq!(kind_of("noext"), None);
    }
}

//! Platform-specific special entries: symlinks, junctions and sparse files.
//!
//! Every function here returns an [`Outcome`]. A capability the machine does not have is a
//! `Skipped` outcome (recorded in the manifest), never an error. Anything unexpected is an
//! [`io::Error`] and aborts generation.
//!
//! External commands (`cmd.exe`, `fsutil.exe`) are located by absolute path under
//! `%SystemRoot%\System32`, take their arguments as separate array elements, and have their
//! output discarded. No `unsafe` code is used anywhere.

use std::fs::OpenOptions;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

/// What happened when a special entry was attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The entry was created and verified.
    Created,
    /// The entry could not be created on this machine. Nothing was left behind.
    Skipped(String),
    /// The entry exists on disk but could not be verified, so it is reported as skipped.
    #[cfg_attr(not(windows), allow(dead_code))]
    SkippedButExists(String),
}

/// Create a file symlink at `link` pointing to `target_name` (relative to the link's folder).
pub fn symlink_file(link: &Path, target_name: &str) -> io::Result<Outcome> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target_name, link)?;
        Ok(Outcome::Created)
    }
    #[cfg(windows)]
    {
        windows_symlink(std::os::windows::fs::symlink_file(target_name, link))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target_name);
        Ok(Outcome::Skipped(
            "symlinks are not supported on this platform".into(),
        ))
    }
}

/// Create a directory symlink at `link` pointing to `target_name` (relative to its folder).
pub fn symlink_dir(link: &Path, target_name: &str) -> io::Result<Outcome> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target_name, link)?;
        Ok(Outcome::Created)
    }
    #[cfg(windows)]
    {
        windows_symlink(std::os::windows::fs::symlink_dir(target_name, link))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target_name);
        Ok(Outcome::Skipped(
            "symlinks are not supported on this platform".into(),
        ))
    }
}

#[cfg(windows)]
fn windows_symlink(result: io::Result<()>) -> io::Result<Outcome> {
    /// `ERROR_PRIVILEGE_NOT_HELD`: Developer Mode is off and we are not elevated.
    const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
    match result {
        Ok(()) => Ok(Outcome::Created),
        Err(err) if err.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD) => {
            Ok(Outcome::Skipped("symlink privilege not held".into()))
        }
        Err(err) => Err(err),
    }
}

/// Create a directory junction at `link` pointing to the absolute directory `target`.
///
/// Windows only. Elsewhere the entry is skipped as "not applicable".
pub fn junction(link: &Path, target: &Path) -> io::Result<Outcome> {
    #[cfg(windows)]
    {
        windows_junction(link, target)
    }
    #[cfg(not(windows))]
    {
        let _ = (link, target);
        Ok(Outcome::Skipped(
            "junctions are not applicable on this platform".into(),
        ))
    }
}

/// Turn the (empty, freshly created) file at `path` into a sparse file: logical length
/// `logical`, with `data` written at `offset`. Never writes the full logical size.
pub fn make_sparse(path: &Path, logical: u64, offset: u64, data: &[u8]) -> io::Result<Outcome> {
    #[cfg(windows)]
    {
        if let Some(reason) = windows_set_sparse_flag(path)? {
            return Ok(Outcome::Skipped(reason));
        }
    }
    let mut file = OpenOptions::new().write(true).open(path)?;
    file.set_len(logical)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(data)?;
    file.flush()?;
    Ok(Outcome::Created)
}

// ---------------------------------------------------------------------------------------------
// Windows-only helpers
// ---------------------------------------------------------------------------------------------

/// Locate a tool in `%SystemRoot%\System32`. `SystemRoot` only locates the binary and never
/// influences the generated output. No PATH or current-directory lookup happens.
#[cfg(windows)]
fn system_tool(name: &str) -> Option<std::path::PathBuf> {
    let root = std::env::var_os("SystemRoot")?;
    let path = std::path::PathBuf::from(root).join("System32").join(name);
    path.is_file().then_some(path)
}

#[cfg(windows)]
fn windows_junction(link: &Path, target: &Path) -> io::Result<Outcome> {
    use std::process::{Command, Stdio};

    let Some(cmd) = system_tool("cmd.exe") else {
        return Ok(Outcome::Skipped("cmd.exe not found".into()));
    };
    let (Some(link_text), Some(target_text)) = (plain_path(link), plain_path(target)) else {
        return Ok(Outcome::Skipped("path cannot be passed to cmd.exe".into()));
    };
    if !is_cmd_safe(&link_text) || !is_cmd_safe(&target_text) {
        return Ok(Outcome::Skipped("path not safe for cmd.exe".into()));
    }
    let status = Command::new(cmd)
        .args(["/C", "mklink", "/J"])
        .arg(&link_text)
        .arg(&target_text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        return Ok(Outcome::Skipped(format!("mklink /J failed ({status})")));
    }
    match std::fs::read_link(link) {
        Ok(found) if same_path(&found, target) => Ok(Outcome::Created),
        _ => Ok(Outcome::SkippedButExists(
            "created but not verifiable".into(),
        )),
    }
}

/// Mark the file sparse with `fsutil`. Returns `Some(reason)` if that was not possible.
#[cfg(windows)]
fn windows_set_sparse_flag(path: &Path) -> io::Result<Option<String>> {
    use std::process::{Command, Stdio};

    let Some(fsutil) = system_tool("fsutil.exe") else {
        return Ok(Some("fsutil.exe not found".into()));
    };
    let Some(text) = plain_path(path) else {
        return Ok(Some("path cannot be passed to fsutil".into()));
    };
    let status = Command::new(fsutil)
        .args(["sparse", "setflag"])
        .arg(&text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(None)
    } else {
        Ok(Some(format!("fsutil sparse setflag failed ({status})")))
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit-tested on every platform)
// ---------------------------------------------------------------------------------------------

/// The path as text without the `\\?\` verbatim prefix. UNC verbatim paths are refused.
#[cfg_attr(not(windows), allow(dead_code))]
fn plain_path(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.starts_with(r"\\?\UNC\") {
        return None;
    }
    Some(text.strip_prefix(r"\\?\").unwrap_or(text).to_owned())
}

/// `cmd.exe` has its own quoting rules, so paths are never escaped: only a conservative
/// character set is allowed through, and anything else is skipped.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_cmd_safe(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.' | ':' | '\\'))
}

/// Compare two Windows paths ignoring case, the verbatim prefixes and a trailing separator.
#[cfg_attr(not(windows), allow(dead_code))]
fn same_path(a: &Path, b: &Path) -> bool {
    fn normal(path: &Path) -> String {
        let text = path.to_string_lossy().to_lowercase();
        let text = text
            .strip_prefix(r"\\?\")
            .or_else(|| text.strip_prefix(r"\??\"))
            .unwrap_or(&text)
            .to_owned();
        text.trim_end_matches('\\').to_owned()
    }
    normal(a) == normal(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_path_strips_the_verbatim_prefix() {
        assert_eq!(
            plain_path(Path::new(r"\\?\C:\a\b")).as_deref(),
            Some(r"C:\a\b")
        );
        assert_eq!(plain_path(Path::new(r"C:\a")).as_deref(), Some(r"C:\a"));
        assert_eq!(plain_path(Path::new(r"\\?\UNC\server\share")), None);
    }

    #[test]
    fn cmd_safety_allows_only_a_conservative_set() {
        assert!(is_cmd_safe(r"C:\Users\Test User\fixtures-1\tree\links"));
        for bad in [
            "a&b", "a%b", "a^b", "a!b", "a\"b", "a|b", "a<b", "a>b", "a(b)", "",
        ] {
            assert!(!is_cmd_safe(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn path_comparison_ignores_case_prefixes_and_trailing_separator() {
        assert!(same_path(
            Path::new(r"\??\C:\Tree\T"),
            Path::new(r"c:\tree\t\")
        ));
        assert!(same_path(
            Path::new(r"\\?\C:\Tree\T"),
            Path::new(r"C:\tree\t")
        ));
        assert!(!same_path(Path::new(r"C:\a"), Path::new(r"C:\b")));
    }

    #[cfg(not(windows))]
    #[test]
    fn junctions_are_not_applicable_off_windows() {
        let outcome = junction(
            Path::new("/nonexistent/link"),
            Path::new("/nonexistent/target"),
        );
        assert!(matches!(outcome, Ok(Outcome::Skipped(_))));
    }
}

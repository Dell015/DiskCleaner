//! Turning a path into a comparison form, using text only (no disk access).
//!
//! Anything that cannot be understood safely is reported as invalid, and invalid paths are
//! treated as protected: the engine fails closed.

use crate::verdict::Reason;

/// A normalized absolute drive path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalPath {
    /// Drive letter, lower case.
    pub drive: char,
    /// Components below the drive root: lower case, cleaned, no `.` or `..`.
    pub parts: Vec<String>,
    /// True if any component looks like an 8.3 short name (`PROGRA~1`).
    pub has_short_name: bool,
}

/// Why a path could not be normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// A network (UNC) or device path.
    NetworkOrDevice,
    /// Empty, relative, malformed or unusual.
    InvalidPath,
}

impl Invalid {
    pub(crate) fn reason(self) -> Reason {
        match self {
            Invalid::NetworkOrDevice => Reason::new(
                "network_or_device",
                "network share or device path (never scanned or cleaned)",
            ),
            Invalid::InvalidPath => Reason::new(
                "invalid_path",
                "path is empty, relative, malformed or unusual and cannot be judged",
            ),
        }
    }
}

const RESERVED: [&str; 8] = [
    "con", "prn", "aux", "nul", "conin$", "conout$", "clock$", "$",
];

fn is_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or_default();
    let stem = stem.trim_end();
    if RESERVED.contains(&stem) && stem != "$" {
        return true;
    }
    for prefix in ["com", "lpt"] {
        if let Some(rest) = stem.strip_prefix(prefix) {
            if matches!(
                rest,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            ) {
                return true;
            }
        }
    }
    false
}

/// `PROGRA~1`, `DOCUME~1.TXT`: up to six characters, a tilde, digits, an optional short extension.
pub(crate) fn looks_like_short_name(component: &str) -> bool {
    let (stem, extension) = component.split_once('.').unwrap_or((component, ""));
    if extension.chars().count() > 3 || stem.chars().count() > 8 {
        return false;
    }
    let Some((before, digits)) = stem.rsplit_once('~') else {
        return false;
    };
    let before_len = before.chars().count();
    (1..=6).contains(&before_len)
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
}

/// Normalize `input`. See the requirements (R2) for the rules.
pub fn normalize(input: &str) -> Result<NormalPath, Invalid> {
    let text = input.trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(Invalid::InvalidPath);
    }
    let text = text.replace('/', "\\");
    let lower = text.to_lowercase();

    if lower.starts_with(r"\\.\") || lower.starts_with(r"\\?\unc\") {
        return Err(Invalid::NetworkOrDevice);
    }
    // Windows does not resolve `.` and `..` after `\\?\`; if we did, our view of the path
    // could differ from the file system's, so such components are refused there.
    let literal_prefix = lower.starts_with(r"\\?\") || lower.starts_with(r"\??\");
    let rest = if literal_prefix {
        text.get(4..).unwrap_or_default()
    } else {
        text.as_str()
    };
    if rest.starts_with(r"\\") {
        return Err(Invalid::NetworkOrDevice);
    }
    if rest
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(Invalid::InvalidPath);
    }

    let mut chars = rest.chars();
    let drive = chars
        .next()
        .filter(char::is_ascii_alphabetic)
        .ok_or(Invalid::InvalidPath)?;
    if chars.next() != Some(':') {
        return Err(Invalid::InvalidPath);
    }
    let below = rest.get(2..).unwrap_or_default();
    if !below.is_empty() && !below.starts_with('\\') {
        return Err(Invalid::InvalidPath); // "C:foo" is relative to the current folder of C:
    }

    let mut parts: Vec<String> = Vec::new();
    let mut has_short_name = false;
    for raw in below.split('\\') {
        // Alternate data stream or `::$DATA` suffix: everything from the first colon.
        let name = raw.split(':').next().unwrap_or_default();
        if name.chars().all(|c| c == '.' || c == ' ') {
            match name {
                "" | "." => {
                    if (raw.contains(':') && !raw.starts_with(':'))
                        || (literal_prefix && name == ".")
                    {
                        return Err(Invalid::InvalidPath);
                    }
                    continue;
                }
                ".." => {
                    if literal_prefix || parts.pop().is_none() {
                        return Err(Invalid::InvalidPath); // climbs above the drive root
                    }
                    continue;
                }
                _ => return Err(Invalid::InvalidPath), // "...", ". ", ".. ": fail closed
            }
        }
        // Windows ignores trailing dots and spaces on a component.
        let cleaned = name.trim_end_matches(['.', ' ']).to_lowercase();
        if cleaned.is_empty() || is_reserved(&cleaned) {
            return Err(Invalid::InvalidPath);
        }
        has_short_name |= looks_like_short_name(&cleaned);
        parts.push(cleaned);
    }

    Ok(NormalPath {
        drive: drive.to_ascii_lowercase(),
        parts,
        has_short_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> (char, Vec<String>) {
        let n = normalize(text).unwrap();
        (n.drive, n.parts)
    }

    #[test]
    fn plain_paths_become_lower_case_components() {
        assert_eq!(
            ok(r"C:\Windows\System32"),
            ('c', vec!["windows".into(), "system32".into()])
        );
        assert_eq!(
            ok("c:/WINDOWS//system32/"),
            ('c', vec!["windows".into(), "system32".into()])
        );
        assert_eq!(ok(r"D:\"), ('d', vec![]));
        assert_eq!(ok("d:"), ('d', vec![]));
    }

    #[test]
    fn dots_and_detours_are_resolved_lexically() {
        assert_eq!(ok(r"C:\a\.\b\..\c"), ('c', vec!["a".into(), "c".into()]));
        assert_eq!(normalize(r"C:\.."), Err(Invalid::InvalidPath));
        assert_eq!(normalize(r"C:\a\..\.."), Err(Invalid::InvalidPath));
    }

    #[test]
    fn trailing_dots_spaces_and_streams_are_ignored() {
        assert_eq!(
            ok("C:\\Windows.\\System32 "),
            ('c', vec!["windows".into(), "system32".into()])
        );
        assert_eq!(
            ok(r"C:\Windows\System32::$DATA"),
            ('c', vec!["windows".into(), "system32".into()])
        );
        assert_eq!(
            ok(r"C:\a\b.txt:stream"),
            ('c', vec!["a".into(), "b.txt".into()])
        );
        assert_eq!(normalize(r"C:\a\...\b"), Err(Invalid::InvalidPath));
        assert_eq!(normalize("C:\\a\\.. \\b"), Err(Invalid::InvalidPath));
    }

    #[test]
    fn prefixes_are_handled() {
        assert_eq!(ok(r"\\?\C:\Windows"), ('c', vec!["windows".into()]));
        assert_eq!(ok(r"\??\C:\Windows"), ('c', vec!["windows".into()]));
        assert_eq!(
            normalize(r"\\?\UNC\server\share"),
            Err(Invalid::NetworkOrDevice)
        );
        assert_eq!(
            normalize(r"\\server\share\x"),
            Err(Invalid::NetworkOrDevice)
        );
        assert_eq!(
            normalize(r"\\.\PhysicalDrive0"),
            Err(Invalid::NetworkOrDevice)
        );
        assert_eq!(normalize(r"\\?\Volume{1234}\x"), Err(Invalid::InvalidPath));
        assert_eq!(
            normalize(r"\\?\GLOBALROOT\Device\x"),
            Err(Invalid::InvalidPath)
        );
        assert_eq!(
            normalize(r"\Device\HarddiskVolume1\x"),
            Err(Invalid::InvalidPath)
        );
        assert_eq!(normalize(r"\\?\C:\a\..\b"), Err(Invalid::InvalidPath));
        assert_eq!(normalize(r"\\?\C:\a\.\b"), Err(Invalid::InvalidPath));
    }

    #[test]
    fn unusable_input_is_invalid() {
        for bad in [
            "",
            "   ",
            "relative\\path",
            "C:foo",
            "C",
            "1:\\x",
            "C:\\a*b",
            "C:\\a?b",
            "C:\\a\u{0}b",
            "C:\\con",
            "C:\\a\\NUL.txt",
            "C:\\COM1",
            "C:\\lpt9.log",
        ] {
            assert!(normalize(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn short_names_are_recognised() {
        assert!(looks_like_short_name("progra~1"));
        assert!(looks_like_short_name("docume~1.txt"));
        assert!(!looks_like_short_name("program files"));
        assert!(!looks_like_short_name("a~b"));
        assert!(!looks_like_short_name("toolongname~1"));
        assert!(normalize(r"C:\PROGRA~1\x").unwrap().has_short_name);
        assert!(!normalize(r"C:\Program Files\x").unwrap().has_short_name);
    }
}

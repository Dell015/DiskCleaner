//! Component patterns: `*:\users\*\.ssh`. `*` as a whole segment matches any one component;
//! a `*` inside a segment matches any text within that component.

use crate::normalize::NormalPath;

/// One component of a pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    AnyOne,
    Glob(String),
}

impl Segment {
    fn parse(text: &str) -> Segment {
        if text == "*" {
            Segment::AnyOne
        } else if text.contains('*') {
            Segment::Glob(text.to_owned())
        } else {
            Segment::Literal(text.to_owned())
        }
    }

    fn matches(&self, component: &str) -> bool {
        match self {
            Segment::Literal(text) => text == component,
            Segment::AnyOne => true,
            Segment::Glob(pattern) => glob_match(pattern, component),
        }
    }
}

/// A parsed pattern: a drive (or any drive) and component segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    drive: Option<char>,
    segments: Vec<Segment>,
}

impl Pattern {
    /// Parse `*:\a\b` or `c:\a\b` (lower case). `None` if the text is not of that form.
    pub fn parse(text: &str) -> Option<Pattern> {
        let mut parts = text.split('\\');
        let head = parts.next()?;
        let drive = match head {
            "*:" => None,
            other => {
                let mut chars = other.chars();
                let letter = chars.next().filter(char::is_ascii_alphabetic)?;
                if chars.next() != Some(':') || chars.next().is_some() {
                    return None;
                }
                Some(letter.to_ascii_lowercase())
            }
        };
        let segments: Vec<Segment> = parts
            .filter(|p| !p.is_empty())
            .map(Segment::parse)
            .collect();
        Some(Pattern { drive, segments })
    }

    /// Number of components below the drive.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// True for a pattern with no components (the drive root).
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// How many components are plain text (no wildcard).
    pub fn literal_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| matches!(s, Segment::Literal(_)))
            .count()
    }

    /// How many components name something: plain text, or a wildcard pattern with letters or
    /// digits in it such as `*.exe` or `thumbcache_*.db`.
    pub fn named_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| match s {
                Segment::Literal(_) => true,
                Segment::Glob(text) => text.chars().any(char::is_alphanumeric),
                Segment::AnyOne => false,
            })
            .count()
    }

    /// True if any component (the drive does not count) contains a wildcard.
    pub fn has_wildcard_segments(&self) -> bool {
        self.segments
            .iter()
            .any(|s| !matches!(s, Segment::Literal(_)))
    }

    /// True if the last component is plain text or contains letters besides wildcards.
    pub fn last_is_named(&self) -> bool {
        match self.segments.last() {
            Some(Segment::Literal(_)) => true,
            Some(Segment::Glob(text)) => text.chars().any(char::is_alphanumeric),
            _ => false,
        }
    }

    /// A pattern that matches one component name at any depth, on any drive (`.git`, `*.vhdx`).
    pub fn any_component(text: &str) -> Pattern {
        Pattern {
            drive: None,
            segments: vec![Segment::parse(text)],
        }
    }

    /// A pattern for exactly this path (used for runtime additions).
    pub fn literal(path: &NormalPath) -> Pattern {
        Pattern {
            drive: Some(path.drive),
            segments: path.parts.iter().cloned().map(Segment::Literal).collect(),
        }
    }

    fn drive_matches(&self, path: &NormalPath) -> bool {
        self.drive.is_none_or(|d| d == path.drive)
    }

    fn prefix_matches(&self, path: &NormalPath) -> bool {
        self.segments
            .iter()
            .zip(&path.parts)
            .all(|(segment, part)| segment.matches(part))
    }

    /// The path is exactly what the pattern describes.
    pub fn matches_exactly(&self, path: &NormalPath) -> bool {
        self.drive_matches(path)
            && path.parts.len() == self.segments.len()
            && self.prefix_matches(path)
    }

    /// The path is what the pattern describes, or anything below it.
    pub fn matches_subtree(&self, path: &NormalPath) -> bool {
        self.drive_matches(path)
            && path.parts.len() >= self.segments.len()
            && self.prefix_matches(path)
    }

    /// The path is a folder that lies above the pattern's location (so deleting it would delete
    /// the protected location too).
    pub fn is_proper_ancestor_of_pattern(&self, path: &NormalPath) -> bool {
        self.drive_matches(path)
            && path.parts.len() < self.segments.len()
            && self.prefix_matches(path)
    }

    /// Some component of the path matches the pattern's single segment (`.git` anywhere).
    pub fn matches_any_component(&self, path: &NormalPath) -> bool {
        match self.segments.as_slice() {
            [only] => self.drive_matches(path) && path.parts.iter().any(|p| only.matches(p)),
            _ => false,
        }
    }
}

/// Iterative wildcard match (`*` only), without recursion.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some(p);
                mark = t;
                p += 1;
            }
            Some(c) if Some(c) == text.get(t) => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some(s) => {
                    p = s + 1;
                    mark += 1;
                    t = mark;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::normalize;

    fn n(text: &str) -> NormalPath {
        normalize(text).unwrap()
    }

    #[test]
    fn globs_match_as_expected() {
        assert!(glob_match("ntuser.dat*", "ntuser.dat.log1"));
        assert!(glob_match("ntuser.dat*", "ntuser.dat"));
        assert!(!glob_match("ntuser.dat*", "xntuser.dat"));
        assert!(glob_match("onedrive*", "onedrive - company"));
        assert!(glob_match("*", ""));
        assert!(glob_match("a*b*c", "aXXbYYc"));
        assert!(!glob_match("a*b*c", "aXXbYY"));
        assert!(glob_match("$*", "$mft"));
        assert!(glob_match("***", "x"));
        // Pathological input stays fast and correct.
        assert!(!glob_match(&"a*".repeat(30), &"a".repeat(10)));
    }

    #[test]
    fn subtree_and_exact_matching() {
        let p = Pattern::parse(r"*:\users\*\.ssh").unwrap();
        assert!(p.matches_exactly(&n(r"C:\Users\Bob\.ssh")));
        assert!(!p.matches_exactly(&n(r"C:\Users\Bob\.ssh\id")));
        assert!(p.matches_subtree(&n(r"D:\Users\Bob\.ssh\id_rsa")));
        assert!(!p.matches_subtree(&n(r"C:\Users\Bob")));
        assert!(!p.matches_subtree(&n(r"C:\Users\Bob\.sshx")));
    }

    #[test]
    fn a_fixed_drive_only_matches_that_drive() {
        let p = Pattern::parse(r"c:\windows").unwrap();
        assert!(p.matches_subtree(&n(r"C:\Windows\x")));
        assert!(!p.matches_subtree(&n(r"D:\Windows\x")));
    }

    #[test]
    fn root_pattern_matches_only_the_root_exactly() {
        let p = Pattern::parse(r"*:\").unwrap();
        assert!(p.matches_exactly(&n(r"E:\")));
        assert!(!p.matches_exactly(&n(r"E:\x")));
    }

    #[test]
    fn any_component_matches_at_any_depth() {
        let p = Pattern::parse(r"*:\.git").unwrap();
        assert!(p.matches_any_component(&n(r"C:\a\b\.git\hooks")));
        assert!(p.matches_any_component(&n(r"C:\.git")));
        assert!(!p.matches_any_component(&n(r"C:\a\gitx")));
    }

    #[test]
    fn bad_pattern_text_is_rejected() {
        assert!(Pattern::parse("nodrive").is_none());
        assert!(Pattern::parse("1:\\x").is_none());
        assert!(Pattern::parse("cc:\\x").is_none());
    }
}

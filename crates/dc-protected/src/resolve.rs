//! Asking the operating system what a path really is (links, junctions, 8.3 names).

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// What the resolver found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The real path, with links and short names resolved.
    Resolved(String),
    /// Nothing along the path exists at all.
    NotFound,
    /// Something went wrong; the caller must fail closed.
    Failed(String),
}

/// Resolves a path to its real location. Implementations must not modify anything.
pub trait PathResolver {
    /// Resolve `path`.
    fn resolve(&self, path: &str) -> Resolution;
}

/// Resolver backed by the real file system (read-only: `std::fs::canonicalize`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FsResolver;

impl PathResolver for FsResolver {
    fn resolve(&self, path: &str) -> Resolution {
        let original = PathBuf::from(path);
        // Walk up until some ancestor exists, remembering the names that do not.
        let mut existing: &Path = original.as_path();
        let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
        loop {
            match std::fs::canonicalize(existing) {
                Ok(real) => {
                    let mut full = strip_verbatim(real);
                    for name in tail.iter().rev() {
                        full.push(name);
                    }
                    return match full.into_os_string().into_string() {
                        Ok(text) => Resolution::Resolved(text),
                        Err(_) => Resolution::Failed("path is not valid text".to_owned()),
                    };
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    match (existing.parent(), existing.file_name()) {
                        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                            tail.push(name);
                            existing = parent;
                        }
                        _ => return Resolution::NotFound,
                    }
                }
                Err(error) => return Resolution::Failed(error.to_string()),
            }
        }
    }
}

/// `\\?\C:\x` -> `C:\x` (canonicalize on Windows returns the verbatim form). UNC stays as is.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.to_ascii_lowercase().starts_with("unc\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Scripted resolver for tests: maps exact input text to an outcome.
#[derive(Debug, Clone, Default)]
pub struct FakeResolver {
    map: BTreeMap<String, String>,
    failing: BTreeSet<String>,
}

impl FakeResolver {
    /// An empty resolver: everything is `NotFound`.
    pub fn new() -> FakeResolver {
        FakeResolver::default()
    }

    /// `input` resolves to `real`.
    #[must_use]
    pub fn link(mut self, input: &str, real: &str) -> FakeResolver {
        self.map.insert(input.to_owned(), real.to_owned());
        self
    }

    /// `input` fails to resolve.
    #[must_use]
    pub fn fail(mut self, input: &str) -> FakeResolver {
        self.failing.insert(input.to_owned());
        self
    }
}

impl PathResolver for FakeResolver {
    fn resolve(&self, path: &str) -> Resolution {
        if self.failing.contains(path) {
            Resolution::Failed("scripted failure".to_owned())
        } else if let Some(real) = self.map.get(path) {
            Resolution::Resolved(real.clone())
        } else {
            Resolution::NotFound
        }
    }
}

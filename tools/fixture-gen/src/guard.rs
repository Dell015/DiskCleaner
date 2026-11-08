//! The output-location guard. It runs before anything is written and fails closed.
//!
//! [`validate_out`] performs no writes at all, so tests can point it at a drive root safely.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::{io_err, GenError};

/// An output directory that passed every check.
#[derive(Debug, Clone)]
pub struct ValidatedOut {
    path: PathBuf,
}

impl ValidatedOut {
    /// The canonical location of the output directory (it may not exist yet).
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Check that `input` is a safe place to generate into. Performs **no writes**.
///
/// Refuses: an empty path, any `..` component, a path that cannot be resolved, a link, a
/// non-directory, a non-empty directory, and any drive or file system root.
pub fn validate_out(input: &Path) -> Result<ValidatedOut, GenError> {
    let unresolvable = || GenError::OutDirUnresolvable(input.to_path_buf());

    if input.as_os_str().is_empty() {
        return Err(unresolvable());
    }
    if input
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(GenError::OutDirHasParentRef(input.to_path_buf()));
    }
    let absolute = std::path::absolute(input).map_err(|_| unresolvable())?;
    if is_root(&absolute) {
        return Err(GenError::OutDirIsRoot(input.to_path_buf()));
    }

    match fs::symlink_metadata(&absolute) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(GenError::OutDirIsLink(input.to_path_buf()));
            }
            if !meta.is_dir() {
                return Err(GenError::OutDirNotADirectory(input.to_path_buf()));
            }
            let mut entries = fs::read_dir(&absolute).map_err(|_| unresolvable())?;
            if entries.next().is_some() {
                return Err(GenError::OutDirNotEmpty(input.to_path_buf()));
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(unresolvable()),
    }

    let canonical = canonicalize_lenient(&absolute).ok_or_else(unresolvable)?;
    if is_root(&absolute) || is_root(&canonical) {
        return Err(GenError::OutDirIsRoot(input.to_path_buf()));
    }
    Ok(ValidatedOut { path: canonical })
}

/// Create the output directory and its fresh `tree/` subdirectory, returning the tree path.
///
/// `tree/` is made with `create_dir` (not `create_dir_all`), so it fails if it already exists.
/// That closes the gap between "checked that it is empty" and "started writing".
pub fn create_tree(out: &ValidatedOut) -> Result<PathBuf, GenError> {
    fs::create_dir_all(out.path()).map_err(io_err("create output directory", "<out>"))?;
    let tree = out.path().join("tree");
    fs::create_dir(&tree).map_err(io_err("create tree directory", "tree"))?;
    Ok(tree)
}

/// Canonicalize a path that may not exist yet: resolve the nearest existing ancestor and
/// append the remaining components.
fn canonicalize_lenient(path: &Path) -> Option<PathBuf> {
    let mut tail: Vec<OsString> = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        match fs::canonicalize(&current) {
            Ok(mut resolved) => {
                for part in tail.iter().rev() {
                    resolved.push(part);
                }
                return Some(resolved);
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                tail.push(current.file_name()?.to_owned());
                if !current.pop() {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

/// A path with no parent is a root: `/`, `C:\`, `\\?\C:\`, or a UNC share root.
fn is_root(path: &Path) -> bool {
    path.parent().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_path_is_refused() {
        assert!(matches!(
            validate_out(Path::new("")),
            Err(GenError::OutDirUnresolvable(_))
        ));
    }

    #[test]
    fn parent_components_are_refused() {
        assert!(matches!(
            validate_out(Path::new("a/../b")),
            Err(GenError::OutDirHasParentRef(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn file_system_root_is_refused_without_writing() {
        assert!(matches!(
            validate_out(Path::new("/")),
            Err(GenError::OutDirIsRoot(_))
        ));
    }

    #[test]
    fn root_detection() {
        #[cfg(unix)]
        assert!(is_root(Path::new("/")));
        #[cfg(windows)]
        {
            assert!(is_root(Path::new(r"C:\")));
            assert!(is_root(Path::new(r"\\?\C:\")));
        }
        assert!(!is_root(&std::env::temp_dir().join("x")));
    }
}

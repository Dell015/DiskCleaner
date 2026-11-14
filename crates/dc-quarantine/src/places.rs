//! Where held items go, and what may never be touched.

use std::path::{Path, PathBuf};

use dc_protected::{FsResolver, ProtectedList};

/// Which holding folder an item goes to. It must be on the same drive as the item, or the move
/// fails (and the item is skipped): nothing is ever copied between drives.
pub trait Places: Sync {
    /// The holding folder for `item`, or `None` if it has none (network paths, odd paths).
    fn holding_root(&self, item: &Path) -> Option<PathBuf>;
}

/// A last word on paths: protected places are refused.
pub trait Guard: Sync {
    /// `Some(reason)` if `path` (already resolved, no links) must not be touched.
    fn refuse(&self, path: &Path) -> Option<String>;
    /// `Some(reason)` if `path` is not in one of the person's own places, so it cannot be removed
    /// as "your own files" (only as a DiskCleaner finding).
    fn refuse_own(&self, path: &Path) -> Option<String>;
}

/// Windows: the profile's drive holds items in `%LOCALAPPDATA%\DiskCleaner\Holding`, every other
/// drive in a hidden `X:\.diskcleaner-holding`.
#[derive(Debug, Clone)]
pub struct WindowsPlaces {
    profile_holding: PathBuf,
    profile_drive: Option<char>,
}

/// The drive letter of a path (`C:\...` or `\\?\C:\...`), upper case.
#[must_use]
pub fn drive_of(path: &Path) -> Option<char> {
    let text = path.to_string_lossy();
    let rest = text.strip_prefix(r"\\?\").unwrap_or(&text);
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':') && matches!(chars.next(), Some('\\' | '/')))
        .then(|| letter.to_ascii_uppercase())
}

impl WindowsPlaces {
    /// `local_app_data` is `%LOCALAPPDATA%` (for example `C:\Users\me\AppData\Local`).
    #[must_use]
    pub fn new(local_app_data: &Path) -> Self {
        Self {
            // Written with backslashes so it is the same on every system that builds it.
            profile_holding: PathBuf::from(format!(
                "{}\\DiskCleaner\\Holding",
                local_app_data
                    .to_string_lossy()
                    .trim_end_matches(['\\', '/'])
            )),
            profile_drive: drive_of(local_app_data),
        }
    }
}

impl Places for WindowsPlaces {
    fn holding_root(&self, item: &Path) -> Option<PathBuf> {
        let drive = drive_of(item)?;
        if Some(drive) == self.profile_drive {
            Some(self.profile_holding.clone())
        } else {
            Some(PathBuf::from(format!("{drive}:\\.diskcleaner-holding")))
        }
    }
}

/// The compiled-in protected list, checked on the path as given and where the OS says it is.
#[derive(Debug, Clone)]
pub struct ProtectedGuard {
    list: ProtectedList,
}

impl Default for ProtectedGuard {
    fn default() -> Self {
        Self {
            list: ProtectedList::builtin(),
        }
    }
}

impl Guard for ProtectedGuard {
    fn refuse(&self, path: &Path) -> Option<String> {
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        let verdict = self.list.check(text, &FsResolver);
        verdict.protected.then(|| {
            verdict.reasons.first().map_or_else(
                || "it is protected".to_owned(),
                |r| format!("it is protected: {}", r.description),
            )
        })
    }

    fn refuse_own(&self, path: &Path) -> Option<String> {
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        dc_protected::not_your_own(text)
            .map(|why| format!("it is not one of your own files: {why}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_go_to_a_holding_folder_on_their_own_drive() {
        let p = WindowsPlaces::new(Path::new(r"C:\Users\me\AppData\Local"));
        assert_eq!(
            p.holding_root(Path::new(r"C:\Users\me\Downloads\x.zip")),
            Some(PathBuf::from(
                r"C:\Users\me\AppData\Local\DiskCleaner\Holding"
            ))
        );
        assert_eq!(
            p.holding_root(Path::new(r"\\?\e:\Games\old")),
            Some(PathBuf::from(r"E:\.diskcleaner-holding"))
        );
        assert_eq!(p.holding_root(Path::new(r"\\server\share\x")), None);
        assert_eq!(p.holding_root(Path::new("relative")), None);
    }

    #[test]
    fn the_protected_list_refuses_windows_and_odd_paths() {
        let g = ProtectedGuard::default();
        assert!(g.refuse(Path::new(r"C:\Windows\System32")).is_some());
        assert!(g.refuse(Path::new(r"\\?\C:\Windows")).is_some());
        // Anything the list cannot understand is refused (fail closed).
        assert!(g.refuse(Path::new("/tmp/x")).is_some());
    }

    #[test]
    fn program_folders_and_your_protected_folders_are_never_your_own_files() {
        let g = ProtectedGuard::default();
        // Finding 3 of the B9 review: a child of Program Files is not "your own".
        assert!(g
            .refuse_own(Path::new(r"C:\Program Files\SomeTool"))
            .is_some());
        assert!(g.refuse_own(Path::new(r"\\?\C:\ProgramData\X")).is_some());
        assert!(g
            .refuse_own(Path::new(r"C:\Users\me\Downloads\a.zip"))
            .is_none());
        // Finding 7: Documents, Desktop, Pictures, Music and Videos stay protected.
        for p in ["Documents", "Desktop", "Pictures", "Music", "Videos"] {
            let path = format!(r"C:\Users\me\{p}\x.txt");
            assert!(g.refuse(Path::new(&path)).is_some(), "{path}");
        }
        assert!(g
            .refuse(Path::new(r"C:\Users\me\Downloads\a.zip"))
            .is_none());
    }
}

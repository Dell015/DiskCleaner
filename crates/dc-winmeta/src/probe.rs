//! [`WindowsProbe`]: file identity and allocated size, read without touching file contents.

use std::ffi::c_void;
use std::fs::Metadata;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;

use dc_scan::{flags_from_windows_attributes, FileId, FileProbe, Probe};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileStandardInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    OPEN_EXISTING,
};

/// Probe backed by the Win32 file APIs.
///
/// It opens each file with `FILE_READ_ATTRIBUTES` only (no read or write access to the data),
/// with `FILE_FLAG_OPEN_REPARSE_POINT` so that links and cloud placeholders are neither
/// followed nor downloaded, and with full sharing so it never blocks other programs.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsProbe;

impl FileProbe for WindowsProbe {
    fn probe(&self, path: &Path, _meta: &Metadata) -> io::Result<Probe> {
        let wide = to_wide(path)?;

        // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the call. The security
        // attributes and template handle may be null. The call has no other preconditions.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `handle` is a valid, open handle that nothing else owns. `OwnedHandle` takes
        // ownership and closes it exactly once when dropped.
        let owned = unsafe { OwnedHandle::from_raw_handle(handle) };

        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the handle is valid for the duration of the call and `info` is a properly
        // sized, writable BY_HANDLE_FILE_INFORMATION.
        let ok = unsafe { GetFileInformationByHandle(owned.as_raw_handle(), &mut info) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut standard = FILE_STANDARD_INFO::default();
        // SAFETY: the handle is valid; `standard` is a writable FILE_STANDARD_INFO and the
        // size passed is exactly its size, as `FileStandardInfo` requires.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                owned.as_raw_handle(),
                FileStandardInfo,
                std::ptr::from_mut(&mut standard).cast::<c_void>(),
                u32::try_from(std::mem::size_of::<FILE_STANDARD_INFO>()).unwrap_or(0),
            )
        };
        // If this second query fails the allocation size stays unknown; the scanner then
        // falls back to an estimate. The identity above is still valid.
        let size_on_disk = (ok != 0)
            .then(|| normalize_allocation(u64::try_from(standard.AllocationSize).unwrap_or(0)));

        let file_index = (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow);
        Ok(Probe {
            file_id: Some(FileId {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: file_index,
            }),
            link_count: info.nNumberOfLinks,
            size_on_disk,
            flags: flags_from_windows_attributes(info.dwFileAttributes),
        })
    }
}

/// Real disk allocations are whole sectors (multiples of 512 bytes). NTFS reports a smaller,
/// unaligned number for tiny files stored inside the master file table (no clusters of their
/// own). Explorer counts those as 0 bytes on disk, so we do too.
fn normalize_allocation(allocation: u64) -> u64 {
    if allocation.is_multiple_of(512) {
        allocation
    } else {
        0
    }
}

/// UTF-16, NUL-terminated, with the `\\?\` prefix added to absolute drive paths so that paths
/// longer than 260 characters work. Paths that already carry a prefix are left alone.
fn to_wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = Vec::new();
    let text = path.as_os_str();
    let units: Vec<u16> = text.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains NUL",
        ));
    }
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    let is_drive_absolute = units.len() >= 3
        && units.get(1) == Some(&u16::from(b':'))
        && units.get(2) == Some(&u16::from(b'\\'))
        && units
            .first()
            .is_some_and(|c| u8::try_from(*c).is_ok_and(|b| b.is_ascii_alphabetic()));
    if !units.starts_with(&prefix) && is_drive_absolute {
        wide.extend_from_slice(&prefix);
    }
    wide.extend_from_slice(&units);
    wide.push(0);
    Ok(wide)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(wide: &[u16]) -> String {
        String::from_utf16_lossy(&wide[..wide.len() - 1])
    }

    #[test]
    fn mft_resident_files_count_as_zero_on_disk() {
        assert_eq!(normalize_allocation(672), 0);
        assert_eq!(normalize_allocation(0), 0);
        assert_eq!(normalize_allocation(4096), 4096);
        assert_eq!(normalize_allocation(8192), 8192);
        assert_eq!(normalize_allocation(1024), 1024);
    }

    #[test]
    fn drive_paths_get_the_verbatim_prefix() {
        assert_eq!(text(&to_wide(Path::new(r"C:\a\b")).unwrap()), r"\\?\C:\a\b");
    }

    #[test]
    fn prefixed_and_relative_paths_are_left_alone() {
        assert_eq!(text(&to_wide(Path::new(r"\\?\C:\a")).unwrap()), r"\\?\C:\a");
        assert_eq!(text(&to_wide(Path::new(r"rel\x")).unwrap()), r"rel\x");
        assert_eq!(*to_wide(Path::new("x")).unwrap().last().unwrap(), 0);
    }
}

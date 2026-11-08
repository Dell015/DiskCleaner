//! [`WindowsProbe`]: file identity and allocated size, read without touching file contents.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs::Metadata;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::Mutex;

use dc_scan::{flags_from_windows_attributes, FileId, FileProbe, Probe};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileStandardInfo, GetDiskFreeSpaceW, GetFileInformationByHandle,
    GetFileInformationByHandleEx, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_STANDARD_INFO, OPEN_EXISTING,
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
        let size_on_disk = (ok != 0).then(|| {
            normalize_allocation(
                u64::try_from(standard.AllocationSize).unwrap_or(0),
                cluster_size_for(path),
            )
        });

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

/// Cluster size per volume root (for example `C:\`), looked up once and remembered.
static CLUSTER_SIZES: Mutex<BTreeMap<String, Option<u64>>> = Mutex::new(BTreeMap::new());

/// The cluster size of the volume holding `path`, or `None` if it cannot be determined
/// (network paths, volumes mounted on folders, errors). Never fails the scan.
fn cluster_size_for(path: &Path) -> Option<u64> {
    let root = volume_root(path)?;
    let mut cache = CLUSTER_SIZES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(known) = cache.get(&root) {
        return *known;
    }
    let found = query_cluster_size(&root);
    cache.insert(root, found);
    found
}

fn query_cluster_size(root: &str) -> Option<u64> {
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let (mut sectors, mut bytes, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the call and the four
    // out-pointers refer to writable u32 values on this stack frame.
    let ok = unsafe {
        GetDiskFreeSpaceW(
            wide.as_ptr(),
            &mut sectors,
            &mut bytes,
            &mut free,
            &mut total,
        )
    };
    if ok == 0 {
        return None;
    }
    let size = u64::from(sectors) * u64::from(bytes);
    (size > 0).then_some(size)
}

/// `C:\` for `C:\...` and `\\?\C:\...`; `None` for anything else.
fn volume_root(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    let mut chars = text.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':') && chars.next() == Some('\\'))
        .then(|| format!("{}:\\", drive.to_ascii_uppercase()))
}

/// Turn the raw allocation into "bytes on disk" the way Explorer does.
///
/// Real allocations are whole clusters. NTFS keeps tiny files inside the master file table and
/// reports a smaller number for them (sometimes unaligned, sometimes a multiple of 512 but less
/// than one cluster); they use no clusters, so they count as 0. If the cluster size is unknown
/// the weaker rule "must be a whole number of 512-byte sectors" is used.
fn normalize_allocation(allocation: u64, cluster: Option<u64>) -> u64 {
    match cluster {
        Some(cluster) if cluster > 0 => {
            if allocation >= cluster && allocation.is_multiple_of(cluster) {
                allocation
            } else {
                0
            }
        }
        _ if allocation.is_multiple_of(512) => allocation,
        _ => 0,
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
        // Known cluster size: only whole clusters count.
        assert_eq!(normalize_allocation(672, Some(4096)), 0);
        assert_eq!(normalize_allocation(512, Some(4096)), 0);
        assert_eq!(normalize_allocation(0, Some(4096)), 0);
        assert_eq!(normalize_allocation(4096, Some(4096)), 4096);
        assert_eq!(normalize_allocation(12288, Some(4096)), 12288);
        assert_eq!(normalize_allocation(6144, Some(4096)), 0);
        assert_eq!(normalize_allocation(65536, Some(65536)), 65536);
        // Unknown cluster size: fall back to whole sectors.
        assert_eq!(normalize_allocation(672, None), 0);
        assert_eq!(normalize_allocation(1024, None), 1024);
    }

    #[test]
    fn volume_roots_are_found_for_drive_paths_only() {
        assert_eq!(volume_root(Path::new(r"c:\a\b")).as_deref(), Some(r"C:\"));
        assert_eq!(volume_root(Path::new(r"\\?\E:\x")).as_deref(), Some(r"E:\"));
        assert_eq!(volume_root(Path::new(r"\\server\share\x")), None);
        assert_eq!(volume_root(Path::new(r"rel\x")), None);
    }

    #[test]
    fn the_cluster_size_of_the_current_drive_is_sane() {
        let dir = std::env::current_dir().unwrap();
        if let Some(size) = cluster_size_for(&dir) {
            assert!(size.is_power_of_two() && size >= 512, "got {size}");
        }
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

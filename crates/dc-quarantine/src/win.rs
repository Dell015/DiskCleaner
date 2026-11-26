//! Windows-only low-level calls for moving safely. This is the one file in `dc-quarantine`
//! allowed to use `unsafe` (approved by the owner on 2026-11-14, D119). Each block carries a
//! `// SAFETY:` comment. Only `ops.rs` calls into this file.
//!
//! - [`move_checked`]: open the item itself (never following a link), check that its real
//!   location is the path we were given, then rename *that open item* without replacing
//!   anything. A folder above it that is swapped for a junction between the checks and the
//!   move cannot redirect it: the move acts on what was opened and checked.
//! - [`hide_and_protect`]: a new holding folder is hidden and only its owner, SYSTEM and
//!   administrators may use it (a drive-root folder would otherwise inherit "Authenticated
//!   Users: Modify" and other local users could plant links in it).
//! - Moving an app (B11, D141): [`make_junction`] (a new folder made into a junction to the
//!   app's new place), [`read_junction`] (where one points), [`open_alone`] (is a file in use?),
//!   [`volume`] (file system, free space, removable or network), [`drive_letters`].

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FileRenameInfo, GetDriveTypeW, GetFileAttributesW, GetFileInformationByHandle,
    GetFinalPathNameByHandleW, MoveFileExW, SetFileAttributesW, SetFileInformationByHandle,
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_NAME_NORMALIZED,
    FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, SYNCHRONIZE, VOLUME_NAME_DOS,
};

use crate::check::same_key;

/// `C:\x` as a NUL-terminated wide string in the `\\?\C:\x` form (no 260-character limit).
/// Paths already in a `\\` form are kept as they are.
fn wide(path: &Path) -> Vec<u16> {
    let text = path.as_os_str().to_string_lossy();
    let mut s: Vec<u16> = Vec::new();
    if !text.starts_with(r"\\") {
        s.extend(r"\\?\".encode_utf16());
        s.extend(path.as_os_str().encode_wide().map(|c| {
            if c == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                c
            }
        }));
    } else {
        s.extend(path.as_os_str().encode_wide());
    }
    s.push(0);
    s
}

/// An open handle, closed when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a handle returned by CreateFileW that was not closed yet; it is
        // closed exactly once, here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Open `path` itself (a link is opened as the link, never followed) with the right to move it.
fn open_for_move(path: &Path) -> io::Result<Handle> {
    let name = wide(path);
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; there are no
    // security attributes and no template handle (both null).
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(h))
}

/// Where an open item really is (links and short names resolved), in the `\\?\C:\...` form.
fn final_path(h: &Handle) -> io::Result<PathBuf> {
    let mut buf: Vec<u16> = vec![0; 512];
    loop {
        let size = u32::try_from(buf.len()).map_err(|_| io::Error::other("path too long"))?;
        // SAFETY: `buf` has room for `size` UTF-16 units; the call writes at most that many and
        // returns the length written, or the size needed when it does not fit.
        let n = unsafe {
            GetFinalPathNameByHandleW(
                h.0,
                buf.as_mut_ptr(),
                size,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        let n = n as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            return Ok(PathBuf::from(OsString::from_wide(&buf[..n])));
        }
        if n > 32_768 {
            return Err(io::Error::other("path too long"));
        }
        buf.resize(n + 1, 0);
    }
}

/// The attributes of an open item.
fn attributes(h: &Handle) -> io::Result<u32> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `info` is a valid, writable BY_HANDLE_FILE_INFORMATION for the call to fill.
    let ok = unsafe { GetFileInformationByHandle(h.0, &mut info) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info.dwFileAttributes)
}

/// Rename the open item to `to`; fails if something is already there.
fn rename_handle(h: &Handle, to: &Path) -> io::Result<()> {
    let name = wide(to);
    let units = name.len() - 1; // without the NUL
                                // The whole struct plus the name with its NUL (a few bytes more than needed, as Rust's own
                                // std does); the name starts at `FileName`.
    let size = std::mem::size_of::<FILE_RENAME_INFO>() + name.len() * 2;
    // u64 storage: aligned for FILE_RENAME_INFO (8 bytes on 64-bit), zeroed.
    let mut buf: Vec<u64> = vec![0; size.div_ceil(8)];
    let info = buf.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    let name_bytes = u32::try_from(units * 2).map_err(|_| io::Error::other("path too long"))?;
    let size_u32 = u32::try_from(size).map_err(|_| io::Error::other("path too long"))?;
    // SAFETY: `buf` is zeroed, 8-byte aligned and at least `size` bytes long: the whole struct
    // plus the name and its NUL, so the name copied to `FileName` and every write stay inside it.
    unsafe {
        (*info).Anonymous.Flags = 0; // ReplaceIfExists = false
        (*info).RootDirectory = ptr::null_mut();
        (*info).FileNameLength = name_bytes;
        let dst = ptr::addr_of_mut!((*info).FileName).cast::<u16>();
        ptr::copy_nonoverlapping(name.as_ptr(), dst, name.len());
    }
    // SAFETY: `info` points to `size` initialised bytes laid out as FILE_RENAME_INFO followed by
    // the name; `h` is an open handle with DELETE access.
    let ok = unsafe { SetFileInformationByHandle(h.0, FileRenameInfo, info.cast(), size_u32) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Move `from` to `to` on the same drive: never a copy, never replacing anything, and only if
/// `from` is really where it says it is (no link at it or above it).
pub fn move_checked(from: &Path, to: &Path) -> io::Result<()> {
    let h = open_for_move(from)?;
    if attributes(&h)? & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "it is a link or junction (just now)",
        ));
    }
    let real = final_path(&h)?;
    if same_key(&real) != same_key(from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder above it became a link just now: its real location is elsewhere",
        ));
    }
    match rename_handle(&h, to) {
        Ok(()) => Ok(()),
        Err(e)
            if matches!(
                e.raw_os_error().and_then(|c| u32::try_from(c).ok()),
                Some(ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED | ERROR_INVALID_FUNCTION)
            ) =>
        {
            // Some file systems cannot rename by handle. Rename by name, still never replacing
            // and never copying (the check above was made a moment ago).
            drop(h);
            let (a, b) = (wide(from), wide(to));
            // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call; flags 0 means
            // no copy between drives and no replacing.
            let ok = unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Is drive `letter` a network drive (a mapped share)? Such drives get no holding folder: they
/// may be gone next time and their permissions cannot be set.
pub fn is_network_drive(letter: char) -> bool {
    const DRIVE_REMOTE: u32 = 4; // winbase.h
    let root: Vec<u16> = format!("{letter}:\\")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `root` is a NUL-terminated UTF-16 string ("X:\") that outlives the call.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
}

/// Hide a new holding folder and let only its owner, SYSTEM and administrators use it.
pub fn hide_and_protect(dir: &Path) -> io::Result<()> {
    let name = wide(dir);
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
    let attrs = unsafe { GetFileAttributesW(name.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the holding folder is a link",
        ));
    }
    // SAFETY: as above; only the hidden attribute is added.
    let hidden = unsafe { SetFileAttributesW(name.as_ptr(), attrs | FILE_ATTRIBUTE_HIDDEN) };
    let hide_error = (hidden == 0).then(io::Error::last_os_error);
    // Owner rights, SYSTEM and administrators: full control, inherited by everything inside;
    // nothing inherited from the drive.
    let sddl: Vec<u16> = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; `sd` receives a descriptor allocated with LocalAlloc,
    // freed below; the size output is not needed (null).
    let made = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            ptr::null_mut(),
        )
    };
    if made == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `name` is NUL-terminated and `sd` is the valid descriptor made above.
    let set = unsafe {
        SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            sd,
        )
    };
    let set_error = (set == 0).then(io::Error::last_os_error);
    // SAFETY: `sd` was allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW with
    // LocalAlloc and is freed exactly once, here.
    unsafe {
        LocalFree(sd);
    }
    match (set_error, hide_error) {
        (Some(e), _) | (None, Some(e)) => Err(e),
        (None, None) => Ok(()),
    }
}

/// Start a program, or open a Windows settings page, the way double-clicking does: with its own
/// window, and with Windows' permission question when the program needs administrator rights
/// (D133: only an installed app's own uninstaller, or the Installed apps page). Never through a
/// shell: `args` goes to the program as it is.
pub fn shell_open(target: &Path, args: Option<&str>) -> io::Result<()> {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Plain form (ShellExecute does not take `\\?\` paths).
    let file: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let params: Option<Vec<u16>> = args
        .filter(|a| !a.is_empty())
        .map(|a| a.encode_utf16().chain(std::iter::once(0)).collect());
    let info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
        fMask: SEE_MASK_NOASYNC,
        lpFile: file.as_ptr(),
        lpParameters: params.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
        nShow: SW_SHOWNORMAL,
        ..Default::default()
    };
    let mut info = info;
    // Microsoft: COM must be set up on the calling thread first (opening a settings page or
    // a program can go through shell extensions). Undone below when this call set it up.
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    // SAFETY: plain call with a null reserved pointer and valid flags.
    let com = unsafe {
        CoInitializeEx(
            ptr::null(),
            (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
        )
    };
    // SAFETY: `info` is a correctly sized SHELLEXECUTEINFOW; `file` and `params` are
    // NUL-terminated UTF-16 strings that outlive the call; no handle is asked for
    // (no SEE_MASK_NOCLOSEPROCESS), so nothing has to be closed afterwards.
    let ok = unsafe { ShellExecuteExW(&mut info) };
    // Read the error before anything else can change it.
    let err = (ok == 0).then(io::Error::last_os_error);
    if com >= 0 {
        // SAFETY: balances the successful CoInitializeEx above (S_OK or S_FALSE) on this thread.
        unsafe { CoUninitialize() };
    }
    if let Some(e) = err {
        const ERROR_CANCELLED: i32 = 1223;
        if e.raw_os_error() == Some(ERROR_CANCELLED) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Windows' permission question was answered No",
            ));
        }
        return Err(e);
    }
    Ok(())
}

// ---------- Moving an app (B11) ----------

/// The `\??\D:\x` form a junction stores, from `D:\x` or `\\?\D:\x`.
fn nt_target(target: &Path) -> io::Result<String> {
    let text = target.to_string_lossy();
    let plain = text
        .strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('/', "\\");
    let mut chars = plain.chars();
    let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.next() == Some(':')
        && chars.next() == Some('\\');
    if !ok {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a link can only point to a full path on a drive with a letter",
        ));
    }
    Ok(plain)
}

/// Make a junction at `link` (a new, empty folder) pointing to the folder `target`.
pub fn make_junction(link: &Path, target: &Path) -> io::Result<()> {
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::Storage::FileSystem::{CreateDirectoryW, RemoveDirectoryW};
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let print = nt_target(target)?;
    let data = crate::mover::mount_point_buffer(&print)?;
    let name = wide(link);
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; no security
    // attributes (null). Fails when anything already exists at `link`.
    if unsafe { CreateDirectoryW(name.as_ptr(), ptr::null()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let set = || -> io::Result<()> {
        // SAFETY: as above; the new folder itself is opened (not followed), for writing its
        // reparse data; no template handle.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let h = Handle(h);
        let len = u32::try_from(data.len()).map_err(|_| io::Error::other("link too long"))?;
        let mut returned = 0u32;
        // SAFETY: `data` is a complete REPARSE_DATA_BUFFER of `len` bytes that outlives the
        // call; no output buffer; `returned` is writable; no overlapped I/O (null).
        let ok = unsafe {
            DeviceIoControl(
                h.0,
                FSCTL_SET_REPARSE_POINT,
                data.as_ptr().cast(),
                len,
                ptr::null_mut(),
                0,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    };
    if let Err(e) = set() {
        // Take away the empty folder just made (RemoveDirectoryW refuses a folder that is not
        // empty, so nothing else can go).
        // SAFETY: `name` is NUL-terminated and outlives the call.
        unsafe {
            RemoveDirectoryW(name.as_ptr());
        }
        return Err(e);
    }
    Ok(())
}

const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
/// MAXIMUM_REPARSE_DATA_BUFFER_SIZE.
const REPARSE_MAX: usize = 16 * 1024;

/// Where the junction at `link` points (`D:\x`); `Ok(None)` when `link` is not a reparse point;
/// an error for any other kind of link (symbolic links, cloud files, ...).
pub fn read_junction(link: &Path) -> io::Result<Option<PathBuf>> {
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let name = wide(link);
    // SAFETY: `name` is NUL-terminated and outlives the call; the item itself is opened (not
    // followed) only to read its attributes and reparse data; no template handle.
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let h = Handle(h);
    if attributes(&h)? & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; REPARSE_MAX];
    let mut returned = 0u32;
    let size = u32::try_from(buf.len()).map_err(|_| io::Error::other("buffer"))?;
    // SAFETY: no input buffer; `buf` is writable for `size` bytes; `returned` is writable; no
    // overlapped I/O.
    let ok = unsafe {
        DeviceIoControl(
            h.0,
            FSCTL_GET_REPARSE_POINT,
            ptr::null(),
            0,
            buf.as_mut_ptr().cast(),
            size,
            &mut returned,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(usize::try_from(returned).unwrap_or(0).min(REPARSE_MAX));
    crate::mover::parse_mount_point(&buf)
        .map(|t| Some(PathBuf::from(t)))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "it is a different kind of link than the one Rhizome makes",
            )
        })
}

/// Open `file` for reading with no sharing: fails with a sharing violation while any program
/// has it open. Nothing is read or changed; the handle is closed at once.
pub fn open_alone(file: &Path) -> io::Result<()> {
    use windows_sys::Win32::Foundation::GENERIC_READ;
    let name = wide(file);
    // SAFETY: `name` is NUL-terminated and outlives the call; share mode 0 asks for the file
    // alone; the item itself is opened (a link is not followed); no template handle.
    let h = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            0,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    drop(Handle(h));
    Ok(())
}

/// The drive holding `path`: file system, free space, removable or network.
pub fn volume(path: &Path) -> io::Result<crate::ops::Volume> {
    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW};
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_REMOTE: u32 = 4;
    let letter = crate::places::drive_of(path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "it is not on a drive with a letter",
        )
    })?;
    let root: Vec<u16> = format!("{letter}:\\")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut fs_name = [0u16; 64];
    let fs_len = u32::try_from(fs_name.len()).unwrap_or(64);
    // SAFETY: `root` is NUL-terminated; only the file system name is asked for, into
    // `fs_name` (writable for `fs_len` units); every other output is null.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            fs_name.as_mut_ptr(),
            fs_len,
        )
    };
    let filesystem = if ok == 0 {
        String::new()
    } else {
        let end = fs_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(fs_name.len());
        String::from_utf16_lossy(&fs_name[..end])
    };
    let (mut avail, mut total, mut free_all) = (0u64, 0u64, 0u64);
    // SAFETY: `root` is NUL-terminated; the three outputs are writable u64 values.
    let ok = unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut avail, &mut total, &mut free_all) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `root` is NUL-terminated.
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    Ok(crate::ops::Volume {
        filesystem,
        free: avail,
        total,
        removable: kind == DRIVE_REMOVABLE,
        network: kind == DRIVE_REMOTE,
    })
}

/// The drive letters Windows has right now (a bit mask read once; no drive is touched, so an
/// empty card reader or DVD drive is not asked for a disk).
pub fn drive_letters() -> Vec<char> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    // SAFETY: a plain call without arguments; it returns a bit mask (0 on failure).
    let mask = unsafe { GetLogicalDrives() };
    (0u8..26)
        .filter(|k| mask & (1 << k) != 0)
        .map(|k| char::from(b'A' + k))
        .collect()
}

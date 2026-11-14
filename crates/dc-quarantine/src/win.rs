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
    CreateFileW, FileRenameInfo, GetFileAttributesW, GetFileInformationByHandle,
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

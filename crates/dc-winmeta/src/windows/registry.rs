//! Read-only registry access. Keys are opened with `KEY_READ` (plus a 32/64-bit view flag) and
//! nothing here calls any registry function that writes, creates or deletes.

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, KEY_READ, REG_DWORD,
    REG_EXPAND_SZ, REG_SZ,
};

use crate::model::{Problem, RawUninstallEntry, RegistrySource};
use crate::parse::expand_env;
use crate::source::SourceReport;

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
/// Registry strings longer than this are ignored (real values are a few hundred bytes).
const MAX_VALUE_BYTES: u32 = 64 * 1024;
/// Safety valve against a corrupt registry: at most this many subkeys are enumerated.
const MAX_SUBKEYS: u32 = 100_000;
/// Registry key names are at most 255 characters.
const NAME_BUFFER: usize = 256;

/// An open key, closed exactly once when dropped.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: `self.0` was returned by a successful `RegOpenKeyExW` and is closed only here.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Open `path` under `root` for reading. Returns the Windows error code on failure.
fn open(root: HKEY, path: &str, view: u32) -> Result<Key, u32> {
    let path = wide(path);
    let mut handle: HKEY = std::ptr::null_mut();
    // SAFETY: `path` is a NUL-terminated UTF-16 buffer that outlives the call, and `handle` is
    // a writable HKEY. `KEY_READ` requests read-only access.
    let status = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ | view, &mut handle) };
    if status == ERROR_SUCCESS {
        Ok(Key(handle))
    } else {
        Err(status)
    }
}

/// Names of the direct subkeys of `key`.
fn subkey_names(key: &Key) -> Result<Vec<String>, u32> {
    let mut names = Vec::new();
    let mut index = 0u32;
    while index < MAX_SUBKEYS {
        let mut buffer = [0u16; NAME_BUFFER];
        let mut length = u32::try_from(NAME_BUFFER).unwrap_or(256);
        // SAFETY: `buffer` is writable for `length` UTF-16 units; the optional class and
        // time-stamp parameters may be null; `key.0` is a valid open key.
        let status = unsafe {
            RegEnumKeyExW(
                key.0,
                index,
                buffer.as_mut_ptr(),
                &mut length,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        match status {
            ERROR_SUCCESS => {
                let used = usize::try_from(length).unwrap_or(0).min(NAME_BUFFER);
                names.push(String::from_utf16_lossy(&buffer[..used]));
            }
            ERROR_MORE_DATA => {}
            ERROR_NO_MORE_ITEMS => break,
            other => return Err(other),
        }
        index += 1;
    }
    Ok(names)
}

/// The type and raw bytes of one value, or `None` if it is missing, empty or too large.
fn query(key: &Key, name: &str) -> Option<(u32, Vec<u8>)> {
    let name = wide(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    // SAFETY: `name` is NUL-terminated and outlives the call; a null data pointer asks only
    // for the type and size, which are written to the two local variables.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if (status != ERROR_SUCCESS && status != ERROR_MORE_DATA) || size == 0 || size > MAX_VALUE_BYTES
    {
        return None;
    }
    let mut data = vec![0u8; usize::try_from(size).ok()?];
    let mut written = size;
    // SAFETY: `data` is writable for `written` bytes (its exact length), and the other
    // pointers are as in the call above.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            data.as_mut_ptr(),
            &mut written,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    data.truncate(usize::try_from(written).ok()?.min(data.len()));
    Some((kind, data))
}

fn read_string(key: &Key, name: &str) -> Option<String> {
    let (kind, data) = query(key, name)?;
    if kind != REG_SZ && kind != REG_EXPAND_SZ {
        return None;
    }
    let (pairs, _) = data.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    let mut text = String::from_utf16_lossy(&units);
    if kind == REG_EXPAND_SZ {
        text = expand_env(&text, &|var| std::env::var(var).ok());
    }
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

fn read_dword(key: &Key, name: &str) -> Option<u32> {
    let (kind, data) = query(key, name)?;
    if kind != REG_DWORD {
        return None;
    }
    let bytes: [u8; 4] = data.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_entry(key: &Key, name: &str, source: RegistrySource) -> RawUninstallEntry {
    RawUninstallEntry {
        key: name.to_owned(),
        source: Some(source),
        display_name: read_string(key, "DisplayName"),
        publisher: read_string(key, "Publisher"),
        display_version: read_string(key, "DisplayVersion"),
        install_location: read_string(key, "InstallLocation"),
        display_icon: read_string(key, "DisplayIcon"),
        uninstall_string: read_string(key, "UninstallString"),
        estimated_size_kb: read_dword(key, "EstimatedSize"),
        install_date: read_string(key, "InstallDate")
            .or_else(|| read_dword(key, "InstallDate").map(|d| d.to_string())),
        system_component: read_dword(key, "SystemComponent") == Some(1),
        windows_installer: read_dword(key, "WindowsInstaller") == Some(1),
        parent_key_name: read_string(key, "ParentKeyName"),
        release_type: read_string(key, "ReleaseType"),
    }
}

/// Read every uninstall entry under one registry root and view. A missing key is normal (empty
/// result); anything else that fails becomes a problem and the rest is still read.
pub fn read_uninstall(
    root: HKEY,
    view: u32,
    source: RegistrySource,
    label: &str,
) -> SourceReport<Vec<RawUninstallEntry>> {
    let mut report = SourceReport::<Vec<RawUninstallEntry>>::default();
    let problem = |message: String| Problem {
        source: label.to_owned(),
        message,
    };

    let base = match open(root, UNINSTALL, view) {
        Ok(key) => key,
        Err(code) if code == ERROR_FILE_NOT_FOUND => return report,
        Err(code) => {
            report.problems.push(problem(format!(
                "cannot open the Uninstall key (error {code})"
            )));
            return report;
        }
    };
    let names = match subkey_names(&base) {
        Ok(names) => names,
        Err(code) => {
            report
                .problems
                .push(problem(format!("cannot list entries (error {code})")));
            return report;
        }
    };
    for name in names {
        match open(root, &format!("{UNINSTALL}\\{name}"), view) {
            Ok(key) => report.items.push(read_entry(&key, &name, source)),
            Err(code) => report.problems.push(problem(format!(
                "cannot open entry '{name}' (error {code})"
            ))),
        }
    }
    report
}

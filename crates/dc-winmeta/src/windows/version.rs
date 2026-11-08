//! Reading a program file's version resource (company, product, description, version).
//!
//! `GetFileVersionInfoW` maps the file as data to read its resource block: the program is never
//! started, and nothing is written.

use std::ffi::c_void;

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

use crate::model::VersionInfo;

/// Version blocks are a few KiB; anything bigger than this is ignored.
const MAX_BLOCK_BYTES: u32 = 1024 * 1024;
/// Longest string kept, in characters.
const MAX_TEXT: usize = 256;
/// Language/code-page pairs tried when the file lists none that work.
const FALLBACK_LANGUAGES: [&str; 3] = ["040904b0", "040904e4", "04090000"];

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Ask for `sub_block` inside the version data. Returns the pointer and length (in the unit the
/// block uses: characters for strings, bytes for binary data).
fn query(block: &[u8], sub_block: &str) -> Option<(*const c_void, usize)> {
    let sub_block = wide(sub_block);
    let mut data: *mut c_void = std::ptr::null_mut();
    let mut length = 0u32;
    // SAFETY: `block` holds a complete version-information block read by
    // `GetFileVersionInfoW`; `sub_block` is NUL-terminated and outlives the call; `data` and
    // `length` are writable. On success `data` points inside `block`.
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr().cast::<c_void>(),
            sub_block.as_ptr(),
            &mut data,
            &mut length,
        )
    };
    if ok == 0 || data.is_null() || length == 0 {
        return None;
    }
    Some((data.cast_const(), usize::try_from(length).ok()?))
}

/// A string value, copied out of the block and cleaned.
fn text(block: &[u8], language: &str, name: &str) -> Option<String> {
    let (pointer, length) = query(block, &format!("\\StringFileInfo\\{language}\\{name}"))?;
    let length = length.min(MAX_TEXT * 2);
    let base = pointer.cast::<u16>();
    let mut units = Vec::with_capacity(length);
    for index in 0..length {
        // SAFETY: `VerQueryValueW` reported `length` UTF-16 units starting at `base`, all
        // inside `block`. The data may be unaligned, so each unit is read unaligned.
        let unit = unsafe { base.add(index).read_unaligned() };
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let cleaned: String = String::from_utf16_lossy(&units)
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let cleaned = cleaned.trim().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// The language and code page pairs the file declares, formatted as eight hex digits.
fn languages(block: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    if let Some((pointer, bytes)) = query(block, "\\VarFileInfo\\Translation") {
        let base = pointer.cast::<u16>();
        for pair in 0..(bytes / 4).min(8) {
            // SAFETY: the translation value is `bytes` bytes long, an array of (language,
            // code page) u16 pairs inside `block`; reads are unaligned and within `bytes`.
            let (language, page) = unsafe {
                (
                    base.add(pair * 2).read_unaligned(),
                    base.add(pair * 2 + 1).read_unaligned(),
                )
            };
            found.push(format!("{language:04x}{page:04x}"));
        }
    }
    found.extend(FALLBACK_LANGUAGES.iter().map(|l| (*l).to_owned()));
    found
}

/// Read the version resource of `path`, or `None` if the file has none or cannot be read.
pub fn read_version_info(path: &str) -> Option<VersionInfo> {
    let wide_path = wide(path);
    let mut ignored = 0u32;
    // SAFETY: `wide_path` is NUL-terminated and outlives the call; `ignored` is writable.
    let size = unsafe { GetFileVersionInfoSizeW(wide_path.as_ptr(), &mut ignored) };
    if size == 0 || size > MAX_BLOCK_BYTES {
        return None;
    }
    let mut block = vec![0u8; usize::try_from(size).ok()?];
    // SAFETY: `block` is writable for exactly `size` bytes, as the call requires; the
    // handle parameter is ignored by Windows and passed as 0.
    let ok = unsafe {
        GetFileVersionInfoW(
            wide_path.as_ptr(),
            0,
            size,
            block.as_mut_ptr().cast::<c_void>(),
        )
    };
    if ok == 0 {
        return None;
    }

    for language in languages(&block) {
        let info = VersionInfo {
            company: text(&block, &language, "CompanyName"),
            product: text(&block, &language, "ProductName"),
            description: text(&block, &language, "FileDescription"),
            file_version: text(&block, &language, "FileVersion"),
        };
        if info != VersionInfo::default() {
            return Some(info);
        }
    }
    None
}

//! Authenticode signature check with `WinVerifyTrust`.
//!
//! Offline by design: no revocation lookups and cache-only URL retrieval, so nothing goes to the
//! network. The file is only read (to hash and verify it); it is never run.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Security::Cryptography::{
    CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE,
};
use windows_sys::Win32::Security::WinTrust::{
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

use crate::signature::{classify_trust_result, SignatureStatus};

/// Longest certificate display name kept, in UTF-16 units.
const NAME_BUFFER: usize = 256;

/// Check the signature of `path`.
pub fn verify_signature(path: &str) -> SignatureStatus {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    if wide.len() <= 1 || wide.len() > 32_768 {
        return SignatureStatus::Unknown {
            reason: "unusable path".into(),
        };
    }

    let mut file = WINTRUST_FILE_INFO {
        cbStruct: u32::try_from(std::mem::size_of::<WINTRUST_FILE_INFO>()).unwrap_or(0),
        pcwszFilePath: wide.as_ptr(),
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    // SAFETY: WINTRUST_DATA is a plain C struct whose all-zero value (null pointers, zero
    // numbers) is a valid starting state; the fields that matter are set right below.
    let mut data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    data.cbStruct = u32::try_from(std::mem::size_of::<WINTRUST_DATA>()).unwrap_or(0);
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = &mut file;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE;

    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `action` and `data` are valid, writable structures; `data` points to `file`,
    // which points to `wide`; all three outlive the call. A window handle of -1 means no UI.
    let result = unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE,
            &mut action,
            std::ptr::from_mut(&mut data).cast::<c_void>(),
        )
    };

    let signer = signer_name(&data);

    // The state must be released with a second call, whatever the first returned.
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: same structures as above; `hWVTStateData` was filled in by the first call and
    // this call frees it. `signer_name` copied everything it needed into an owned String.
    unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE,
            &mut action,
            std::ptr::from_mut(&mut data).cast::<c_void>(),
        );
    }

    classify_trust_result(result, signer)
}

/// The display name of the signing certificate, copied out of Windows' state data.
fn signer_name(data: &WINTRUST_DATA) -> Option<String> {
    if data.hWVTStateData.is_null() {
        return None;
    }
    // SAFETY: `hWVTStateData` is the live state handle from a `WTD_STATEACTION_VERIFY` call
    // that has not been closed yet. The helper returns null or a pointer into that state; the
    // signer and certificate pointers below are checked for null before use and are only read
    // while the state is open (the caller closes it afterwards).
    unsafe {
        let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return None;
        }
        let certificate = (*(*signer).pasCertChain).pCert;
        if certificate.is_null() {
            return None;
        }
        let mut buffer = [0u16; NAME_BUFFER];
        let length = CertGetNameStringW(
            certificate,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            std::ptr::null(),
            buffer.as_mut_ptr(),
            u32::try_from(NAME_BUFFER).unwrap_or(0),
        );
        // `length` counts the terminating NUL; 1 means an empty name.
        let used = usize::try_from(length)
            .unwrap_or(0)
            .saturating_sub(1)
            .min(NAME_BUFFER);
        let name = String::from_utf16_lossy(&buffer[..used]);
        let name = name.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

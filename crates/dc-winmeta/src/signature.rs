//! Signature results and how Windows trust codes map to them. This part is pure and portable;
//! the Windows call that produces the codes lives in `windows/signature.rs`.

use serde::{Deserialize, Serialize};

/// `TRUST_E_NOSIGNATURE`: the file has no embedded signature. It may still be signed through a
/// Windows security catalog (most Windows system files are), which this check does not look at.
pub const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100_u32 as i32;

/// The result of checking one program file. Offline only: no revocation lookups, no network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SignatureStatus {
    /// An embedded signature that Windows accepts. `signer` is the certificate's display name.
    Signed {
        /// Name of the signing certificate, when it could be read.
        signer: Option<String>,
    },
    /// No embedded signature. The file is unsigned *or* signed through a Windows catalog.
    NoEmbeddedSignature,
    /// A signature exists but Windows does not accept it (bad digest, untrusted root, expired
    /// certificate, ...). `code` is the Windows result as hex.
    Invalid {
        /// Windows trust result, for example `0x800B0109`.
        code: String,
        /// Name of the signing certificate, when it could be read.
        signer: Option<String>,
    },
    /// The check could not run (file unreadable, API failure).
    Unknown {
        /// Why.
        reason: String,
    },
}

/// Turn the result of `WinVerifyTrust` into a [`SignatureStatus`].
pub fn classify_trust_result(code: i32, signer: Option<String>) -> SignatureStatus {
    match code {
        0 => SignatureStatus::Signed { signer },
        TRUST_E_NOSIGNATURE => SignatureStatus::NoEmbeddedSignature,
        other => SignatureStatus::Invalid {
            code: format!("0x{:08X}", other as u32),
            signer,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_signed_and_keeps_the_signer() {
        assert_eq!(
            classify_trust_result(0, Some("Example Corp".into())),
            SignatureStatus::Signed {
                signer: Some("Example Corp".into())
            }
        );
    }

    #[test]
    fn no_signature_is_its_own_status() {
        assert_eq!(
            classify_trust_result(TRUST_E_NOSIGNATURE, None),
            SignatureStatus::NoEmbeddedSignature
        );
    }

    #[test]
    fn other_codes_are_invalid_with_a_hex_code() {
        let bad_digest = 0x8009_6010_u32 as i32;
        assert_eq!(
            classify_trust_result(bad_digest, None),
            SignatureStatus::Invalid {
                code: "0x80096010".into(),
                signer: None
            }
        );
    }

    #[test]
    fn statuses_round_trip_through_json() {
        let all = [
            SignatureStatus::Signed { signer: None },
            SignatureStatus::NoEmbeddedSignature,
            SignatureStatus::Invalid {
                code: "0x800B0109".into(),
                signer: Some("X".into()),
            },
            SignatureStatus::Unknown {
                reason: "unreadable".into(),
            },
        ];
        for status in all {
            let text = serde_json::to_string(&status).unwrap();
            assert_eq!(
                serde_json::from_str::<SignatureStatus>(&text).unwrap(),
                status
            );
        }
        assert_eq!(
            serde_json::to_string(&SignatureStatus::NoEmbeddedSignature).unwrap(),
            r#"{"status":"no_embedded_signature"}"#
        );
    }
}

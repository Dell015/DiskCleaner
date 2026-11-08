//! The result of a protection check. There is deliberately no "allowed" or "safe" value: the
//! only positive statement this crate can make is "protected".

use serde::{Deserialize, Serialize};

/// Why a path is protected. Ids are stable strings such as `windows_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    /// Stable machine-readable id.
    pub id: String,
    /// Human-readable explanation.
    pub description: String,
}

impl Reason {
    pub(crate) fn new(id: &str, description: &str) -> Reason {
        Reason {
            id: id.to_owned(),
            description: description.to_owned(),
        }
    }
}

/// The answer of [`crate::ProtectedList::check`].
///
/// `protected == false` means only "no protection rule matched". It does **not** mean the path
/// is safe to touch: unknown paths are `DontTouch` by default, and only the rules engine can
/// ever say otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    /// Whether at least one protection applies.
    pub protected: bool,
    /// Every protection that applies, without duplicates. Empty when `protected` is false.
    pub reasons: Vec<Reason>,
}

impl Verdict {
    /// Build a verdict from reasons, dropping duplicate ids and keeping the first occurrence.
    pub(crate) fn from_reasons(reasons: Vec<Reason>) -> Verdict {
        let mut unique: Vec<Reason> = Vec::with_capacity(reasons.len());
        for reason in reasons {
            if !unique.iter().any(|r| r.id == reason.id) {
                unique.push(reason);
            }
        }
        Verdict {
            protected: !unique.is_empty(),
            reasons: unique,
        }
    }

    /// The reason ids, in order.
    pub fn reason_ids(&self) -> Vec<&str> {
        self.reasons.iter().map(|r| r.id.as_str()).collect()
    }
}

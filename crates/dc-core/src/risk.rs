//! The [`Risk`] type: how careful we must be before offering an item for removal.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Deletion risk of an item.
///
/// The ordering is load-bearing: `Safe < ProbablySafe < DontTouch`.
/// **Do not reorder the variants.** `Ord` is derived from declaration order, and a test pins it.
///
/// The default is [`Risk::DontTouch`]: anything unclassified is untouchable (default-deny).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum Risk {
    /// Regenerates by itself or is pure junk. Still never deleted without the user's say-so.
    Safe,
    /// Probably fine to remove, but the user should look first.
    ProbablySafe,
    /// Required by the system, or possibly the user's own data. Never offered for removal.
    #[default]
    DontTouch,
}

impl Risk {
    /// Every level, from least to most cautious.
    pub const ALL: [Risk; 3] = [Risk::Safe, Risk::ProbablySafe, Risk::DontTouch];

    /// The more cautious of two risks (the maximum).
    ///
    /// This is the only way two verdicts are combined: the advisor merge in Phase B8 uses it,
    /// so a second opinion can raise caution but never lower it.
    #[must_use]
    pub fn escalate(self, other: Risk) -> Risk {
        self.max(other)
    }

    /// The most cautious of many risks. An empty input yields [`Risk::DontTouch`] (fail closed).
    #[must_use]
    pub fn escalate_all<I: IntoIterator<Item = Risk>>(risks: I) -> Risk {
        risks
            .into_iter()
            .reduce(Risk::escalate)
            .unwrap_or(Risk::DontTouch)
    }

    /// The stable string name of this level: `"Safe"`, `"ProbablySafe"` or `"DontTouch"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Risk::Safe => "Safe",
            Risk::ProbablySafe => "ProbablySafe",
            Risk::DontTouch => "DontTouch",
        }
    }
}

impl fmt::Display for Risk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Risk {
    type Err = CoreError;

    /// Exact, case-sensitive match. Anything else is an error, never a guess.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Safe" => Ok(Risk::Safe),
            "ProbablySafe" => Ok(Risk::ProbablySafe),
            "DontTouch" => Ok(Risk::DontTouch),
            other => Err(CoreError::InvalidRisk {
                value: other.to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_safe_then_probably_safe_then_dont_touch() {
        assert!(Risk::Safe < Risk::ProbablySafe);
        assert!(Risk::ProbablySafe < Risk::DontTouch);
        assert!(Risk::Safe < Risk::DontTouch);
    }

    #[test]
    fn all_is_sorted_and_complete() {
        let mut sorted = Risk::ALL;
        sorted.sort();
        assert_eq!(sorted, Risk::ALL);
        assert_eq!(Risk::ALL.len(), 3);
    }

    #[test]
    fn default_is_dont_touch() {
        assert_eq!(Risk::default(), Risk::DontTouch);
    }

    #[test]
    fn escalate_all_of_nothing_is_dont_touch() {
        assert_eq!(Risk::escalate_all(Vec::new()), Risk::DontTouch);
    }

    #[test]
    fn escalate_takes_the_more_cautious() {
        assert_eq!(Risk::Safe.escalate(Risk::ProbablySafe), Risk::ProbablySafe);
        assert_eq!(Risk::DontTouch.escalate(Risk::Safe), Risk::DontTouch);
    }

    #[test]
    fn display_and_from_str_round_trip() {
        for risk in Risk::ALL {
            assert_eq!(risk.to_string().parse::<Risk>(), Ok(risk));
        }
    }

    #[test]
    fn from_str_rejects_everything_else() {
        for bad in [
            "safe",
            "SAFE",
            "Unknown",
            "",
            " Safe",
            "Safe ",
            "probablysafe",
            "Dont Touch",
        ] {
            assert_eq!(
                bad.parse::<Risk>(),
                Err(CoreError::InvalidRisk {
                    value: bad.to_owned()
                }),
                "{bad:?} must be rejected"
            );
        }
    }
}

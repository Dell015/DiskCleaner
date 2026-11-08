//! The check: normalize, match, ask the OS, match again, merge. Either view can protect.

use crate::list::ProtectedList;
use crate::normalize::{normalize, NormalPath};
use crate::resolve::{PathResolver, Resolution};
use crate::verdict::{Reason, Verdict};

fn unresolved() -> Reason {
    Reason::new(
        "unresolved",
        "the real location of this path could not be determined",
    )
}

impl ProtectedList {
    /// Is `path` protected? Looks at the path as typed and at where the OS says it really is;
    /// protection from either view counts. Anything that cannot be understood is protected.
    ///
    /// A result of `protected == false` does **not** mean the path is safe to touch.
    pub fn check(&self, path: &str, resolver: &dyn PathResolver) -> Verdict {
        let mut reasons: Vec<Reason> = Vec::new();
        let typed: Option<NormalPath> = match normalize(path) {
            Ok(normal) => {
                reasons.extend(self.matches(&normal));
                Some(normal)
            }
            Err(invalid) => {
                reasons.push(invalid.reason());
                None
            }
        };
        let had_short_name = typed.as_ref().is_some_and(|n| n.has_short_name);

        match resolver.resolve(path) {
            Resolution::Resolved(real) => match normalize(&real) {
                Ok(real_normal) => {
                    reasons.extend(self.matches(&real_normal));
                    if had_short_name && real_normal.has_short_name {
                        reasons.push(unresolved());
                    }
                }
                Err(invalid) => reasons.push(invalid.reason()),
            },
            Resolution::NotFound => {
                if had_short_name {
                    reasons.push(unresolved());
                }
            }
            Resolution::Failed(_) => reasons.push(unresolved()),
        }
        Verdict::from_reasons(reasons)
    }
}

//! Rules engine: TOML rule packs that set a deterministic risk level and reason codes.
//!
//! **SAFETY-CRITICAL.** See SAFETY.md. No `unwrap`, `expect` or `panic!` in non-test code.
//!
//! Classification is advice, never an action: nothing here deletes, moves or even prepares
//! deletion. Unknown means `DontTouch`; risk only goes up when contributions are combined; the
//! protected list always wins.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod builtin;
mod candidates;
mod classify;
mod pack;
mod validate;

pub use candidates::{
    explain, find_candidates, verify, Blocked, Candidate, Candidates, Explanation, Sizes,
};
pub use classify::{Classification, Classifier, NodeFacts};
pub use pack::{parse as parse_pack, CompiledRule, Problem, Rule, RulePack, RuleScope};
pub use validate::{validate, MIN_LITERALS};

/// The text of the built-in rule pack.
pub fn builtin_pack_text() -> &'static str {
    builtin::PACK_TEXT
}

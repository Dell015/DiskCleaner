//! Protected-path engine: path normalization and the never-delete list.
//!
//! **SAFETY-CRITICAL.** See SAFETY.md. No `unwrap`, `expect` or `panic!` in non-test code.
//!
//! The only positive statement this crate makes is "protected". `protected == false` means no
//! rule matched; it never means a path is safe to delete.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod check;
mod list;
mod normalize;
mod pattern;
mod resolve;
mod verdict;

pub use list::{ProtectedList, Row, Scope, BUILTIN};
pub use normalize::{normalize, Invalid, NormalPath};
pub use resolve::{FakeResolver, FsResolver, PathResolver, Resolution};
pub use verdict::{Reason, Verdict};

//! Removing safely: an item is **moved** into a holding folder on the same drive (a rename: instant,
//! no extra space, the bytes are not touched), can be restored, and is deleted for good only when
//! its cleanup is emptied (after the retention time, or on request).
//!
//! **SAFETY-CRITICAL.** See SAFETY.md and `docs/phases/b9-cleaning/design.md`.
//! - Everything here is behind the Cargo feature `destructive-actions`, off by default.
//! - Every item is checked again on the disk right before it moves; any doubt skips it.
//! - Links and junctions are never followed, never moved, never entered.
//! - The only deletion is emptying a cleanup, and only inside a holding folder.
//! - Every step is written to an append-only journal *before* it happens; the journal is the only
//!   record, so a crash at any point leaves a state that can be read back exactly.
//! - No `unwrap`, `expect` or `panic!` outside tests.
//! - `unsafe` only in `win.rs` (Windows calls for checked moves and protected holding folders,
//!   D119), each block with a `// SAFETY:` note.

#![deny(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(feature = "destructive-actions")]
mod check;
#[cfg(feature = "destructive-actions")]
mod engine;
#[cfg(feature = "destructive-actions")]
mod journal;
#[cfg(feature = "destructive-actions")]
mod ops;
#[cfg(feature = "destructive-actions")]
mod places;
#[cfg(all(windows, feature = "destructive-actions"))]
#[allow(unsafe_code)]
mod win;

#[cfg(feature = "destructive-actions")]
pub use engine::{Cleanup, Engine, Group, HeldItem, Item, Outcome, Restored, Skipped, State};
#[cfg(feature = "destructive-actions")]
pub use journal::Record;
#[cfg(feature = "destructive-actions")]
pub use ops::{Ops, RealOps};
#[cfg(feature = "destructive-actions")]
pub use places::{Guard, Places, ProtectedGuard, WindowsPlaces};

//! Deterministic dummy file-tree generator for testing DiskCleaner.
//!
//! The generator is create-only: it never deletes, renames or overwrites anything, and it
//! refuses to write anywhere but a new or empty directory that is not a root or a link.

#![forbid(unsafe_code)]

mod error;
mod guard;
mod layout;
mod manifest;
mod platform;
mod rng;
mod writer;

use std::path::PathBuf;

pub use error::GenError;
pub use guard::{validate_out, ValidatedOut};
pub use manifest::{Manifest, MANIFEST_SCHEMA_VERSION};
pub use rng::{fnv1a64, Xoshiro256StarStar};

/// Default seed.
pub const DEFAULT_SEED: u64 = 0x4443_4230_0000_0001;
/// Default number of files in the huge-file-count directory.
pub const DEFAULT_FILE_COUNT: u32 = 10_000;

/// Generation options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Output directory (new or empty).
    pub out: PathBuf,
    /// RNG seed.
    pub seed: u64,
    /// Files in the huge-file-count directory.
    pub file_count: u32,
}

impl Options {
    /// Options with default seed and file count.
    #[must_use]
    pub fn new(out: impl Into<PathBuf>) -> Self {
        Self {
            out: out.into(),
            seed: DEFAULT_SEED,
            file_count: DEFAULT_FILE_COUNT,
        }
    }
}

/// Generate the tree and write `manifest.json` next to it.
pub fn generate(options: &Options) -> Result<Manifest, GenError> {
    let out = guard::validate_out(&options.out)?;
    let tree = guard::create_tree(&out)?;
    let mut ctx = writer::Ctx::new(tree, options.seed);
    layout::build(&mut ctx, options.file_count)?;
    let manifest = manifest::build(options, ctx.into_parts());
    manifest::write(&out.path().join("manifest.json"), &manifest)?;
    Ok(manifest)
}

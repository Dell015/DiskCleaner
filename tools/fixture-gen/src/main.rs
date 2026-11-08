//! Command-line front end for the fixture generator.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use fixture_gen::{generate, Options, DEFAULT_FILE_COUNT, DEFAULT_SEED};

/// Generate a deterministic dummy file tree for testing. Never deletes anything.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Output directory. Must be new or empty; never a drive root or a link.
    #[arg(long)]
    out: PathBuf,
    /// Seed for the deterministic generator.
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,
    /// Number of files in the huge-file-count directory.
    #[arg(long, default_value_t = DEFAULT_FILE_COUNT)]
    file_count: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let manifest = generate(&Options {
        out: args.out.clone(),
        seed: args.seed,
        file_count: args.file_count,
    })?;
    println!(
        "generated {} files in {} dirs ({} bytes) under {}",
        manifest.totals.file_paths,
        manifest.totals.dirs,
        manifest.totals.logical_bytes_all_paths,
        args.out.display()
    );
    Ok(())
}

//! The `dc` command-line tool.
//!
//! Phase B1 commands: `dc scan` (read-only scan of a folder to a JSON file) and `dc top`
//! (reports from a saved scan). Nothing here deletes, moves or modifies scanned files.
//!
//! Exit codes: 0 = ok, 1 = error, 2 = the scan finished but was partial (unreadable entries
//! or cancelled).

#![forbid(unsafe_code)]

mod scan_cmd;
mod size;
mod top_cmd;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{ArgGroup, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "dc",
    version,
    about = "DiskCleaner: see what uses your disk space (read-only)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scan a folder (read-only) and save the result as JSON.
    Scan {
        /// Folder to scan.
        path: PathBuf,
        /// Where to write the JSON result. Must not exist yet (it is never overwritten). It is written after the scan finishes.
        #[arg(long)]
        out: PathBuf,
        /// Only store entries at most this deep (totals still include everything).
        #[arg(long)]
        max_depth: Option<u32>,
        /// Only store files at least this large, e.g. 10MB (totals still include everything).
        #[arg(long, value_parser = size::parse_size)]
        min_size: Option<u64>,
        /// Allow scanning a whole drive (e.g. C:\).
        #[arg(long)]
        i_know: bool,
    },
    /// Show a report from a saved scan.
    #[command(group(ArgGroup::new("report").required(true).args(["files", "dirs", "ext", "category", "oldest"])))]
    Top {
        /// A JSON file written by `dc scan`.
        file: PathBuf,
        /// Largest files.
        #[arg(long)]
        files: bool,
        /// Largest folders.
        #[arg(long)]
        dirs: bool,
        /// Totals per file extension.
        #[arg(long)]
        ext: bool,
        /// Totals per category (video, image, ...).
        #[arg(long)]
        category: bool,
        /// Oldest files by modification time.
        #[arg(long)]
        oldest: bool,
        /// How many rows to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn run() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::Scan {
            path,
            out,
            max_depth,
            min_size,
            i_know,
        } => scan_cmd::run(scan_cmd::ScanArgs {
            path,
            out,
            max_depth,
            min_size,
            i_know,
        }),
        Command::Top {
            file,
            files,
            dirs,
            ext,
            category,
            oldest,
            limit,
        } => {
            let report = if files {
                top_cmd::Report::Files
            } else if dirs {
                top_cmd::Report::Dirs
            } else if ext {
                top_cmd::Report::Ext
            } else if category {
                top_cmd::Report::Category
            } else {
                debug_assert!(oldest);
                top_cmd::Report::Oldest
            };
            top_cmd::run(&file, &report, limit)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

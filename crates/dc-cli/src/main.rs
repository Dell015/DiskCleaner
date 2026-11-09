//! The `dc` command-line tool.
//!
//! Commands: `dc scan` (read-only scan of a folder to a JSON file), `dc top` (reports from a
//! saved scan), `dc attrib` (who owns which folders), `dc protected` (is this path protected?) and `dc apps` (installed programs, Windows only). Nothing here deletes, moves or modifies scanned files.
//!
//! Exit codes: 0 = ok, 1 = error, 2 = the scan finished but was partial (unreadable entries
//! or cancelled).

#![forbid(unsafe_code)]

mod apps_cmd;
mod attrib_cmd;
mod classify_cmd;
mod leftovers_cmd;
mod protected_cmd;
mod rules_cmd;
mod scan_cmd;
mod size;
mod timing;
mod top_cmd;
mod view_cmd;

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
        /// Label folders that are installed apps. Give a snapshot from `dc apps --out`, or
        /// leave the value out to read the Windows registry (Windows only).
        #[arg(long, num_args = 0..=1, value_name = "SNAPSHOT")]
        apps: Option<Option<PathBuf>>,
    },
    /// List installed programs (reads the Windows registry; read-only).
    Apps {
        /// Also save the list as a JSON snapshot. Must not exist yet (it is never overwritten).
        #[arg(long, conflicts_with = "from")]
        out: Option<PathBuf>,
        /// Show a saved snapshot instead of reading the registry (works on any system).
        #[arg(long)]
        from: Option<PathBuf>,
        /// Also show updates, patches and entries without a name.
        #[arg(long)]
        include_hidden: bool,
        /// Check the digital signature of each app's main program (offline, slower).
        #[arg(long, conflicts_with = "from")]
        verify_signatures: bool,
    },
    /// Work out which app owns which folders in a saved scan (read-only).
    Attrib {
        /// A JSON file written by `dc scan`.
        scan: PathBuf,
        /// A snapshot written by `dc apps --out`.
        #[arg(long)]
        apps: PathBuf,
        /// Also save the full result as JSON. Must not exist yet (it is never overwritten).
        #[arg(long)]
        out: Option<PathBuf>,
        /// How many rows to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Explain the ownership of one path that is in the scan.
        #[arg(long, value_name = "PATH")]
        explain: Option<String>,
    },
    /// Suggest what could be cleaned up in a saved scan, by rule (read-only; deletes nothing).
    Classify {
        /// A JSON file written by `dc scan`.
        scan: PathBuf,
        /// A snapshot written by `dc apps --out`; adds the owner of each folder to the list.
        #[arg(long)]
        apps: Option<PathBuf>,
        /// Which candidates to show.
        #[arg(long, value_enum, default_value = "all")]
        risk: classify_cmd::RiskFilter,
        /// How many rows to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Explain the classification of one path that is in the scan.
        #[arg(long, value_name = "PATH")]
        explain: Option<String>,
        /// Print how long each step took (to standard error).
        #[arg(long)]
        timing: bool,
    },
    /// Find folders that apps left behind after being removed, with the evidence (read-only).
    Leftovers {
        /// One or more JSON files written by `dc scan`. To cover Program Files and ProgramData as
        /// well as your profile, scan each folder and give all the files here.
        #[arg(required = true, num_args = 1..)]
        scans: Vec<PathBuf>,
        /// A snapshot written by `dc apps --out` (needed: it says which apps are installed).
        #[arg(long)]
        apps: PathBuf,
        /// How many findings to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Explain what the search says about one path that is in the scan.
        #[arg(long, value_name = "PATH")]
        explain: Option<String>,
        /// Save the full result as JSON (never overwrites an existing file).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Print how long each step took (to standard error).
        #[arg(long)]
        timing: bool,
    },
    /// Write one compact file for the graph view (read-only; never overwrites a file).
    ViewData {
        /// One or more JSON files written by `dc scan`; they are joined under one root.
        #[arg(required = true, num_args = 1..)]
        scans: Vec<PathBuf>,
        /// A snapshot written by `dc apps --out`: adds owner apps and leftovers.
        #[arg(long)]
        apps: Option<PathBuf>,
        /// Where to save the view file.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Print how long each step took (to standard error).
        #[arg(long)]
        timing: bool,
    },
    /// Show or check cleanup rule packs (read-only).
    #[command(group(ArgGroup::new("what").required(true).args(["list", "check"])))]
    Rules {
        /// Print the built-in rule pack.
        #[arg(long)]
        list: bool,
        /// Check a rule pack file and report every problem.
        #[arg(long, value_name = "FILE")]
        check: Option<PathBuf>,
    },
    /// Check whether paths are protected (read-only). "Not protected" does not mean safe.
    #[command(group(ArgGroup::new("what").required(true).args(["paths", "list"])))]
    Protected {
        /// Paths to check.
        paths: Vec<String>,
        /// Print the built-in protected list instead.
        #[arg(long)]
        list: bool,
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
            apps,
        } => scan_cmd::run(scan_cmd::ScanArgs {
            path,
            out,
            max_depth,
            min_size,
            i_know,
            apps,
        }),
        Command::Apps {
            out,
            from,
            include_hidden,
            verify_signatures,
        } => apps_cmd::run(apps_cmd::AppsArgs {
            out,
            from,
            include_hidden,
            verify_signatures,
        }),
        Command::Attrib {
            scan,
            apps,
            out,
            limit,
            explain,
        } => attrib_cmd::run(&attrib_cmd::AttribArgs {
            scan,
            apps,
            out,
            limit,
            explain,
        }),
        Command::Classify {
            scan,
            apps,
            risk,
            limit,
            explain,
            timing,
        } => classify_cmd::run(&classify_cmd::ClassifyArgs {
            scan,
            apps,
            risk,
            limit,
            explain,
            timing,
        }),
        Command::Leftovers {
            scans,
            apps,
            limit,
            explain,
            out,
            timing,
        } => leftovers_cmd::run(&leftovers_cmd::LeftoversArgs {
            scans,
            apps,
            limit,
            explain,
            out,
            timing,
        }),
        Command::ViewData {
            scans,
            apps,
            out,
            timing,
        } => view_cmd::run(&view_cmd::ViewArgs {
            scans,
            apps,
            out,
            timing,
        }),
        Command::Rules { list, check } => rules_cmd::run(list, check.as_deref()),
        Command::Protected { paths, list } => protected_cmd::run(&paths, list),
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

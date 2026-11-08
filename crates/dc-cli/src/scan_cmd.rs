//! `dc scan`.

use std::fs::OpenOptions;
use std::io::{BufWriter, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use dc_core::ScanStatus;
use dc_scan::{scan, ScanOptions};

use crate::size::format_bytes;

pub struct ScanArgs {
    pub path: PathBuf,
    pub out: PathBuf,
    pub max_depth: Option<u32>,
    pub min_size: Option<u64>,
    pub i_know: bool,
}

#[cfg(windows)]
fn probe() -> Box<dyn dc_scan::FileProbe> {
    Box::new(dc_winmeta::WindowsProbe)
}

#[cfg(not(windows))]
fn probe() -> Box<dyn dc_scan::FileProbe> {
    dc_scan::default_probe()
}

pub fn run(args: ScanArgs) -> Result<ExitCode> {
    let show_progress = std::io::stderr().is_terminal();
    let options = ScanOptions {
        max_depth: args.max_depth,
        min_size: args.min_size,
        allow_drive_root: args.i_know,
        progress: show_progress.then(|| {
            Arc::new(|p: &dc_scan::Progress| {
                eprint!(
                    "\rscanned {} files, {} dirs, {}    ",
                    p.files,
                    p.dirs,
                    format_bytes(p.bytes)
                );
            }) as dc_scan::ProgressFn
        }),
        ..ScanOptions::default()
    };

    let result = scan(&args.path, probe().as_ref(), &options)?;
    if show_progress {
        eprintln!();
    }

    // `create_new`: never overwrite an existing file.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .with_context(|| {
            format!(
                "cannot create {} (it must not already exist)",
                args.out.display()
            )
        })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &result).context("writing scan JSON")?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    let t = result.totals;
    println!("scanned   {}", args.path.display());
    println!("files     {}", t.files);
    println!("folders   {}", t.dirs);
    println!("links     {} (not followed)", t.links);
    println!(
        "size      {} ({} bytes)",
        format_bytes(t.logical_bytes),
        t.logical_bytes
    );
    println!("on disk   {}", format_bytes(t.size_on_disk_bytes));
    if t.hardlink_extra_paths > 0 {
        println!(
            "hardlinks {} extra paths, counted once",
            t.hardlink_extra_paths
        );
    }
    println!(
        "stored    {} entries -> {}",
        result.nodes.len(),
        args.out.display()
    );
    match result.status {
        ScanStatus::Complete => Ok(ExitCode::SUCCESS),
        ScanStatus::Partial => {
            println!(
                "WARNING   {} entries could not be read (see the errors list in the file)",
                t.errors
            );
            Ok(ExitCode::from(2))
        }
        ScanStatus::Cancelled => {
            println!("WARNING   scan was cancelled; results are partial");
            Ok(ExitCode::from(2))
        }
    }
}

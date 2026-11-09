//! `dc view-data`: one compact file for the graph view (read-only).
//!
//! The format and the checks live in `dc-api`, shared with the desktop app. This command reads
//! scan files, runs the checks, and writes the file. It never overwrites an existing file.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use dc_api::analysis::analyze;
use dc_api::view::ViewBuilder;
use dc_core::ScanResult;
use dc_rules::Classifier;

use crate::apps_cmd;
use crate::size::format_bytes;
use crate::timing::Stopwatch;

pub struct ViewArgs {
    pub scans: Vec<PathBuf>,
    pub apps: Option<PathBuf>,
    pub out: PathBuf,
    pub timing: bool,
}

pub fn run(args: &ViewArgs) -> Result<ExitCode> {
    let mut watch = Stopwatch::new(args.timing);
    if args.out.exists() {
        bail!(
            "{} already exists; choose a new file name",
            args.out.display()
        );
    }
    let snapshot = match &args.apps {
        Some(p) => Some(apps_cmd::load(p)?),
        None => None,
    };
    let classifier = Classifier::builtin_with(crate::protected_cmd::build_list());
    watch.lap("read apps, protected");

    let mut builder = ViewBuilder::new();
    for path in &args.scans {
        let reader = BufReader::new(
            File::open(path).with_context(|| format!("cannot open {}", path.display()))?,
        );
        let scan: ScanResult = serde_json::from_reader(reader)
            .with_context(|| format!("{} is not a valid scan file", path.display()))?;
        watch.lap("read scan file");
        let tree = scan.tree().context("the scan file is damaged")?;
        let analysis = analyze(&scan, &tree, snapshot.as_ref(), &classifier)
            .context("the scan file is damaged")?;
        watch.lap("checks");
        builder
            .add(&scan, &tree, &analysis)
            .context("too many entries for one view file")?;
        watch.lap("columns");
    }
    let view = builder.finish();

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .with_context(|| format!("cannot create {}", args.out.display()))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &view)?;
    writer.flush()?;
    watch.lap("write");
    let total: u64 = view.nodes.size.iter().sum();
    println!("entries   {}", view.nodes.name.len());
    println!("size      {}", format_bytes(total));
    println!("marks     {}", view.marks.len());
    println!("owners    {}", view.owners.len());
    println!("saved     {}", args.out.display());
    println!();
    println!(
        "Open the graph view and load this file. It stays on this computer. Nothing was changed."
    );
    Ok(ExitCode::SUCCESS)
}

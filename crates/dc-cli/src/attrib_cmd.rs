//! `dc attrib`: which app owns which folders in a saved scan (read-only).

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use dc_attrib::{Confidence, OwnerKind};
use dc_core::ScanResult;

use crate::apps_cmd;
use crate::size::format_bytes;

pub struct AttribArgs {
    pub scan: PathBuf,
    pub apps: PathBuf,
    pub out: Option<PathBuf>,
    pub limit: usize,
    pub explain: Option<String>,
}

const REMINDER: &str =
    "Ownership is information, not permission: nothing here says a folder is safe to delete.";

fn kind_name(kind: OwnerKind) -> &'static str {
    match kind {
        OwnerKind::App => "app",
        OwnerKind::Vendor => "vendor",
        OwnerKind::Windows => "windows",
    }
}

fn confidence_name(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
        Confidence::Ambiguous => "ambiguous",
    }
}

pub fn run(args: &AttribArgs) -> Result<ExitCode> {
    let reader = BufReader::new(
        File::open(&args.scan).with_context(|| format!("cannot open {}", args.scan.display()))?,
    );
    let scan: ScanResult = serde_json::from_reader(reader).context("not a valid scan file")?;
    let snapshot = apps_cmd::load(&args.apps)?;
    #[cfg(windows)]
    let result = dc_attrib::attribute_with(&scan, &snapshot, &dc_winmeta::WindowsSource);
    #[cfg(not(windows))]
    let result = dc_attrib::attribute(&scan, &snapshot);
    let result = result.context("the scan file is damaged")?;

    if let Some(path) = &args.explain {
        return explain(&scan, &result, path);
    }

    println!(
        "scan      {}   ({} on disk)",
        result.scan_root,
        format_bytes(result.scan_total_on_disk)
    );
    println!();
    println!(
        "{:>12} {:>12} {:>8}  owner",
        "exclusive", "footprint", "kind"
    );
    let mut rows: Vec<_> = result.totals.iter().collect();
    rows.sort_by(|a, b| {
        b.own
            .on_disk
            .cmp(&a.own.on_disk)
            .then(a.owner.cmp(&b.owner))
    });
    for total in rows.into_iter().take(args.limit) {
        if let Some(owner) = result.owners.iter().find(|o| o.id == total.owner) {
            println!(
                "{:>12} {:>12} {:>8}  {}",
                format_bytes(total.own.on_disk),
                format_bytes(total.footprint_on_disk),
                kind_name(owner.kind),
                owner.name
            );
        }
    }
    println!();
    println!(
        "{:>12}  shared (claimed about equally by several apps)",
        format_bytes(result.shared.on_disk)
    );
    println!(
        "{:>12}  no owner found",
        format_bytes(result.unattributed.on_disk)
    );
    println!(
        "{:>12}  programs identified inside app folders (version info, read-only)",
        result.programs.len()
    );
    println!();
    println!("Biggest folders with no owner:");
    for dir in result.top_unattributed.iter().take(args.limit) {
        println!("{:>12}  {}", format_bytes(dir.on_disk), dir.path);
    }
    println!();
    println!("Exclusive sizes add up to the scan total; footprint includes owners nested inside (a vendor folder includes its apps).");
    println!("{REMINDER}");

    if let Some(out) = &args.out {
        // `create_new`: never overwrite an existing file.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out)
            .with_context(|| {
                format!(
                    "cannot create {} (it must not already exist)",
                    out.display()
                )
            })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &result).context("writing JSON")?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        println!("saved     {}", out.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn explain(scan: &ScanResult, result: &dc_attrib::AttribResult, path: &str) -> Result<ExitCode> {
    let Some(found) = dc_attrib::explain(scan, result, path).context("the scan file is damaged")?
    else {
        println!("{path} is not in this scan (looked up in the scan file, not on disk)");
        return Ok(ExitCode::SUCCESS);
    };
    println!("path      {}", found.path);
    if dc_attrib::protected_marker(&found.path) {
        println!("protected yes (built-in protected list; see `dc protected`)");
    }
    match &found.assignment {
        None => println!("owner     none found"),
        Some(assignment) => {
            let name_of = |id: u32| {
                result
                    .owners
                    .iter()
                    .find(|o| o.id == id)
                    .map_or("?", |o| o.name.as_str())
            };
            match assignment.owner {
                Some(id) => println!("owner     {}", name_of(id)),
                None => println!("owner     unclear (several candidates, none is claimed)"),
            }
            println!("how sure  {}", confidence_name(assignment.confidence));
            if let (true, Some(from)) = (found.inherited, &found.assigned_path) {
                println!("inherited from {from}");
            }
            for evidence in &assignment.evidence {
                println!("evidence  {}", evidence.detail);
            }
            if !assignment.alternatives.is_empty() {
                let names: Vec<&str> = assignment
                    .alternatives
                    .iter()
                    .map(|id| name_of(*id))
                    .collect();
                println!("also      {}", names.join(", "));
            }
        }
    }
    println!("{REMINDER}");
    Ok(ExitCode::SUCCESS)
}

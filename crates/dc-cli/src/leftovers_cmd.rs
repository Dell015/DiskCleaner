//! `dc leftovers`: folders that apps left behind, with the evidence (read-only).

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use dc_core::ScanResult;
use dc_leftovers::{Finding, Standing};
use dc_protected::FsResolver;
use dc_rules::Classifier;

use crate::apps_cmd;
use crate::size::format_bytes;
use crate::timing::Stopwatch;

pub struct LeftoversArgs {
    pub scans: Vec<PathBuf>,
    pub apps: PathBuf,
    pub limit: usize,
    pub explain: Option<String>,
    pub out: Option<PathBuf>,
    pub timing: bool,
}

const REMINDER: &str = "These are suggestions with evidence. Nothing has been or will be deleted by this command, and the registry is never changed.";

pub(crate) fn kind_text(kind: dc_leftovers::Kind) -> &'static str {
    match kind {
        dc_leftovers::Kind::MissingApp => "app's program folder is gone",
        dc_leftovers::Kind::BackupCopy => "old backup copy",
        dc_leftovers::Kind::KnownAppRemoved => "known app, not installed",
        dc_leftovers::Kind::UnregisteredProgram => "programs nobody registered",
    }
}

pub fn run(args: &LeftoversArgs) -> Result<ExitCode> {
    let mut watch = Stopwatch::new(args.timing);
    // Refuse early if the output file exists, before any work is done.
    if let Some(out) = &args.out {
        if out.exists() {
            bail!("{} already exists; choose a new file name", out.display());
        }
    }
    let snapshot = apps_cmd::load(&args.apps)?;
    let classifier = Classifier::builtin_with(crate::protected_cmd::build_list());
    watch.lap("read apps, protected");
    let mut parts: Vec<dc_leftovers::Leftovers> = Vec::new();
    for path in &args.scans {
        let reader = BufReader::new(
            File::open(path).with_context(|| format!("cannot open {}", path.display()))?,
        );
        let scan: ScanResult = serde_json::from_reader(reader)
            .with_context(|| format!("{} is not a valid scan file", path.display()))?;
        watch.lap("read scan file");
        let attrib = dc_attrib::attribute(&scan, &snapshot).context("the scan file is damaged")?;
        watch.lap("attribution");
        if let Some(wanted) = &args.explain {
            let e =
                dc_leftovers::explain(&scan, &snapshot, &attrib, &classifier, &FsResolver, wanted)
                    .context("the scan file is damaged")?;
            if e.standing != Standing::NotInScan || path == args.scans.last().unwrap_or(path) {
                return explain(&e, wanted);
            }
            continue;
        }
        let found =
            dc_leftovers::find_leftovers(&scan, &snapshot, &attrib, &classifier, &FsResolver)
                .context("the scan file is damaged")?;
        watch.lap("find leftovers");
        // Second look at the real file system: links above a finding, failures, protected targets.
        parts.push(dc_leftovers::verify(found, &classifier, &FsResolver));
        watch.lap("check against disk");
    }
    let found = dc_leftovers::merge(parts);

    println!("scan        {}", found.scan_root);
    println!(
        "offered     {:>12}  in {} findings (look first)",
        format_bytes(found.offered.on_disk),
        found
            .findings
            .iter()
            .filter(|f| f.candidate.risk != dc_core::Risk::DontTouch)
            .count()
    );
    println!(
        "shown only  {:>12}  in {} findings (weak evidence, may hold your data, or an app that can hold games)",
        format_bytes(found.reported_only.on_disk),
        found
            .findings
            .iter()
            .filter(|f| f.candidate.risk == dc_core::Risk::DontTouch)
            .count()
    );
    println!(
        "refused     {:>12}  {} folders (protected, Windows/Microsoft, or links inside)",
        "",
        found.refused.len()
    );
    println!(
        "looked at   {:>12}  {} folders directly inside AppData, ProgramData, Program Files, Common Files",
        "", found.examined
    );

    // Group by the app each finding probably belonged to, biggest group first.
    let mut groups: BTreeMap<String, Vec<&Finding>> = BTreeMap::new();
    for finding in found.findings.iter().take(args.limit) {
        let who = finding
            .app
            .clone()
            .unwrap_or_else(|| "(unknown app)".to_owned());
        groups.entry(who).or_default().push(finding);
    }
    let mut ordered: Vec<(String, Vec<&Finding>, u64)> = groups
        .into_iter()
        .map(|(name, list)| {
            let total = list.iter().map(|f| f.candidate.sizes.on_disk).sum();
            (name, list, total)
        })
        .collect();
    ordered.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    for (name, list, total) in &ordered {
        println!();
        println!(
            "{name}  ({} in {} folders)",
            format_bytes(*total),
            list.len()
        );
        for f in list {
            println!(
                "  {:>10}  {:<7} {:<13} {}",
                format_bytes(f.candidate.sizes.on_disk),
                format!("{:?}", f.confidence),
                f.candidate.risk.as_str(),
                f.candidate.path
            );
            println!("              {}", kind_text(f.kind));
            for e in &f.evidence {
                println!("              + {:>2}  {}", e.points, e.detail);
            }
            if f.candidate.risk == dc_core::Risk::DontTouch {
                println!("              not offered: {}", f.candidate.summary);
            }
        }
    }
    if !found.stale_apps.is_empty() {
        println!();
        println!("Apps that are registered but whose program folder is gone (information only):");
        for a in found.stale_apps.iter().take(args.limit) {
            println!("  {}  ->  {}", a.name, a.registered_folder);
        }
    }
    for r in found.refused.iter().take(args.limit) {
        println!(
            "refused   {}  [{}]  because {}",
            r.path,
            r.kind.id(),
            r.because
        );
    }
    watch.lap("print");

    if let Some(out) = &args.out {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out)
            .with_context(|| format!("cannot create {}", out.display()))?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &found)?;
        writer.flush()?;
        println!("saved     {}", out.display());
    }
    println!();
    println!("{REMINDER}");
    Ok(ExitCode::SUCCESS)
}

fn explain(e: &dc_leftovers::Explanation, path: &str) -> Result<ExitCode> {
    let state = match e.standing {
        Standing::NotInScan => "not in this scan (looked up in the scan file, not on disk)",
        Standing::NotExamined => "in the scan, but not looked at",
        Standing::NoEvidence => "looked at: no evidence it is a leftover",
        Standing::Finding => "a finding",
        Standing::Refused => "refused",
    };
    println!(
        "path      {}",
        if e.path.is_empty() { path } else { &e.path }
    );
    println!("state     {state}");
    for note in &e.notes {
        println!("          {note}");
    }
    println!("{REMINDER}");
    Ok(ExitCode::SUCCESS)
}

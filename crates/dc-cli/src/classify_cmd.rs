//! `dc classify`: what could be cleaned up, according to the rules (read-only).

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::ValueEnum;
use dc_core::{Risk, ScanResult};
use dc_protected::{FakeResolver, FsResolver};
use dc_rules::Classifier;

use crate::apps_cmd;
use crate::size::format_bytes;
use crate::timing::Stopwatch;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RiskFilter {
    /// Show everything offered.
    All,
    /// Only `Safe` candidates.
    Safe,
    /// Only `ProbablySafe` candidates.
    Probable,
}

pub struct ClassifyArgs {
    pub scan: PathBuf,
    pub apps: Option<PathBuf>,
    pub risk: RiskFilter,
    pub limit: usize,
    pub explain: Option<String>,
    pub timing: bool,
}

const REMINDER: &str =
    "These are suggestions from rules. Nothing has been or will be deleted by this command.";

pub fn run(args: &ClassifyArgs) -> Result<ExitCode> {
    let mut watch = Stopwatch::new(args.timing);
    let reader = BufReader::new(
        File::open(&args.scan).with_context(|| format!("cannot open {}", args.scan.display()))?,
    );
    let scan: ScanResult = serde_json::from_reader(reader).context("not a valid scan file")?;
    watch.lap("read scan file");
    let classifier = Classifier::builtin_with(crate::protected_cmd::build_list());
    watch.lap("build protected list");
    // Ages are measured as of the end of the scan, the moment the data describes.
    let now = scan.finished_unix;

    if let Some(path) = &args.explain {
        return explain(&scan, &classifier, now, path);
    }

    let found = dc_rules::find_candidates(&scan, &classifier, &FakeResolver::new(), now)
        .context("the scan file is damaged")?;
    watch.lap("find candidates");
    if args.timing {
        let w = found.work;
        eprintln!(
            "timing      classify one by one  {} items, {:.2}s",
            w.classified,
            w.classify_micros as f64 / 1e6
        );
        eprintln!(
            "timing      look inside folders  {} items, {:.2}s",
            w.inspected,
            w.inspect_micros as f64 / 1e6
        );
    }
    // Second look at the real file system: links above a candidate, failures, protected targets.
    let found = dc_rules::verify(found, &classifier, &FsResolver);

    watch.lap("check against disk");
    let owners = match &args.apps {
        Some(file) => {
            let snapshot = apps_cmd::load(file)?;
            let attrib =
                dc_attrib::attribute(&scan, &snapshot).context("the scan file is damaged")?;
            Some(attrib)
        }
        None => None,
    };

    watch.lap("attribution (apps)");
    println!("scan      {}", scan.root);
    println!(
        "safe      {:>12}  in {} items (regenerate by themselves)",
        format_bytes(found.safe.on_disk),
        found.items.iter().filter(|i| i.risk == Risk::Safe).count()
    );
    println!(
        "probably  {:>12}  in {} items (look first)",
        format_bytes(found.probably_safe.on_disk),
        found
            .items
            .iter()
            .filter(|i| i.risk == Risk::ProbablySafe)
            .count()
    );
    if !found.blocked.is_empty() {
        println!(
            "refused   {:>12}  {} matches not offered because of what they contain or where they really are",
            "", found.blocked.len()
        );
    }
    println!();
    println!("{:>12}  {:<13} {:<22} owner / path", "size", "risk", "rule");
    let shown = found.items.iter().filter(|i| match args.risk {
        RiskFilter::All => true,
        RiskFilter::Safe => i.risk == Risk::Safe,
        RiskFilter::Probable => i.risk == Risk::ProbablySafe,
    });
    let shown: Vec<&dc_rules::Candidate> = shown.take(args.limit).collect();
    // Index the scan once for all the owner lookups (doing it per item is very slow).
    let paths: Vec<&str> = shown.iter().map(|i| i.path.as_str()).collect();
    let explained = match &owners {
        Some(a) => dc_attrib::explain_many(&scan, a, &paths).unwrap_or_default(),
        None => Vec::new(),
    };
    for (n, item) in shown.into_iter().enumerate() {
        let owner = explained
            .get(n)
            .cloned()
            .flatten()
            .and_then(|e| e.assignment)
            .and_then(|a| a.owner)
            .and_then(|id| {
                owners
                    .as_ref()
                    .and_then(|a| a.owners.iter().find(|o| o.id == id))
                    .map(|o| o.name.clone())
            });
        let partial = if item.partial {
            " (only some files)"
        } else {
            ""
        };
        println!(
            "{:>12}  {:<13} {:<22} {}{partial}",
            format_bytes(item.sizes.on_disk),
            item.risk.as_str(),
            item.rule,
            item.path
        );
        if let Some(owner) = owner {
            println!("{:>12}  {:<13} {:<22} owner: {owner}", "", "", "");
        }
    }
    watch.lap("owner lookup + print");
    for blocked in found.blocked.iter().take(args.limit) {
        println!(
            "refused   {}  [{}]  because of {}",
            blocked.path, blocked.rule, blocked.because
        );
    }
    println!();
    println!("{REMINDER}");
    Ok(ExitCode::SUCCESS)
}

fn explain(scan: &ScanResult, classifier: &Classifier, now: i64, path: &str) -> Result<ExitCode> {
    let Some(found) = dc_rules::explain(scan, classifier, &FsResolver, now, path)
        .context("the scan file is damaged")?
    else {
        println!("{path} is not in this scan (looked up in the scan file, not on disk)");
        return Ok(ExitCode::SUCCESS);
    };
    println!("path      {}", found.path);
    println!("kind      {}", if found.is_dir { "folder" } else { "file" });
    println!("risk      {}", found.classification.risk.as_str());
    for reason in &found.classification.reasons {
        println!("reason    {}: {}", reason.id, reason.description);
    }
    for rule in &found.classification.rules {
        println!("rule      {rule}");
    }
    if let Some(blocker) = &found.blocker {
        println!("blocked   a rule matches the folder, but it is not offered: {blocker}");
    }
    println!("{REMINDER}");
    Ok(ExitCode::SUCCESS)
}

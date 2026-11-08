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
}

const REMINDER: &str =
    "These are suggestions from rules. Nothing has been or will be deleted by this command.";

pub fn run(args: &ClassifyArgs) -> Result<ExitCode> {
    let reader = BufReader::new(
        File::open(&args.scan).with_context(|| format!("cannot open {}", args.scan.display()))?,
    );
    let scan: ScanResult = serde_json::from_reader(reader).context("not a valid scan file")?;
    let classifier = Classifier::builtin_with(crate::protected_cmd::build_list());
    // Ages are measured as of the end of the scan, the moment the data describes.
    let now = scan.finished_unix;

    if let Some(path) = &args.explain {
        return explain(&scan, &classifier, now, path);
    }

    let found = dc_rules::find_candidates(&scan, &classifier, &FakeResolver::new(), now)
        .context("the scan file is damaged")?;
    // Second look at the real file system: links above a candidate, failures, protected targets.
    let found = dc_rules::verify(found, &classifier, &FsResolver);

    let owners = match &args.apps {
        Some(file) => {
            let snapshot = apps_cmd::load(file)?;
            let attrib =
                dc_attrib::attribute(&scan, &snapshot).context("the scan file is damaged")?;
            Some(attrib)
        }
        None => None,
    };

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
    for item in shown.take(args.limit) {
        let owner = owners
            .as_ref()
            .and_then(|a| dc_attrib::explain(&scan, a, &item.path).ok().flatten())
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

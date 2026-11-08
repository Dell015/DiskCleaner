//! `dc rules`: show or check rule packs (read-only).

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use dc_protected::ProtectedList;
use dc_rules::{builtin_pack_text, parse_pack, validate, RuleScope};

pub fn run(list: bool, check: Option<&Path>) -> Result<ExitCode> {
    if let Some(file) = check {
        let text = std::fs::read_to_string(file)
            .with_context(|| format!("cannot read {}", file.display()))?;
        let problems = match parse_pack(&text) {
            Ok(pack) => {
                let problems = validate(&pack, &ProtectedList::builtin());
                if problems.is_empty() {
                    println!("ok: {} rules, no problems", pack.rules.len());
                    return Ok(ExitCode::SUCCESS);
                }
                problems
            }
            Err(problems) => problems,
        };
        for problem in &problems {
            println!("problem: {problem}");
        }
        println!("{} problem(s); this pack cannot be used", problems.len());
        return Ok(ExitCode::from(1));
    }
    debug_assert!(list);
    let pack = parse_pack(builtin_pack_text())
        .map_err(|p| anyhow::anyhow!("built-in pack is invalid: {p:?}"))?;
    println!(
        "{:<26} {:<13} {:<24} {:>5}  path",
        "id", "risk", "reason", "age"
    );
    for rule in &pack.rules {
        let age = rule
            .min_age_days
            .map_or_else(|| "-".to_owned(), |d| format!("{d}d"));
        let scope = if rule.scope == RuleScope::SelfOnly {
            " (self)"
        } else {
            ""
        };
        println!(
            "{:<26} {:<13} {:<24} {:>5}  {}{scope}",
            rule.id,
            rule.risk.as_str(),
            rule.reason,
            age,
            rule.path
        );
    }
    println!();
    println!("Safe means \"rebuilds itself\", never \"deleted\". The protected list always wins. Highest risk wins.");
    Ok(ExitCode::SUCCESS)
}

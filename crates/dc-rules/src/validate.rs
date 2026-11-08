//! Checks a pack must pass before it can be used. All problems are reported together.

use dc_core::Risk;
use dc_protected::{normalize, FakeResolver, ProtectedList};
use std::collections::BTreeSet;

use crate::pack::{compile_pattern, Problem, RulePack, RuleScope};

fn problem(rule: &str, message: impl Into<String>) -> Problem {
    Problem {
        rule: Some(rule.to_owned()),
        message: message.into(),
    }
}

fn is_kebab(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// Minimum number of plain-text components in a rule path.
pub const MIN_LITERALS: usize = 3;

/// Check a pack. An empty result means it is usable.
pub fn validate(pack: &RulePack, protected: &ProtectedList) -> Vec<Problem> {
    let mut problems = Vec::new();
    if pack.schema != 1 {
        problems.push(Problem {
            rule: None,
            message: format!("schema {} is not supported (only 1)", pack.schema),
        });
    }
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for rule in &pack.rules {
        let id = rule.id.as_str();
        if !is_kebab(id) {
            problems.push(problem(id, "id must be kebab-case (a-z, 0-9, '-')"));
        }
        if !ids.insert(id) {
            problems.push(problem(id, "duplicate id"));
        }
        if rule.reason.trim().is_empty() || rule.summary.trim().is_empty() {
            problems.push(problem(id, "reason and summary must not be empty"));
        }
        if let Some(days) = rule.min_age_days {
            if !(1..=3650).contains(&days) {
                problems.push(problem(id, "min_age_days must be between 1 and 3650"));
            }
        }
        if rule.min_age_days.is_some() && rule.scope == RuleScope::SelfOnly {
            problems.push(problem(id, "min_age_days needs scope = \"subtree\""));
        }
        let Some(pattern) = compile_pattern(&rule.path) else {
            problems.push(problem(
                id,
                "path is not a valid pattern like *:\\Users\\*\\AppData\\...",
            ));
            continue;
        };
        // A path without wildcards names exactly one place and cannot be too broad; a path with
        // wildcards must still name at least MIN_LITERALS components.
        let specific = if pattern.has_wildcard_segments() {
            pattern.named_count() >= MIN_LITERALS
        } else {
            pattern.named_count() >= 1
        };
        if !specific {
            problems.push(problem(
                id,
                format!("path is too broad: with wildcards it needs at least {MIN_LITERALS} named components"),
            ));
        }
        if rule.risk != Risk::DontTouch && !pattern.last_is_named() {
            problems.push(problem(
                id,
                "a Safe or ProbablySafe rule must end in a named component",
            ));
        }
        if rule.examples_match.is_empty() || rule.examples_no_match.is_empty() {
            problems.push(problem(
                id,
                "needs at least one examples_match and one examples_no_match",
            ));
        }
        for example in &rule.examples_match {
            match normalize(example) {
                Err(_) => problems.push(problem(
                    id,
                    format!("example is not a usable path: {example}"),
                )),
                Ok(normal) => {
                    let reach = match rule.scope {
                        RuleScope::Subtree => pattern.matches_subtree(&normal),
                        RuleScope::SelfOnly => pattern.matches_exactly(&normal),
                    };
                    if !reach {
                        problems.push(problem(
                            id,
                            format!("example does not match the rule: {example}"),
                        ));
                    }
                    if protected.check(example, &FakeResolver::new()).protected {
                        problems.push(problem(
                            id,
                            format!(
                                "example is protected, so the rule would never apply: {example}"
                            ),
                        ));
                    }
                }
            }
        }
        for example in &rule.examples_no_match {
            if let Ok(normal) = normalize(example) {
                let reach = match rule.scope {
                    RuleScope::Subtree => pattern.matches_subtree(&normal),
                    RuleScope::SelfOnly => pattern.matches_exactly(&normal),
                };
                if reach {
                    problems.push(problem(
                        id,
                        format!("example must not match but does: {example}"),
                    ));
                }
            } else {
                problems.push(problem(
                    id,
                    format!("example is not a usable path: {example}"),
                ));
            }
        }
    }
    problems
}

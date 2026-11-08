//! Classification of one item: protected list first, then the rules, escalate-only.

use dc_core::Risk;
use dc_protected::{normalize, NormalPath, PathResolver, ProtectedList, Reason, Resolution};

use crate::pack::{compile_pattern, CompiledRule, Problem, RulePack, RuleScope};
use crate::validate::validate;

/// What the scan knows about an item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeFacts {
    /// A folder.
    pub is_dir: bool,
    /// Last-modified time, seconds since the Unix epoch.
    pub mtime_secs: Option<i64>,
    /// A link, junction or other reparse point.
    pub reparse: bool,
    /// A cloud placeholder (OneDrive and similar).
    pub cloud: bool,
    /// A file with more than one name.
    pub hardlink: bool,
}

/// The answer for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// Resulting risk. Never "better" than the riskiest contribution.
    pub risk: Risk,
    /// Why, in stable ids with text.
    pub reasons: Vec<Reason>,
    /// Ids of the rules that matched.
    pub rules: Vec<String>,
}

fn reason(id: &str, text: &str) -> Reason {
    Reason {
        id: id.to_owned(),
        description: text.to_owned(),
    }
}

/// Protected list plus a validated rule pack.
#[derive(Debug, Clone)]
pub struct Classifier {
    protected: ProtectedList,
    rules: Vec<CompiledRule>,
}

impl Classifier {
    /// Build from a protected list and pack text. The pack is validated; any problem rejects it.
    pub fn from_pack_text(
        protected: ProtectedList,
        text: &str,
    ) -> Result<Classifier, Vec<Problem>> {
        let pack = crate::pack::parse(text)?;
        Classifier::from_pack(protected, &pack)
    }

    /// Build from a parsed pack.
    pub fn from_pack(
        protected: ProtectedList,
        pack: &RulePack,
    ) -> Result<Classifier, Vec<Problem>> {
        let problems = validate(pack, &protected);
        if !problems.is_empty() {
            return Err(problems);
        }
        let mut rules = Vec::with_capacity(pack.rules.len());
        for rule in &pack.rules {
            // Validation already parsed every pattern; a miss here is reported, not ignored.
            let Some(pattern) = compile_pattern(&rule.path) else {
                return Err(vec![Problem {
                    rule: Some(rule.id.clone()),
                    message: "path pattern cannot be parsed".to_owned(),
                }]);
            };
            rules.push(CompiledRule {
                rule: rule.clone(),
                pattern,
            });
        }
        Ok(Classifier { protected, rules })
    }

    /// The built-in pack with the built-in protected list. If the built-in pack were ever
    /// invalid (a test makes sure it is not), the classifier has no rules and therefore
    /// classifies everything as `DontTouch`.
    pub fn builtin() -> Classifier {
        Classifier::builtin_with(ProtectedList::builtin())
    }

    /// The built-in pack with a given protected list (for example with extra entries).
    pub fn builtin_with(protected: ProtectedList) -> Classifier {
        match Classifier::from_pack_text(protected.clone(), crate::builtin::PACK_TEXT) {
            Ok(classifier) => classifier,
            Err(_) => Classifier {
                protected,
                rules: Vec::new(),
            },
        }
    }

    /// The protected list in use.
    pub fn protected(&self) -> &ProtectedList {
        &self.protected
    }

    /// The compiled rules.
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }

    /// Does `rule` apply to this path and these facts? `now` is seconds since the Unix epoch.
    fn applies(rule: &CompiledRule, path: &NormalPath, facts: &NodeFacts, now: i64) -> bool {
        let reach = match rule.rule.scope {
            RuleScope::Subtree => rule.pattern.matches_subtree(path),
            RuleScope::SelfOnly => rule.pattern.matches_exactly(path),
        };
        if !reach {
            return false;
        }
        match rule.rule.min_age_days {
            None => true,
            // Age rules are about files; they need a known modification time.
            Some(days) => {
                !facts.is_dir
                    && facts.mtime_secs.is_some_and(|m| {
                        now.saturating_sub(m) >= i64::from(days).saturating_mul(86_400)
                    })
            }
        }
    }

    /// Classify one item. `now` is the current time in seconds since the Unix epoch.
    pub fn classify(
        &self,
        path: &str,
        facts: &NodeFacts,
        resolver: &dyn PathResolver,
        now: i64,
    ) -> Classification {
        let verdict = self.protected.check(path, resolver);
        if verdict.protected {
            return Classification {
                risk: Risk::DontTouch,
                reasons: verdict.reasons,
                rules: Vec::new(),
            };
        }
        let mut reasons: Vec<Reason> = Vec::new();
        let mut risks: Vec<Risk> = Vec::new();
        let mut matched: Vec<String> = Vec::new();

        if facts.reparse {
            risks.push(Risk::DontTouch);
            reasons.push(reason("link", "a link or junction (never followed)"));
        }
        if facts.cloud {
            risks.push(Risk::DontTouch);
            reasons.push(reason("cloud_file", "a cloud placeholder file"));
        }
        if facts.hardlink {
            risks.push(Risk::DontTouch);
            reasons.push(reason("hardlink", "a file with more than one name"));
        }

        let typed = normalize(path).ok();
        let resolved = match resolver.resolve(path) {
            Resolution::Resolved(real) => normalize(&real).ok().map(Some),
            Resolution::NotFound => Some(None),
            Resolution::Failed(_) => None,
        };
        match (&typed, &resolved) {
            (Some(typed), Some(resolved)) => {
                for rule in &self.rules {
                    let by_typed = Self::applies(rule, typed, facts, now);
                    let by_real = resolved
                        .as_ref()
                        .map_or(by_typed, |r| Self::applies(rule, r, facts, now));
                    // Risky rules apply if either spelling matches; lenient rules need both.
                    let hit = if rule.rule.risk == Risk::DontTouch {
                        by_typed || by_real
                    } else {
                        by_typed && by_real
                    };
                    if hit {
                        risks.push(rule.rule.risk);
                        reasons.push(reason(&rule.rule.reason, &rule.rule.summary));
                        matched.push(rule.rule.id.clone());
                    }
                }
            }
            _ => {
                risks.push(Risk::DontTouch);
                reasons.push(reason(
                    "unresolved",
                    "the path could not be understood or resolved",
                ));
            }
        }
        if matched.is_empty() && risks.is_empty() {
            risks.push(Risk::DontTouch);
            reasons.push(reason(
                "unclassified",
                "no rule describes this; unknown means hands off",
            ));
        }
        // Keep the first reason per id.
        let mut seen: Vec<String> = Vec::new();
        reasons.retain(|r| {
            if seen.contains(&r.id) {
                false
            } else {
                seen.push(r.id.clone());
                true
            }
        });
        Classification {
            risk: Risk::escalate_all(risks),
            reasons,
            rules: matched,
        }
    }
}

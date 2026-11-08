//! Rule packs: the TOML format and its compiled form.

use dc_core::Risk;
use dc_protected::Pattern;
use serde::Deserialize;

/// How far a rule reaches from the folder its path names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleScope {
    /// The folder and everything below it.
    Subtree,
    /// Only the item itself.
    #[serde(rename = "self")]
    SelfOnly,
}

fn default_scope() -> RuleScope {
    RuleScope::Subtree
}

/// One rule as written in a pack.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Unique kebab-case id.
    pub id: String,
    /// Risk this rule gives to what it matches.
    pub risk: Risk,
    /// Stable machine-readable reason id.
    pub reason: String,
    /// One sentence for people.
    pub summary: String,
    /// Path pattern (same syntax as the protected list).
    pub path: String,
    /// Reach of the pattern.
    #[serde(default = "default_scope")]
    pub scope: RuleScope,
    /// Only files at least this many days old match.
    #[serde(default)]
    pub min_age_days: Option<u32>,
    /// Paths that must match (and must not be protected).
    #[serde(default)]
    pub examples_match: Vec<String>,
    /// Paths that must not match.
    #[serde(default)]
    pub examples_no_match: Vec<String>,
}

/// A pack as written in a TOML file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulePack {
    /// Format version; only 1 exists.
    pub schema: u32,
    /// The rules.
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// A problem found while loading or checking a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The rule id, when the problem belongs to one rule.
    pub rule: Option<String>,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.rule {
            Some(rule) => write!(f, "rule \"{rule}\": {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// A rule together with its parsed pattern.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    /// The rule as written.
    pub rule: Rule,
    /// Its pattern.
    pub pattern: Pattern,
}

/// Parse TOML text into a pack. Only syntax and field names are checked here; see
/// [`crate::validate`] for the rest.
pub fn parse(text: &str) -> Result<RulePack, Vec<Problem>> {
    toml::from_str::<RulePack>(text).map_err(|error| {
        vec![Problem {
            rule: None,
            message: format!("not a valid rule pack: {error}"),
        }]
    })
}

/// Pattern text as authors write it (any case) turned into a pattern.
pub fn compile_pattern(text: &str) -> Option<Pattern> {
    Pattern::parse(&text.trim().replace('/', "\\").to_lowercase())
}

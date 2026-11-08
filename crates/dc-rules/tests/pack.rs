//! The built-in pack must pass its own validation, and malformed packs must be rejected.
#![allow(missing_docs)]

use dc_protected::ProtectedList;
use dc_rules::{builtin_pack_text, parse_pack, validate, Classifier};

#[test]
fn the_builtin_pack_is_valid() {
    let pack = parse_pack(builtin_pack_text()).expect("parses");
    let problems = validate(&pack, &ProtectedList::builtin());
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(!Classifier::builtin().rules().is_empty());
    assert!(pack.rules.len() >= 30);
}

fn reject(text: &str, needle: &str) {
    let result = Classifier::from_pack_text(ProtectedList::builtin(), text);
    let problems = match result {
        Ok(_) => panic!("pack should have been rejected: {text}"),
        Err(p) => p,
    };
    assert!(
        problems.iter().any(|p| p.message.contains(needle)),
        "expected a problem mentioning {needle:?}, got {problems:#?}"
    );
}

const GOOD: &str = r#"
schema = 1
[[rule]]
id = "demo-cache"
risk = "Safe"
reason = "demo"
summary = "Demo"
path = '*:\Users\*\AppData\Local\Demo\Cache'
examples_match = ['C:\Users\bob\AppData\Local\Demo\Cache\a']
examples_no_match = ['C:\Users\bob\AppData\Local\Demo\Data\a']
"#;

#[test]
fn a_small_good_pack_loads() {
    assert!(Classifier::from_pack_text(ProtectedList::builtin(), GOOD).is_ok());
}

#[test]
fn unknown_fields_wrong_risk_and_wrong_schema_are_errors() {
    reject(
        &GOOD.replace("summary = \"Demo\"", "summary = \"Demo\"\nsurprise = 1"),
        "unknown field",
    );
    reject(&GOOD.replace("\"Safe\"", "\"safe\""), "unknown variant");
    reject(&GOOD.replace("schema = 1", "schema = 2"), "schema 2");
    reject("this is not toml [", "not a valid rule pack");
}

#[test]
fn too_broad_or_unnamed_paths_are_rejected() {
    reject(
        &GOOD.replace(r"*:\Users\*\AppData\Local\Demo\Cache", r"*:\Users\*\*"),
        "too broad",
    );
    reject(
        &GOOD.replace(r"*:\Users\*\AppData\Local\Demo\Cache", r"*:\*\*\*\*"),
        "too broad",
    );
    reject(
        &GOOD.replace(r"Local\Demo\Cache", r"Local\Demo\*"),
        "named component",
    );
}

#[test]
fn examples_are_required_and_must_be_truthful() {
    reject(
        &GOOD.replace(
            "examples_match = ['C:\\Users\\bob\\AppData\\Local\\Demo\\Cache\\a']",
            "examples_match = []",
        ),
        "needs at least one",
    );
    reject(
        &GOOD.replace(
            "Local\\Demo\\Cache\\a']\nexamples_no",
            "Local\\Other\\a']\nexamples_no",
        ),
        "does not match the rule",
    );
    reject(
        &GOOD.replace("Local\\Demo\\Data\\a", "Local\\Demo\\Cache\\b"),
        "must not match",
    );
}

#[test]
fn rules_that_could_never_apply_because_the_path_is_protected_are_rejected() {
    let pack = GOOD
        .replace(r"*:\Users\*\AppData\Local\Demo\Cache", r"*:\Windows\Temp")
        .replace(
            r"C:\Users\bob\AppData\Local\Demo\Cache\a",
            r"C:\Windows\Temp\a",
        )
        .replace(
            r"C:\Users\bob\AppData\Local\Demo\Data\a",
            r"C:\Other\Temp\a",
        );
    reject(&pack, "protected");
}

#[test]
fn duplicate_ids_and_bad_ages_are_rejected() {
    let doubled = format!("{GOOD}{}", GOOD.replace("schema = 1", ""));
    reject(&doubled, "duplicate id");
    reject(
        &GOOD.replace("risk = \"Safe\"", "risk = \"Safe\"\nmin_age_days = 0"),
        "min_age_days",
    );
    reject(
        &GOOD.replace("id = \"demo-cache\"", "id = \"Demo Cache\""),
        "kebab-case",
    );
}

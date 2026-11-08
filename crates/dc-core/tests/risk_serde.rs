//! Serialization tests for `Risk`: stable names, exact matching, no silent downgrades.

use dc_core::Risk;

#[test]
fn json_round_trip_for_every_variant() {
    for risk in Risk::ALL {
        let json = serde_json::to_string(&risk).expect("serialize");
        let back: Risk = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(risk, back);
    }
}

#[test]
fn json_snapshot_of_all_variants() {
    insta::assert_json_snapshot!(Risk::ALL);
}

#[test]
fn unknown_miscased_or_non_string_values_are_rejected() {
    for bad in [
        "\"safe\"",
        "\"SAFE\"",
        "\"Unknown\"",
        "\"\"",
        "\" Safe\"",
        "\"probablysafe\"",
        "0",
        "1",
        "null",
        "true",
        "[]",
        "{}",
    ] {
        assert!(
            serde_json::from_str::<Risk>(bad).is_err(),
            "{bad} must not deserialize into any Risk variant"
        );
    }
}

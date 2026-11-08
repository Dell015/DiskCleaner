//! Property tests: buckets always add up, order never matters, nothing panics.
#![allow(missing_docs)]

mod common;
use common::{apps, scan};
use dc_attrib::attribute;
use proptest::prelude::*;

const DIRS: [&str; 8] = [
    "Program Files",
    "Windows",
    "Users",
    "AppData",
    "Alpha App",
    "Beta Suite",
    "Data",
    "Contoso",
];

fn path_strategy() -> impl Strategy<Value = (String, u64)> {
    (
        prop::collection::vec(prop::sample::select(DIRS.to_vec()), 1..5),
        1u64..10_000,
    )
        .prop_map(|(dirs, size)| {
            let mut parts: Vec<String> = dirs.iter().map(|d| (*d).to_owned()).collect();
            parts.push("file.bin".to_owned());
            (parts.join("\\"), size)
        })
}

proptest! {
    #[test]
    fn every_byte_is_in_exactly_one_bucket(mut files in prop::collection::vec(path_strategy(), 1..30), shuffle in any::<bool>()) {
        // Distinct paths only (a path can be a file or a folder, not both).
        files.sort();
        files.dedup_by(|a, b| a.0 == b.0);
        let all = files.clone();
        files.retain(|(p, _)| !all.iter().any(|(q, _)| q.len() > p.len() && q.starts_with(&format!("{p}\\"))));
        let refs: Vec<(&str, u64)> = files.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        let snapshot = apps(&[
            ("Alpha App", "Contoso Ltd", Some(r"C:\Program Files\Alpha App")),
            ("Beta Suite", "Contoso Ltd", Some(r"C:\Program Files\Beta Suite")),
            ("Beta Suite Pro", "Contoso Ltd", Some(r"C:\Program Files\Beta Suite")),
        ]);
        let result = attribute(&scan(&refs), &snapshot);
        prop_assume!(result.is_ok());
        let result = result.unwrap_or_else(|_| unreachable!());
        let sum: u64 = result.totals.iter().map(|t| t.own.on_disk).sum::<u64>()
            + result.shared.on_disk + result.unattributed.on_disk;
        prop_assert_eq!(sum, result.scan_total_on_disk);

        if shuffle {
            let mut reversed = refs.clone();
            reversed.reverse();
            // Same content in another order gives the same bucket totals.
            let again = attribute(&scan(&reversed), &snapshot);
            prop_assume!(again.is_ok());
            let again = again.unwrap_or_else(|_| unreachable!());
            prop_assert_eq!(again.shared, result.shared);
            prop_assert_eq!(again.unattributed, result.unattributed);
        }
    }
}

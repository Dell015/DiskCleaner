//! Property tests for the classifier.
#![allow(missing_docs)]

use dc_core::Risk;
use dc_protected::{FakeResolver, ProtectedList};
use dc_rules::{Classifier, NodeFacts};
use proptest::prelude::*;

const NOW: i64 = 1_800_000_000;

proptest! {
    #[test]
    fn never_panics(text in ".{0,80}", age in 0i64..2000, dir in any::<bool>()) {
        let facts = NodeFacts { is_dir: dir, mtime_secs: Some(NOW - age * 86_400), ..NodeFacts::default() };
        let _ = Classifier::builtin().classify(&text, &facts, &FakeResolver::new(), NOW);
    }

    #[test]
    fn protected_paths_are_always_dont_touch(
        tail in "[A-Za-z0-9_. ]{0,12}",
        base in prop::sample::select(vec![
            r"C:\Windows\System32", r"C:\Users\bob\Documents", r"C:\Users\bob\.ssh",
            r"D:\proj\.git", r"C:\ProgramData\Microsoft", r"E:\vm\disk.vhdx",
        ]),
        age in 0i64..2000,
    ) {
        let path = format!("{base}\\{tail}");
        let facts = NodeFacts { mtime_secs: Some(NOW - age * 86_400), ..NodeFacts::default() };
        let classifier = Classifier::builtin();
        prop_assert_eq!(classifier.classify(&path, &facts, &FakeResolver::new(), NOW).risk, Risk::DontTouch);
    }

    #[test]
    fn extra_protection_never_lowers_a_result(
        tail in "[A-Za-z0-9_.]{1,8}",
        base in prop::sample::select(vec![
            r"C:\Users\bob\AppData\Local\D3DSCache", r"C:\Users\bob\AppData\Local\Temp",
            r"C:\Users\bob\AppData\Roaming\discord\Cache", r"C:\Windows.old", r"C:\Other",
        ]),
        extra in prop::sample::select(vec![r"C:\Users\bob", r"C:\Windows.old", r"D:\x", r"C:\Other"]),
    ) {
        let path = format!("{base}\\{tail}");
        let facts = NodeFacts { mtime_secs: Some(NOW - 900 * 86_400), ..NodeFacts::default() };
        let plain = Classifier::builtin().classify(&path, &facts, &FakeResolver::new(), NOW).risk;
        let list = ProtectedList::builtin().with_extra(extra, "x", "x");
        let more = Classifier::builtin_with(list).classify(&path, &facts, &FakeResolver::new(), NOW).risk;
        prop_assert!(more >= plain);
    }
}

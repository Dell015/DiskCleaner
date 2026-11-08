//! Property tests: invariants that must hold for any machine.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
use common::*;
use dc_core::Risk;
use dc_protected::FakeResolver;
use dc_rules::Classifier;
use proptest::prelude::*;

const CONTAINERS: [&str; 6] = [
    r"ProgramData",
    r"Program Files",
    r"Program Files (x86)",
    r"Users\bob\AppData\Local",
    r"Users\bob\AppData\Roaming",
    r"Users\bob\AppData\LocalLow",
];
const NAMES: [&str; 12] = [
    "McAfee",
    "Tool",
    "Tool.old",
    "Tool.bak",
    "VALORANT",
    "Mystery",
    "Microsoft",
    "Windows NT",
    "Gone App",
    "Zoom",
    "x",
    "Steam",
];
const FILES: [&str; 8] = [
    "a.cfg",
    "m.exe",
    "n.dll",
    "doc.docx",
    "data.sqlite",
    "key.pem",
    "readme.txt",
    "save.dat",
];

fn files_strategy() -> impl Strategy<Value = Vec<(usize, usize, usize, i64, bool)>> {
    prop::collection::vec(
        (
            0..CONTAINERS.len(),
            0..NAMES.len(),
            0..FILES.len(),
            0i64..1200,
            any::<bool>(),
        ),
        0..25,
    )
}

fn build(spec: &[(usize, usize, usize, i64, bool)]) -> (Vec<F>, Vec<A>) {
    let files = spec
        .iter()
        .map(|&(c, n, fl, age, is_link)| {
            let path: &'static str = Box::leak(
                format!("{}\\{}\\{}", CONTAINERS[c], NAMES[n], FILES[fl]).into_boxed_str(),
            );
            if is_link {
                link(path, age)
            } else {
                f(path, age)
            }
        })
        .collect();
    let apps = vec![
        app("Gone App", r"C:\Program Files\Gone App", true),
        app("Zoom", r"C:\Program Files\Zoom", false),
    ];
    (files, apps)
}

proptest! {
    #[test]
    fn invariants_hold_on_any_machine(spec in files_strategy()) {
        let (files, apps) = build(&spec);
        let classifier = Classifier::builtin();
        let found = run_with(&files, &apps, &classifier, &FakeResolver::new());
        prop_assert!(dc_leftovers::never_safe(&found));
        let mut offered = 0u64;
        let mut shown = 0u64;
        for finding in &found.findings {
            prop_assert!(finding.points <= 100);
            prop_assert!(!finding.evidence.is_empty());
            prop_assert!(!classifier.protected().check(&finding.candidate.path, &FakeResolver::new()).protected);
            // Low confidence, personal data or a game-launcher style app is never offered.
            if finding.confidence == dc_leftovers::Confidence::Low
                || !finding.personal_data.is_empty()
                || finding.report_only_app
            {
                prop_assert_eq!(finding.candidate.risk, Risk::DontTouch);
            }
            if finding.candidate.risk == Risk::DontTouch {
                shown += finding.candidate.sizes.on_disk;
            } else {
                prop_assert_eq!(finding.candidate.risk, Risk::ProbablySafe);
                offered += finding.candidate.sizes.on_disk;
            }
        }
        prop_assert_eq!(found.offered.on_disk, offered);
        prop_assert_eq!(found.reported_only.on_disk, shown);
        prop_assert!(found.findings.len() + found.refused.len() <= found.examined as usize);
        // A folder with a link inside is never a finding.
        for finding in &found.findings {
            let path = finding.candidate.path.to_lowercase();
            for (i, file) in files.iter().enumerate() {
                if file.special == "link" {
                    let _ = i;
                    let full = format!(r"c:\{}", file.path.to_lowercase());
                    prop_assert!(!full.starts_with(&format!("{path}\\")), "{path} holds a link");
                }
            }
        }
    }

    #[test]
    fn verifying_twice_changes_nothing(spec in files_strategy()) {
        let (files, apps) = build(&spec);
        let classifier = Classifier::builtin();
        let scan = scan(&files);
        let snap = snapshot(&apps);
        let attrib = attribution(&scan, &snap);
        let resolver = FakeResolver::new();
        let found = dc_leftovers::find_leftovers(&scan, &snap, &attrib, &classifier, &resolver).unwrap();
        let once = dc_leftovers::verify(found, &classifier, &resolver);
        let twice = dc_leftovers::verify(once.clone(), &classifier, &resolver);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn odd_names_never_panic(name in ".{1,20}", age in 0i64..2000) {
        let name = name.replace(['\\', '/'], "_");
        let path: &'static str = Box::leak(format!(r"ProgramData\{name}\a.cfg").into_boxed_str());
        let found = run(&[f(path, age)], &[]);
        prop_assert!(dc_leftovers::never_safe(&found));
    }
}

//! Candidate listing on hand-made scans.
#![allow(missing_docs)]

mod common;
use common::{f, scan, F, NOW};
use dc_core::Risk;
use dc_protected::FakeResolver;
use dc_rules::{explain, find_candidates, Classifier};

fn run(files: &[F]) -> dc_rules::Candidates {
    find_candidates(
        &scan(files),
        &Classifier::builtin(),
        &FakeResolver::new(),
        NOW,
    )
    .unwrap()
}

#[test]
fn a_cache_folder_is_one_candidate_with_its_total_size() {
    let r = run(&[
        f(r"Users\bob\AppData\Local\D3DSCache\a\1.bin", 1000, 1),
        f(r"Users\bob\AppData\Local\D3DSCache\b.bin", 500, 1),
        f(r"Users\bob\Documents\keep.txt", 99, 1),
    ]);
    assert_eq!(r.items.len(), 1);
    let item = &r.items[0];
    assert_eq!(item.path, r"C:\Users\bob\AppData\Local\D3DSCache");
    assert_eq!(item.sizes.on_disk, 1500);
    assert_eq!(item.sizes.files, 2);
    assert_eq!(item.risk, Risk::Safe);
    assert_eq!(r.safe.on_disk, 1500);
    assert_eq!(r.probably_safe.on_disk, 0);
}

#[test]
fn old_temp_files_are_grouped_and_young_ones_left_out() {
    let r = run(&[
        f(r"Users\bob\AppData\Local\Temp\old1.tmp", 100, 30),
        f(r"Users\bob\AppData\Local\Temp\sub\old2.tmp", 200, 40),
        f(r"Users\bob\AppData\Local\Temp\young.tmp", 900, 1),
    ]);
    assert_eq!(r.items.len(), 1, "{:?}", r.items);
    assert_eq!(r.items[0].path, r"C:\Users\bob\AppData\Local\Temp");
    assert_eq!(r.items[0].sizes.on_disk, 300);
    assert!(r.items[0].partial);
}

#[test]
fn a_link_inside_blocks_the_whole_folder() {
    let r = run(&[
        f(r"Users\bob\AppData\Local\D3DSCache\a.bin", 1000, 1),
        F {
            path: r"Users\bob\AppData\Local\D3DSCache\pointer",
            size: 0,
            age_days: 1,
            special: "link",
        },
    ]);
    // The folder as a whole is refused; only ordinary files are still offered, as partial
    // candidates that leave out the blocked part.
    assert!(r.items.iter().all(|i| i.partial), "{:?}", r.items);
    assert_eq!(r.items.iter().map(|i| i.sizes.files).sum::<u64>(), 1);
    assert_eq!(r.blocked.len(), 1);
    assert!(r.blocked[0].because.contains("link"), "{:?}", r.blocked);
}

#[test]
fn something_protected_inside_blocks_the_folder() {
    let r = run(&[
        f(r"Users\bob\AppData\Local\D3DSCache\a.bin", 1000, 1),
        f(r"Users\bob\AppData\Local\D3DSCache\repo\.git\HEAD", 10, 1),
    ]);
    // The folder as a whole is refused; only ordinary files are still offered, as partial
    // candidates that leave out the blocked part.
    assert!(r.items.iter().all(|i| i.partial), "{:?}", r.items);
    assert_eq!(r.items.iter().map(|i| i.sizes.files).sum::<u64>(), 1);
    assert!(
        r.blocked[0].because.contains("protected"),
        "{:?}",
        r.blocked
    );
}

#[test]
fn nothing_inside_protected_areas_is_offered() {
    let r = run(&[
        f(r"Windows\Temp\a.tmp", 100, 900),
        f(r"Users\bob\Documents\Temp\old.tmp", 100, 900),
        f(r"Program Files\App\Cache\a", 100, 900),
    ]);
    assert!(r.items.is_empty(), "{:?}", r.items);
}

#[test]
fn candidates_are_sorted_biggest_first_and_not_nested() {
    let r = run(&[
        f(r"Users\bob\AppData\Local\D3DSCache\a", 100, 1),
        f(r"Users\bob\AppData\Local\NVIDIA\DXCache\a", 900, 1),
        f(r"Windows.old\Windows\x.dll", 5000, 1),
    ]);
    let sizes: Vec<u64> = r.items.iter().map(|i| i.sizes.on_disk).collect();
    assert_eq!(sizes, vec![5000, 900, 100]);
    assert_eq!(r.items[0].risk, Risk::ProbablySafe);
    assert_eq!(r.probably_safe.on_disk, 5000);
    assert_eq!(r.safe.on_disk, 1000);
}

#[test]
fn explain_shows_the_steps_for_a_path_in_the_scan() {
    let s = scan(&[f(r"Users\bob\AppData\Local\D3DSCache\a", 100, 1)]);
    let c = Classifier::builtin();
    let e = explain(
        &s,
        &c,
        &FakeResolver::new(),
        NOW,
        r"C:\Users\bob\AppData\Local\D3DSCache",
    )
    .unwrap()
    .unwrap();
    assert_eq!(e.classification.risk, Risk::Safe);
    assert!(e.is_dir && e.blocker.is_none());
    assert!(explain(&s, &c, &FakeResolver::new(), NOW, r"C:\Nope")
        .unwrap()
        .is_none());
    let w = explain(&s, &c, &FakeResolver::new(), NOW, r"C:\Users")
        .unwrap()
        .unwrap();
    assert_eq!(w.classification.risk, Risk::DontTouch);
}

#[test]
fn verify_moves_candidates_behind_links_or_failures_to_blocked() {
    let c = Classifier::builtin();
    let found = run(&[
        f(r"Users\bob\AppData\Local\D3DSCache\a", 100, 1),
        f(r"Users\bob\AppData\Local\NVIDIA\DXCache\a", 50, 1),
    ]);
    assert_eq!(found.items.len(), 2);
    let resolver = FakeResolver::new()
        .link(r"C:\Users\bob\AppData\Local\D3DSCache", r"D:\Elsewhere")
        .fail(r"C:\Users\bob\AppData\Local\NVIDIA\DXCache");
    let checked = dc_rules::verify(found, &c, &resolver);
    assert!(checked.items.is_empty(), "{:?}", checked.items);
    assert_eq!(checked.blocked.len(), 2);
    assert_eq!(checked.safe.on_disk, 0);
    // A path the resolver does not know (scan from another machine) is kept.
    let found = run(&[f(r"Users\bob\AppData\Local\D3DSCache\a", 100, 1)]);
    assert_eq!(
        dc_rules::verify(found, &c, &FakeResolver::new())
            .items
            .len(),
        1
    );
}

/// The answer must not depend on how the work is divided between threads (D68).
#[test]
fn splitting_the_work_never_changes_the_answer() {
    let mut files: Vec<F> = Vec::new();
    // Names must outlive the call; leak a few static strings built from a small grid.
    for user in ["alice", "bob", "carol"] {
        for k in 0..6 {
            let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
            files.push(f(
                leak(format!(r"Users\{user}\AppData\Local\Temp\t{k}.tmp")),
                100 + k,
                if k % 2 == 0 { 40 } else { 1 },
            ));
            files.push(f(
                leak(format!(r"Users\{user}\AppData\Local\D3DSCache\d{k}\x.bin")),
                1000 * (k + 1),
                1,
            ));
            files.push(f(
                leak(format!(r"Users\{user}\Documents\proj{k}\keep{k}.txt")),
                77,
                500,
            ));
            files.push(f(
                leak(format!(
                    r"Users\{user}\AppData\Local\Pub\Cache\hosted\p{k}\a.dart"
                )),
                10,
                5,
            ));
        }
    }
    let scan = scan(&files);
    let classifier = Classifier::builtin();
    let resolver = FakeResolver::new();
    let one_thread =
        dc_rules::find_candidates_split(&scan, &classifier, &resolver, NOW, usize::MAX).unwrap();
    assert!(!one_thread.items.is_empty());
    for split in [1usize, 2, 3, 5, 8, 50] {
        let divided =
            dc_rules::find_candidates_split(&scan, &classifier, &resolver, NOW, split).unwrap();
        assert_eq!(divided, one_thread, "split at {split}");
    }
}

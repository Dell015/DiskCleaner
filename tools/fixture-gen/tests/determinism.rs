#![allow(missing_docs)]
mod common;

#[test]
fn same_seed_gives_identical_tree_and_manifest() {
    let (a, ma) = common::run("det-a", 42, 200);
    let (b, mb) = common::run("det-b", 42, 200);
    assert_eq!(ma, mb);
    assert_eq!(
        std::fs::read(a.join("manifest.json")).unwrap(),
        std::fs::read(b.join("manifest.json")).unwrap()
    );
    assert_eq!(
        common::snapshot(&a.join("tree")),
        common::snapshot(&b.join("tree"))
    );
}

#[test]
fn different_seed_gives_different_content() {
    let (a, _) = common::run("diff-a", 1, 50);
    let (b, _) = common::run("diff-b", 2, 50);
    assert_ne!(
        common::snapshot(&a.join("tree")),
        common::snapshot(&b.join("tree"))
    );
}

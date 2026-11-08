//! Ownership scenarios on small hand-made scans. No disk access.
#![allow(missing_docs)]

use dc_attrib::{attribute, explain, AttribResult, Confidence, OwnerKind};

mod common;
use common::{apps, scan};

fn owner_id(result: &AttribResult, name: &str) -> u32 {
    result.owners.iter().find(|o| o.name == name).unwrap().id
}

fn own_bytes(result: &AttribResult, name: &str) -> u64 {
    let id = owner_id(result, name);
    result
        .totals
        .iter()
        .find(|t| t.owner == id)
        .map_or(0, |t| t.own.on_disk)
}

fn assert_buckets_add_up(result: &AttribResult) {
    let sum: u64 = result.totals.iter().map(|t| t.own.on_disk).sum::<u64>()
        + result.shared.on_disk
        + result.unattributed.on_disk;
    assert_eq!(
        sum, result.scan_total_on_disk,
        "every byte must be in exactly one bucket"
    );
}

#[test]
fn install_folder_owns_everything_below_it() {
    let s = scan(&[
        (r"Program Files\Git\bin\git.exe", 1000),
        (r"Program Files\Git\doc\a.txt", 500),
        (r"Other\x.bin", 300),
    ]);
    let a = apps(&[("Git", "The Git Community", Some(r"C:\Program Files\Git"))]);
    let r = attribute(&s, &a).unwrap();
    assert_eq!(own_bytes(&r, "Git"), 1500);
    assert_eq!(r.unattributed.on_disk, 300);
    assert_buckets_add_up(&r);
    let git_dir = r
        .assignments
        .iter()
        .find(|x| x.confidence == Confidence::High)
        .unwrap();
    assert!(!git_dir.evidence.is_empty());

    let e = explain(&s, &r, r"C:\Program Files\Git\bin\git.exe")
        .unwrap()
        .unwrap();
    assert!(e.inherited);
    assert_eq!(e.assignment.unwrap().owner, Some(owner_id(&r, "Git")));
    assert_eq!(e.assigned_path.as_deref(), Some(r"C:\Program Files\Git"));
    let unknown = explain(&s, &r, r"C:\Other\x.bin").unwrap().unwrap();
    assert!(unknown.assignment.is_none());
    assert!(explain(&s, &r, r"C:\Nope").unwrap().is_none());
}

#[test]
fn windows_areas_belong_to_windows() {
    let s = scan(&[
        (r"Windows\System32\a.dll", 700),
        (r"ProgramData\Microsoft\b.dat", 200),
        (r"Stuff\c", 50),
    ]);
    let r = attribute(&s, &apps(&[])).unwrap();
    assert_eq!(own_bytes(&r, "Windows"), 900);
    assert_eq!(r.unattributed.on_disk, 50);
    assert_buckets_add_up(&r);
}

#[test]
fn data_folders_match_the_app_by_name_with_medium_confidence() {
    let s = scan(&[
        (r"Program Files\Mozilla Firefox\f.exe", 100),
        (r"Users\bob\AppData\Roaming\Mozilla Firefox\profile.ini", 40),
        (r"Users\bob\AppData\Local\Temp\junk", 10),
    ]);
    let a = apps(&[(
        "Mozilla Firefox",
        "Mozilla",
        Some(r"C:\Program Files\Mozilla Firefox"),
    )]);
    let r = attribute(&s, &a).unwrap();
    assert_eq!(own_bytes(&r, "Mozilla Firefox"), 140);
    assert!(r
        .assignments
        .iter()
        .any(|x| x.confidence == Confidence::Medium));
    assert_eq!(r.unattributed.on_disk, 10);
    assert_buckets_add_up(&r);
}

#[test]
fn short_or_generic_names_never_claim_folders_by_name() {
    let s = scan(&[
        (r"Users\bob\AppData\Local\Go\cache", 100),
        (r"Users\bob\AppData\Local\Gone\x", 20),
    ]);
    let a = apps(&[("Go", "Google", None), ("Gon", "", None)]);
    let r = attribute(&s, &a).unwrap();
    assert_eq!(own_bytes(&r, "Go"), 0);
    assert_eq!(r.unattributed.on_disk, 120);
}

#[test]
fn two_apps_in_one_folder_are_ambiguous_and_shared() {
    let s = scan(&[(r"Program Files\Suite\a.dll", 400)]);
    let a = apps(&[
        ("Suite Reader", "Acme", Some(r"C:\Program Files\Suite")),
        ("Suite Writer", "Acme", Some(r"C:\Program Files\Suite")),
    ]);
    let r = attribute(&s, &a).unwrap();
    assert_eq!(r.shared.on_disk, 400);
    let amb = r
        .assignments
        .iter()
        .find(|x| x.confidence == Confidence::Ambiguous)
        .unwrap();
    assert_eq!(amb.owner, None);
    assert_eq!(amb.alternatives.len(), 2);
    assert_buckets_add_up(&r);
}

#[test]
fn vendor_folder_keeps_its_own_files_and_members_keep_theirs() {
    let s = scan(&[
        (r"Program Files\Contoso\readme.txt", 10),
        (r"Program Files\Contoso\Alpha\a.exe", 100),
        (r"Program Files\Contoso\Beta\b.exe", 200),
    ]);
    let a = apps(&[
        (
            "Alpha",
            "Contoso Ltd",
            Some(r"C:\Program Files\Contoso\Alpha"),
        ),
        (
            "Beta",
            "Contoso Ltd",
            Some(r"C:\Program Files\Contoso\Beta"),
        ),
    ]);
    let r = attribute(&s, &a).unwrap();
    let vendor = r
        .owners
        .iter()
        .find(|o| o.kind == OwnerKind::Vendor)
        .unwrap();
    assert_eq!(vendor.name, "Contoso Ltd");
    assert_eq!(own_bytes(&r, "Contoso Ltd"), 10);
    assert_eq!(own_bytes(&r, "Alpha"), 100);
    assert_eq!(own_bytes(&r, "Beta"), 200);
    let footprint = r
        .totals
        .iter()
        .find(|t| t.owner == vendor.id)
        .unwrap()
        .footprint_on_disk;
    assert_eq!(footprint, 310);
    assert_buckets_add_up(&r);
}

#[test]
fn big_unowned_folders_are_listed_most_specific_first() {
    let s = scan(&[
        (r"Data\Big\one\x", 900),
        (r"Data\Big\two\y", 50),
        (r"Data\small", 5),
    ]);
    let r = attribute(&s, &apps(&[])).unwrap();
    assert_eq!(
        r.top_unattributed.first().map(|d| d.path.as_str()),
        Some(r"C:\Data\Big\one")
    );
    assert_buckets_add_up(&r);
}

#[test]
fn the_result_is_deterministic_and_round_trips() {
    let s = scan(&[(r"Program Files\A\x", 1), (r"Program Files\B\y", 2)]);
    let a = apps(&[
        ("Aaaa", "", Some(r"C:\Program Files\A")),
        ("Bbbb", "", Some(r"C:\Program Files\B")),
    ]);
    let first = attribute(&s, &a).unwrap();
    assert_eq!(first, attribute(&s, &a).unwrap());
    let json = serde_json::to_string(&first).unwrap();
    assert_eq!(first, serde_json::from_str::<AttribResult>(&json).unwrap());
}

#[test]
fn unowned_list_shows_outer_folders_not_their_descendants() {
    let s = scan(&[
        (r"Data\A\one\x", 100),
        (r"Data\A\two\y", 100),
        (r"Data\B\z", 50),
    ]);
    let r = attribute(&s, &apps(&[])).unwrap();
    let paths: Vec<&str> = r.top_unattributed.iter().map(|d| d.path.as_str()).collect();
    assert!(paths.contains(&r"C:\Data\A"), "{paths:?}");
    assert!(
        !paths.iter().any(|p| p.starts_with(r"C:\Data\A\")),
        "{paths:?}"
    );
}

#[test]
fn publisher_folders_inside_common_files_are_vendor_owned() {
    let s = scan(&[(r"Program Files\Common Files\Contoso\shared.dll", 70)]);
    let a = apps(&[("Alpha", "Contoso Ltd", None)]);
    let r = attribute(&s, &a).unwrap();
    assert_eq!(own_bytes(&r, "Contoso Ltd"), 70);
}

fn fake_with(versions: &[(&str, &str, &str)]) -> dc_winmeta::FakeSource {
    let mut fake = dc_winmeta::FakeSource::new();
    for (path, product, company) in versions {
        fake.versions.insert(
            (*path).to_owned(),
            dc_winmeta::VersionInfo {
                product: Some((*product).into()),
                company: Some((*company).into()),
                ..dc_winmeta::VersionInfo::default()
            },
        );
    }
    fake
}

#[test]
fn a_program_inside_an_app_folder_is_recorded_as_its_main_program() {
    let s = scan(&[
        (r"Program Files\Tool\bin\tool.exe", 900),
        (r"Program Files\Tool\readme.txt", 5),
    ]);
    let a = apps(&[("Super Tool", "Acme Inc", Some(r"C:\Program Files\Tool"))]);
    let source = fake_with(&[(
        r"C:\Program Files\Tool\bin\tool.exe",
        "Super Tool",
        "Acme Inc",
    )]);
    let r = dc_attrib::attribute_with(&s, &a, &source).unwrap();
    assert_eq!(r.programs.len(), 1);
    assert_eq!(r.programs[0].path, r"C:\Program Files\Tool\bin\tool.exe");
    assert_eq!(r.programs[0].owner, owner_id(&r, "Super Tool"));
    // A program of another product inside the folder is not recorded.
    let other = fake_with(&[(
        r"C:\Program Files\Tool\bin\tool.exe",
        "Something Else",
        "Other Corp",
    )]);
    assert!(dc_attrib::attribute_with(&s, &a, &other)
        .unwrap()
        .programs
        .is_empty());
}

#[test]
fn many_components_of_one_publisher_give_the_folder_to_the_publisher_at_low_confidence() {
    let s = scan(&[
        (r"Program Files\dotnet\dotnet.exe", 100),
        (r"Program Files\dotnet\sdk\x.dll", 400),
    ]);
    let a = apps(&[
        (
            "Microsoft .NET Runtime - 8.0.21 (x64)",
            "Microsoft Corporation",
            None,
        ),
        (
            "Microsoft .NET Host - 8.0.21 (x64)",
            "Microsoft Corporation",
            None,
        ),
    ]);
    let source = fake_with(&[(
        r"C:\Program Files\dotnet\dotnet.exe",
        "Microsoft\u{ae} .NET",
        "Microsoft Corporation",
    )]);
    let r = dc_attrib::attribute_with(&s, &a, &source).unwrap();
    assert_eq!(own_bytes(&r, "Microsoft Corporation"), 500);
    let assignment = r
        .assignments
        .iter()
        .find(|x| x.owner == Some(owner_id(&r, "Microsoft Corporation")))
        .unwrap();
    assert_eq!(assignment.confidence, Confidence::Low);
    assert_buckets_add_up(&r);
    // Without version information nothing is guessed.
    let none = attribute(&s, &a).unwrap();
    assert_eq!(none.unattributed.on_disk, 500);
}

#[test]
fn one_matching_component_gives_the_folder_to_that_app() {
    let s = scan(&[(r"Program Files\Wire\wire.exe", 100)]);
    let a = apps(&[("Wireshark Capture Suite", "Wire Foundation", None)]);
    let source = fake_with(&[(
        r"C:\Program Files\Wire\wire.exe",
        "Wireshark Capture Suite",
        "Wire Foundation",
    )]);
    let r = dc_attrib::attribute_with(&s, &a, &source).unwrap();
    assert_eq!(own_bytes(&r, "Wireshark Capture Suite"), 100);
}

#[test]
fn an_unknown_publisher_in_a_program_does_not_create_an_owner() {
    let s = scan(&[(r"Program Files\Mystery\m.exe", 100)]);
    let source = fake_with(&[(r"C:\Program Files\Mystery\m.exe", "Mystery", "Nobody Ltd")]);
    let r = dc_attrib::attribute_with(&s, &apps(&[]), &source).unwrap();
    assert_eq!(r.unattributed.on_disk, 100);
}

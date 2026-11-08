//! Leftover scenarios on small synthetic machines.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
use common::*;
use dc_core::Risk;
use dc_leftovers::{Confidence, EvidenceKind, Kind, Standing};
use dc_protected::{FakeResolver, ProtectedList};
use dc_rules::Classifier;

const GONE: &str = r"C:\Program Files\Gone App";

#[test]
fn data_of_an_app_whose_program_folder_is_gone_is_a_strong_leftover() {
    let found = run(
        &[f(r"Users\bob\AppData\Roaming\Gone App\settings.cfg", 400)],
        &[app("Gone App", GONE, true)],
    );
    let finding = find(&found, r"C:\Users\bob\AppData\Roaming\Gone App").expect("a finding");
    assert_eq!(finding.kind, Kind::MissingApp);
    assert_eq!(finding.confidence, Confidence::High);
    assert_eq!(finding.candidate.risk, Risk::ProbablySafe);
    assert_eq!(finding.candidate.rule, "leftover_missing_app");
    assert!(finding
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::StaleRegistration));
    assert!(finding
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Untouched365));
    assert_eq!(found.offered.on_disk, 1000);
    assert_eq!(found.stale_apps.len(), 1);
    assert_eq!(found.stale_apps[0].name, "Gone App");
}

#[test]
fn an_app_that_is_still_installed_has_no_leftovers() {
    let found = run(
        &[f(r"Users\bob\AppData\Roaming\Live App\settings.cfg", 400)],
        &[app("Live App", r"C:\Program Files\Live App", false)],
    );
    assert!(found.findings.is_empty());
    assert!(found.stale_apps.is_empty());
}

#[test]
fn a_recent_change_caps_confidence_at_low_and_nothing_is_offered() {
    let found = run(
        &[f(r"Users\bob\AppData\Roaming\Gone App\settings.cfg", 5)],
        &[app("Gone App", GONE, true)],
    );
    let finding = find(&found, r"C:\Users\bob\AppData\Roaming\Gone App").expect("a finding");
    assert_eq!(finding.confidence, Confidence::Low);
    assert_eq!(finding.candidate.risk, Risk::DontTouch);
    assert_eq!(finding.candidate.reason, "low_confidence");
    assert!(finding
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::RecentlyChanged));
    assert_eq!(found.offered.on_disk, 0);
    assert_eq!(found.reported_only.on_disk, 1000);
}

#[test]
fn a_backup_named_folder_next_to_its_original_is_found() {
    let found = run(
        &[
            f(r"Users\bob\AppData\Local\Tool\a.cfg", 2),
            f(r"Users\bob\AppData\Local\Tool.old\a.cfg", 300),
        ],
        &[],
    );
    let finding = find(&found, r"C:\Users\bob\AppData\Local\Tool.old").expect("a finding");
    assert_eq!(finding.kind, Kind::BackupCopy);
    assert_eq!(finding.confidence, Confidence::Medium);
    assert_eq!(finding.candidate.risk, Risk::ProbablySafe);
    assert_eq!(finding.app.as_deref(), Some("Tool"));
    assert!(find(&found, r"C:\Users\bob\AppData\Local\Tool").is_none());
}

#[test]
fn a_backup_name_without_the_original_is_not_evidence() {
    let found = run(&[f(r"Users\bob\AppData\Local\Tool.old\a.cfg", 300)], &[]);
    assert!(found.findings.is_empty());
}

#[test]
fn a_well_known_app_that_is_not_installed_is_found_and_one_that_is_installed_is_not() {
    let files = [f(r"ProgramData\McAfee\x.dat", 400)];
    let found = run(&files, &[]);
    let finding = find(&found, r"C:\ProgramData\McAfee").expect("a finding");
    assert_eq!(finding.kind, Kind::KnownAppRemoved);
    assert_eq!(finding.candidate.risk, Risk::ProbablySafe);
    assert_eq!(finding.app.as_deref(), Some("McAfee"));

    let installed = run(
        &files,
        &[app("McAfee LiveSafe", r"C:\Program Files\McAfee", false)],
    );
    assert!(find(&installed, r"C:\ProgramData\McAfee").is_none());
}

#[test]
fn game_launcher_style_apps_are_shown_but_never_offered() {
    let found = run(&[f(r"Users\bob\AppData\Local\VALORANT\a.cfg", 400)], &[]);
    let finding = find(&found, r"C:\Users\bob\AppData\Local\VALORANT").expect("a finding");
    assert_eq!(finding.kind, Kind::KnownAppRemoved);
    assert!(finding.report_only_app);
    assert_eq!(finding.candidate.risk, Risk::DontTouch);
    assert_eq!(finding.candidate.reason, "may_hold_installed_games");
}

#[test]
fn anything_that_looks_personal_is_never_offered() {
    let found = run(
        &[
            f(r"ProgramData\McAfee\x.dat", 400),
            f(r"ProgramData\McAfee\report.docx", 400),
        ],
        &[],
    );
    let finding = find(&found, r"C:\ProgramData\McAfee").expect("a finding");
    assert_eq!(finding.candidate.risk, Risk::DontTouch);
    assert_eq!(finding.candidate.reason, "may_hold_personal_data");
    assert!(finding.personal_data.iter().any(|p| p == "documents"));
}

#[test]
fn programs_nobody_registered_are_found_but_windows_areas_are_not() {
    let found = run(
        &[
            f(r"Program Files\Mystery\m.exe", 400),
            f(r"Program Files\Windows NT\w.exe", 400),
            f(r"Program Files\Common Files\c.dll", 400),
            f(r"Program Files\Plain\readme.txt", 400),
        ],
        &[],
    );
    let finding = find(&found, r"C:\Program Files\Mystery").expect("a finding");
    assert_eq!(finding.kind, Kind::UnregisteredProgram);
    // 30 + 30 would be Medium, but "nobody registered it" alone is capped at Low.
    assert_eq!(finding.confidence, Confidence::Low);
    assert_eq!(finding.candidate.risk, Risk::DontTouch);
    assert!(find(&found, r"C:\Program Files\Windows NT").is_none());
    assert!(find(&found, r"C:\Program Files\Common Files").is_none());
    assert!(find(&found, r"C:\Program Files\Plain").is_none());

    let young = run(&[f(r"Program Files\Mystery\m.exe", 3)], &[]);
    let finding = find(&young, r"C:\Program Files\Mystery").expect("a finding");
    assert_eq!(finding.confidence, Confidence::Low);
    assert_eq!(finding.candidate.risk, Risk::DontTouch);
}

#[test]
fn programs_inside_a_registered_apps_folder_are_not_unregistered() {
    let found = run(
        &[f(r"Program Files\Mine\m.exe", 400)],
        &[app("Mine", r"C:\Program Files\Mine", false)],
    );
    assert!(found.findings.is_empty());
}

#[test]
fn a_link_inside_refuses_the_folder() {
    let found = run(
        &[
            f(r"ProgramData\McAfee\x.dat", 400),
            link(r"ProgramData\McAfee\shortcut", 400),
        ],
        &[],
    );
    assert!(find(&found, r"C:\ProgramData\McAfee").is_none());
    assert_eq!(found.refused.len(), 1);
    assert!(
        found.refused[0].because.contains("link"),
        "{:?}",
        found.refused
    );
}

#[test]
fn a_protected_folder_is_refused_and_the_protected_list_cannot_be_overridden() {
    let protected = ProtectedList::builtin().with_extra(r"C:\ProgramData\McAfee", "mine", "kept");
    let classifier = Classifier::builtin_with(protected);
    let found = run_with(
        &[f(r"ProgramData\McAfee\x.dat", 400)],
        &[],
        &classifier,
        &FakeResolver::new(),
    );
    assert!(found.findings.is_empty());
    assert!(
        found.refused[0].because.contains("protected"),
        "{:?}",
        found.refused
    );
}

#[test]
fn windows_and_microsoft_things_are_refused() {
    let found = run(
        &[
            f(r"Users\bob\AppData\Local\Microsoft.old\x.cfg", 400),
            f(r"Users\bob\AppData\Local\Microsoft\y.cfg", 2),
        ],
        &[],
    );
    assert!(found.findings.is_empty());
    assert!(found
        .refused
        .iter()
        .any(|r| r.because.contains("Windows or Microsoft")));
}

#[test]
fn an_app_registered_inside_another_apps_data_is_not_evidence() {
    // Like the Google Drive web-app entry that lives in Chrome's user data (N37).
    let found = run(
        &[f(r"Users\bob\AppData\Local\Drive Web\a.cfg", 400)],
        &[app(
            "Drive Web",
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Web Applications\Drive",
            true,
        )],
    );
    assert!(found.findings.is_empty());
    assert!(found.stale_apps.is_empty());
}

#[test]
fn updates_and_hidden_registrations_are_ignored() {
    let mut snap = snapshot(&[app("Gone App", GONE, true)]);
    if let Some(a) = snap.apps.first_mut() {
        a.flags.update_or_patch = true;
    }
    let scan = scan(&[f(r"Users\bob\AppData\Roaming\Gone App\settings.cfg", 400)]);
    let attrib = attribution(&scan, &snap);
    let found = dc_leftovers::find_leftovers(
        &scan,
        &snap,
        &attrib,
        &Classifier::builtin(),
        &FakeResolver::new(),
    )
    .unwrap();
    assert!(found.findings.is_empty());
    assert!(found.stale_apps.is_empty());
}

#[test]
fn an_unowned_folder_without_evidence_is_only_unowned() {
    let files = [
        f(r"Users\bob\AppData\Roaming\Random Thing\a.cfg", 900),
        f(r"Users\bob\Documents\Old Stuff\b.cfg", 900),
    ];
    let scan = scan(&files);
    let snap = snapshot(&[]);
    let attrib = attribution(&scan, &snap);
    let classifier = Classifier::builtin();
    let resolver = FakeResolver::new();
    let found =
        dc_leftovers::find_leftovers(&scan, &snap, &attrib, &classifier, &resolver).unwrap();
    assert!(found.findings.is_empty());
    assert!(found.refused.is_empty());
    let explained = dc_leftovers::explain(
        &scan,
        &snap,
        &attrib,
        &classifier,
        &resolver,
        r"C:\Users\bob\AppData\Roaming\Random Thing",
    )
    .unwrap();
    assert_eq!(explained.standing, Standing::NoEvidence);
    let elsewhere = dc_leftovers::explain(
        &scan,
        &snap,
        &attrib,
        &classifier,
        &resolver,
        r"C:\Users\bob\Documents\Old Stuff",
    )
    .unwrap();
    assert_eq!(elsewhere.standing, Standing::NotExamined);
    let missing =
        dc_leftovers::explain(&scan, &snap, &attrib, &classifier, &resolver, r"C:\Nowhere")
            .unwrap();
    assert_eq!(missing.standing, Standing::NotInScan);
}

#[test]
fn explain_describes_a_finding_step_by_step() {
    let files = [f(r"ProgramData\McAfee\x.dat", 400)];
    let scan = scan(&files);
    let snap = snapshot(&[]);
    let attrib = attribution(&scan, &snap);
    let classifier = Classifier::builtin();
    let resolver = FakeResolver::new();
    let e = dc_leftovers::explain(
        &scan,
        &snap,
        &attrib,
        &classifier,
        &resolver,
        r"c:\programdata\mcafee\",
    )
    .unwrap();
    assert_eq!(e.standing, Standing::Finding);
    assert!(e.notes.iter().any(|n| n.contains("evidence (+60)")));
    assert!(e
        .notes
        .iter()
        .any(|n| n.contains("final risk: ProbablySafe")));
}

#[test]
fn verify_moves_findings_behind_links_to_refused() {
    let files = [f(r"ProgramData\McAfee\x.dat", 400)];
    let scan = scan(&files);
    let snap = snapshot(&[]);
    let attrib = attribution(&scan, &snap);
    let classifier = Classifier::builtin();
    let found =
        dc_leftovers::find_leftovers(&scan, &snap, &attrib, &classifier, &FakeResolver::new())
            .unwrap();
    assert_eq!(found.findings.len(), 1);

    let linked = FakeResolver::new().link(r"C:\ProgramData\McAfee", r"C:\Windows\System32\drivers");
    let after = dc_leftovers::verify(found.clone(), &classifier, &linked);
    assert!(after.findings.is_empty());
    assert_eq!(after.refused.len(), 1);
    assert_eq!(after.offered.on_disk, 0);

    let failing = FakeResolver::new().fail(r"C:\ProgramData\McAfee");
    assert!(dc_leftovers::verify(found.clone(), &classifier, &failing)
        .findings
        .is_empty());
    let same = dc_leftovers::verify(found.clone(), &classifier, &FakeResolver::new());
    assert_eq!(same, found);
}

#[test]
fn findings_are_never_safe_deterministic_and_round_trip() {
    let files = [
        f(r"ProgramData\McAfee\x.dat", 400),
        f(r"Users\bob\AppData\Local\Tool\a.cfg", 2),
        f(r"Users\bob\AppData\Local\Tool.old\a.cfg", 300),
        f(r"Program Files\Mystery\m.exe", 400),
        f(r"Users\bob\AppData\Local\VALORANT\a.cfg", 400),
    ];
    let apps = [app("Gone App", GONE, true)];
    let a = run(&files, &apps);
    let b = run(&files, &apps);
    assert_eq!(a, b);
    assert!(dc_leftovers::never_safe(&a));
    assert!(a.findings.len() >= 4);
    // offered first, then bigger first
    let risks: Vec<Risk> = a.findings.iter().map(|f| f.candidate.risk).collect();
    let mut sorted = risks.clone();
    sorted.sort();
    assert_eq!(risks, sorted);
    let json = serde_json::to_string(&a).unwrap();
    let back: dc_leftovers::Leftovers = serde_json::from_str(&json).unwrap();
    assert_eq!(a, back);
}

#[test]
fn results_of_several_scans_merge_without_depending_on_order() {
    let a = run(&[f(r"ProgramData\McAfee\x.dat", 400)], &[]);
    let b = run(
        &[
            f(r"Users\bob\AppData\Local\Tool\a.cfg", 2),
            f(r"Users\bob\AppData\Local\Tool.old\a.cfg", 300),
        ],
        &[],
    );
    let ab = dc_leftovers::merge(vec![a.clone(), b.clone()]);
    let ba = dc_leftovers::merge(vec![b.clone(), a.clone()]);
    assert_eq!(ab, ba);
    assert_eq!(ab.findings.len(), a.findings.len() + b.findings.len());
    assert_eq!(ab.offered.on_disk, a.offered.on_disk + b.offered.on_disk);
    assert_eq!(ab.examined, a.examined + b.examined);
    // The same folder found by two scans is kept once.
    let twice = dc_leftovers::merge(vec![a.clone(), a.clone()]);
    assert_eq!(twice.findings.len(), a.findings.len());
    assert_eq!(twice.offered.on_disk, a.offered.on_disk);
}

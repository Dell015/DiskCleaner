//! Classification behaviour with the built-in pack.
#![allow(missing_docs)]

use dc_core::Risk;
use dc_protected::{FakeResolver, ProtectedList};
use dc_rules::{Classifier, NodeFacts};

const NOW: i64 = 1_800_000_000;
const DAY: i64 = 86_400;

fn file(age_days: i64) -> NodeFacts {
    NodeFacts {
        is_dir: false,
        mtime_secs: Some(NOW - age_days * DAY),
        ..NodeFacts::default()
    }
}
fn dir() -> NodeFacts {
    NodeFacts {
        is_dir: true,
        ..NodeFacts::default()
    }
}
fn risk(path: &str, facts: &NodeFacts) -> Risk {
    Classifier::builtin()
        .classify(path, facts, &FakeResolver::new(), NOW)
        .risk
}

#[test]
fn caches_are_safe_and_user_data_is_not() {
    assert_eq!(
        risk(r"C:\Users\bob\AppData\Local\D3DSCache", &dir()),
        Risk::Safe
    );
    assert_eq!(
        risk(
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\Cache_Data\f1",
            &file(0)
        ),
        Risk::Safe
    );
    assert_eq!(
        risk(
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Login Data",
            &file(400)
        ),
        Risk::DontTouch
    );
    assert_eq!(
        risk(
            r"C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cookies",
            &file(400)
        ),
        Risk::DontTouch
    );
}

#[test]
fn age_rules_need_old_enough_files() {
    let p = r"C:\Users\bob\AppData\Local\Temp\x.tmp";
    assert_eq!(risk(p, &file(8)), Risk::Safe);
    assert_eq!(
        risk(p, &file(6)),
        Risk::DontTouch,
        "too young: unclassified"
    );
    assert_eq!(
        risk(
            p,
            &NodeFacts {
                is_dir: false,
                mtime_secs: None,
                ..NodeFacts::default()
            }
        ),
        Risk::DontTouch,
        "unknown age"
    );
    assert_eq!(
        risk(r"C:\Users\bob\AppData\Local\Temp", &dir()),
        Risk::DontTouch,
        "a folder has no age"
    );
}

#[test]
fn the_highest_risk_always_wins() {
    // The generic app-cache rule says ProbablySafe, the vault rule says DontTouch.
    let p = r"C:\Users\bob\AppData\Roaming\Bitwarden\Cache\a";
    assert_eq!(risk(p, &file(100)), Risk::DontTouch);
    assert_eq!(
        risk(r"C:\Users\bob\AppData\Roaming\discord\Cache\a", &file(0)),
        Risk::ProbablySafe
    );
}

#[test]
fn protected_paths_ignore_the_rules() {
    let c = Classifier::builtin().classify(
        r"C:\Windows\Temp\a.tmp",
        &file(900),
        &FakeResolver::new(),
        NOW,
    );
    assert_eq!(c.risk, Risk::DontTouch);
    assert!(c.reasons.iter().any(|r| r.id == "windows_dir"));
    assert!(c.rules.is_empty());
}

#[test]
fn links_cloud_and_hardlinks_are_never_offered() {
    let p = r"C:\Users\bob\AppData\Local\D3DSCache\a";
    for facts in [
        NodeFacts {
            reparse: true,
            ..file(100)
        },
        NodeFacts {
            cloud: true,
            ..file(100)
        },
        NodeFacts {
            hardlink: true,
            ..file(100)
        },
    ] {
        assert_eq!(risk(p, &facts), Risk::DontTouch);
    }
}

#[test]
fn a_rule_needs_the_resolved_path_to_agree() {
    // A junction in the middle that leads somewhere else: the typed path looks like a cache,
    // the real location is not.
    let resolver =
        FakeResolver::new().link(r"C:\Users\bob\AppData\Local\D3DSCache\a", r"D:\Photos\a");
    let c = Classifier::builtin().classify(
        r"C:\Users\bob\AppData\Local\D3DSCache\a",
        &file(1),
        &resolver,
        NOW,
    );
    assert_eq!(c.risk, Risk::DontTouch);
    // And a failed resolution is refused.
    let failing = FakeResolver::new().fail(r"C:\Users\bob\AppData\Local\D3DSCache\a");
    let c = Classifier::builtin().classify(
        r"C:\Users\bob\AppData\Local\D3DSCache\a",
        &file(1),
        &failing,
        NOW,
    );
    assert_eq!(c.risk, Risk::DontTouch);
}

#[test]
fn spelling_does_not_change_the_answer() {
    for p in [
        r"c:\users\BOB\appdata\local\d3dscache\a",
        r"C:/Users/bob/AppData/Local/D3DSCache/a",
        r"C:\Users\bob\AppData\Local\.\D3DSCache\a",
        r"C:\Users\bob\AppData\Local\D3DSCache.\a",
        r"\\?\C:\Users\bob\AppData\Local\D3DSCache\a",
    ] {
        assert_eq!(risk(p, &file(1)), Risk::Safe, "{p}");
    }
}

#[test]
fn never_safe_table() {
    // Things that must come out as DontTouch with the built-in pack, whatever their age.
    for p in [
        r"C:\Users\bob\Documents\a.docx",
        r"C:\Users\bob\Desktop\a.txt",
        r"C:\Users\bob\Pictures\a.jpg",
        r"C:\Users\bob\OneDrive\a",
        r"C:\Users\bob\.ssh\id_ed25519",
        r"C:\Users\bob\.gnupg\x",
        r"D:\code\proj\.git\objects\ab",
        r"C:\Users\bob\AppData\Local\Docker\wsl\data\ext4.vhdx",
        r"E:\vms\machine.vhdx",
        r"C:\Windows\System32\kernel32.dll",
        r"C:\Windows\Temp\a",
        r"C:\Windows\SoftwareDistribution\Download\a",
        r"C:\Program Files\App\app.exe",
        r"C:\ProgramData\Microsoft\Windows Defender\a",
        r"C:\pagefile.sys",
        r"C:\$MFT",
        r"C:\$Recycle.Bin\S-1\a",
        r"X:\unknown\drive\file.bin",
        r"\\server\share\a",
        r"relative\path",
        r"",
        r"C:\Users\bob\AppData\Roaming\Exodus\a",
        r"C:\Users\bob\Saved Games\game\save.dat",
    ] {
        for age in [0, 400] {
            assert_eq!(risk(p, &file(age)), Risk::DontTouch, "{p}");
        }
        assert_eq!(risk(p, &dir()), Risk::DontTouch, "{p}");
    }
}

#[test]
fn upgrade_leftovers_are_probably_safe_never_safe() {
    assert_eq!(risk(r"C:\Windows.old", &dir()), Risk::ProbablySafe);
    assert_eq!(risk(r"C:\$WINDOWS.~BT", &dir()), Risk::ProbablySafe);
    assert_eq!(
        risk(r"C:\$WINDOWS.~BT\Sources\a.dll", &file(1)),
        Risk::ProbablySafe
    );
}

#[test]
fn an_extra_protected_entry_beats_a_safe_rule() {
    let list = ProtectedList::builtin().with_extra(
        r"C:\Users\bob\AppData\Local\D3DSCache",
        "mine",
        "kept on purpose",
    );
    let c = Classifier::builtin_with(list).classify(
        r"C:\Users\bob\AppData\Local\D3DSCache\a",
        &file(1),
        &FakeResolver::new(),
        NOW,
    );
    assert_eq!(c.risk, Risk::DontTouch);
}

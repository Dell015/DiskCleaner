//! Tricks that must not hide a protected path, and cases that must stay unprotected.

use dc_protected::{FakeResolver, ProtectedList};

fn protected(path: &str) -> bool {
    ProtectedList::builtin()
        .check(path, &FakeResolver::new())
        .protected
}

#[test]
fn plain_protected_paths() {
    for p in [
        r"C:\Windows",
        r"C:\Windows\System32",
        r"c:\windows\winsxs\x\y.dll",
        r"C:\Windows\Installer\abc.msi",
        r"D:\Windows",
        r"C:\Boot\BCD",
        r"C:\Recovery",
        r"C:\EFI\Microsoft",
        r"C:\System Volume Information",
        r"C:\$Recycle.Bin\S-1-5\x",
        r"C:\ProgramData\Microsoft\Windows Defender",
        r"C:\ProgramData\Package Cache\{guid}",
        r"C:\Config.Msi",
        r"C:\$PatchCache$\x",
        r"C:\Program Files\WindowsApps\x",
        r"C:\Program Files\Common Files\x",
        r"C:\Program Files (x86)\Common Files\x",
        r"C:\Users\bob\.ssh\id_rsa",
        r"C:\Users\bob\Documents\a.txt",
        r"C:\Users\bob\OneDrive - Corp\a",
        r"C:\Users\bob\NTUSER.DAT.LOG1",
        r"C:\Users\bob\AppData\Roaming\Microsoft\Protect\S-1",
        r"C:\Users\bob\AppData\Local\Docker\wsl\data\ext4.vhdx",
        r"E:\vms\machine.VHDX",
        r"E:\code\proj\.git\objects\ab",
        r"E:\deep\a\$Recycle.Bin\x",
        r"C:\pagefile.sys",
        r"C:\hiberfil.sys",
        r"C:\$Mft",
        r"C:\$Boot",
        r"C:\$SysReset\x",
        r"C:\$Extend\$RmMetadata",
        r"C:\",
        r"C:\Users",
        r"C:\Users\bob",
        r"C:\Users\bob\AppData\Local",
        r"C:\Program Files",
        r"C:\ProgramData",
    ] {
        assert!(protected(p), "should be protected: {p}");
    }
}

#[test]
fn spelling_tricks_do_not_hide_protection() {
    for p in [
        "c:/WINDOWS/system32/.",
        r"C:\Windows.\System32 ",
        r"C:\Windows \System32",
        r"C:\Windows\..\Windows\System32",
        r"C:\Users\bob\..\bob\.ssh",
        r"C:\\Windows\\System32",
        r"C:\Windows\.\System32",
        r"\\?\C:\Windows\System32",
        r"\??\C:\Windows",
        r"c:\WiNdOwS",
        "C:\\Windows\\System32::$DATA",
        r"C:\Windows\System32:stream",
        "  C:\\Windows  ",
        r"C:\Windows\System32\...\x",
        r"C:\Windows\CON",
        r"C:\ΩTemp\..\Windows",
        r"C:\Users\bob\Documents.",
        r"C:\Users\bob\Documents ",
        r"C:\USERS\BOB\DOCUMENTS",
    ] {
        assert!(protected(p), "should be protected: {p:?}");
    }
}

#[test]
fn unusual_or_unreadable_paths_fail_closed() {
    for p in [
        "",
        "   ",
        "relative\\path",
        "C:relative",
        r"\Windows",
        r"\\server\share\x",
        r"\\?\UNC\server\share",
        r"\\.\PhysicalDrive0",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\x",
        r"\\?\Volume{12345678-1234-1234-1234-123456789012}\x",
        r"\Device\HarddiskVolume3\Windows",
        r"C:\a\..\..\x",
        r"\\?\C:\a\..\b",
        "C:\\bad\0name",
        "C:\\a|b",
        "C:\\a?b",
        r"C:\x\nul",
        r"C:\x\COM1.txt",
        r"C:\x\LPT¹",
    ] {
        assert!(protected(p), "should fail closed: {p:?}");
    }
}

#[test]
fn links_and_short_names_use_the_resolved_path() {
    let resolver = FakeResolver::new()
        .link(r"D:\shortcut", r"C:\Windows")
        .link(r"C:\PROGRA~1\foo", r"C:\Program Files\WindowsApps\foo")
        .fail(r"D:\broken");
    let list = ProtectedList::builtin();
    let v = list.check(r"D:\shortcut", &resolver);
    assert!(v.protected && v.reason_ids().contains(&"windows_dir"));
    let v = list.check(r"C:\PROGRA~1\foo", &resolver);
    assert!(v.reason_ids().contains(&"store_apps"));
    let v = list.check(r"D:\broken", &resolver);
    assert!(v.reason_ids().contains(&"unresolved"));
    // A short name nobody could resolve is refused.
    assert!(list.check(r"C:\PROGRA~1", &FakeResolver::new()).protected);
}

#[test]
fn ordinary_places_stay_unprotected() {
    for p in [
        r"C:\Users\bob\Downloads\old-installer.exe",
        r"C:\Users\bob\Downloads\folder",
        r"C:\Users\bob\AppData\Local\Temp\x.tmp",
        r"C:\Users\bob\AppData\Local\SomeApp\Cache",
        r"C:\Users\bob\AppData\Roaming\SomeApp\logs",
        r"C:\Program Files\SomeApp\bin",
        r"C:\ProgramData\SomeVendor\cache",
        r"D:\Games\Cache",
        r"E:\projects\notes.txt",
        r"C:\Windowsx\a",
        r"C:\Users\bob\Documentsx",
        r"D:\my.git\a",
    ] {
        assert!(!protected(p), "should NOT be protected: {p}");
    }
}

#[test]
fn extras_only_add_protection() {
    let base = ProtectedList::builtin();
    let more = base.with_extra(r"D:\Keep\This", "user", "added at runtime");
    assert!(
        !base
            .check(r"D:\Keep\This\x", &FakeResolver::new())
            .protected
    );
    assert!(
        more.check(r"D:\Keep\This\x", &FakeResolver::new())
            .protected
    );
    assert!(more.check(r"C:\Windows", &FakeResolver::new()).protected);
    let bad = more.with_extra("relative", "x", "y");
    assert_eq!(bad.skipped_extras(), ["relative"]);
}

#[test]
fn upgrade_leftovers_are_not_protected_but_other_dollar_names_are() {
    for p in [
        r"C:\$WINDOWS.~BT",
        r"C:\$WINDOWS.~BT\Sources\a",
        r"D:\$Windows.~WS\x",
        r"C:\$windows.~bt.\x",
    ] {
        assert!(!protected(p), "should be left to the rules: {p}");
    }
    for p in [
        r"C:\$MFT",
        r"C:\$Recycle.Bin",
        r"C:\$Windows.~BTX",
        r"C:\$WINDOWS.~BT.old",
        r"C:\x\$WINDOWS.~BT",
    ] {
        // The last two are not the exact leftovers folder names; anything else with `$` stays closed.
        let expect = !p.ends_with(r"x\$WINDOWS.~BT");
        assert_eq!(protected(p), expect, "{p}");
    }
}

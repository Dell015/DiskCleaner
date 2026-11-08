//! Property tests: evasion transforms never un-protect, and nothing panics.

use dc_protected::{FakeResolver, ProtectedList};
use proptest::prelude::*;

const BASES: [&str; 8] = [
    r"C:\Windows\System32\drivers",
    r"C:\Users\bob\.ssh\id",
    r"C:\Users\bob\Documents\a\b",
    r"C:\ProgramData\Microsoft\x",
    r"D:\proj\.git\objects",
    r"C:\Program Files\WindowsApps\x",
    r"C:\Boot\BCD",
    r"E:\vm\disk.vhdx",
];

fn mangle(base: &str, upper: bool, dots: usize, slash: bool, prefix: u8, ads: bool) -> String {
    let mut s = base.to_owned();
    if upper {
        s = s.to_uppercase();
    }
    if slash {
        s = s.replace('\\', "/");
    }
    if dots > 0 {
        // Add `\.` segments and trailing dots/spaces before the last separator.
        s = s.replacen(['\\', '/'], &format!("\\{}\\", ".\\".repeat(dots)), 1);
        s.push_str(&" .".repeat(dots));
    }
    if ads {
        s.push_str(":stream");
    }
    match prefix {
        1 => format!(r"\\?\{}", s.replace('/', "\\")),
        2 => format!(r"\??\{}", s.replace('/', "\\")),
        _ => s,
    }
}

proptest! {
    #[test]
    fn evasion_never_unprotects(
        idx in 0usize..BASES.len(),
        upper in any::<bool>(),
        dots in 0usize..3,
        slash in any::<bool>(),
        prefix in 0u8..3,
        ads in any::<bool>(),
    ) {
        let list = ProtectedList::builtin();
        let text = mangle(BASES[idx], upper, dots, slash, prefix, ads);
        let v = list.check(&text, &FakeResolver::new());
        prop_assert!(v.protected, "unprotected: {text:?}");
    }

    #[test]
    fn never_panics(text in ".{0,80}") {
        let list = ProtectedList::builtin();
        let _ = list.check(&text, &FakeResolver::new());
    }

    #[test]
    fn never_panics_on_pathy_text(text in r"[A-Za-z:\\/.~$ *?]{0,60}") {
        let list = ProtectedList::builtin();
        let _ = list.check(&text, &FakeResolver::new());
    }
}

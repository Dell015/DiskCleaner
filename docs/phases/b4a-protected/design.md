# Phase B4a: Protected paths - Design

Status: **draft for review**. Requirements: `requirements.md`.

## 1. Crate layout (`dc-protected`, safe Rust only)
```
src/
  lib.rs         re-exports, crate docs (the "NotProtected != safe" warning)
  normalize.rs   lexical normalization -> NormalPath (drive + components) or an invalid-path reason
  pattern.rs     component patterns with * (whole-component and inside-component)
  list.rs        the compiled-in entries (data) + ProtectedList (with_extra)
  resolve.rs     PathResolver trait, FsResolver, FakeResolver
  check.rs       check(): normalize -> match -> resolve -> match again -> merge reasons
  verdict.rs     Verdict, Reason
```

## 2. Types
```rust
pub struct Verdict { pub protected: bool, pub reasons: Vec<Reason> }
pub struct Reason { pub id: &'static str /* or String for extras */, pub description: String }

enum Scope { Subtree, SelfOnly }
struct Entry { pattern: Pattern, scope: Scope, reason: Reason, any_depth: bool }

pub trait PathResolver {
    fn resolve(&self, path: &str) -> Resolution;
}
pub enum Resolution { Resolved(String), NotFound, Failed(String) }
```
`Verdict` has no `allowed`/`safe` field; there is nothing to construct a positive result from.

## 3. Normalization (`normalize.rs`)
Input text -> `Result<NormalPath, InvalidReason>`; `NormalPath { drive: char (lowercase), parts: Vec<String> }` with every part lowercased and cleaned.
Steps, in order:
1. Trim whitespace; empty -> `invalid_path`.
2. Detect prefixes: `\\.\` -> `network_or_device`; `\\?\UNC\` or `\\x` (UNC) -> `network_or_device`; `\\?\` and `\??\` stripped.
3. Replace `/` with `\`; the path must then start with `X:\` (absolute drive path); otherwise `invalid_path`.
4. Split on `\`, drop empty and `.`; for `..` pop (if nothing to pop -> `invalid_path`).
5. Per component: strip an NTFS stream (`:` and everything after, except nothing is kept if it starts with `:`), trim trailing dots and spaces, lowercase (full Unicode lowercase). A component that becomes empty after trimming is dropped like `.`. Reserved names (`con`, `prn`, `aux`, `nul`, `com1-9`, `lpt1-9`, with or without extension) -> `invalid_path`.
6. Short-name look: a component matching `^[^~]{1,6}~[0-9]+(\.[^.]{0,3})?$` is flagged. It is only fatal (`unresolved`) if the resolver did not change it (see 5).

Trailing-dot/space handling is applied to the input path before matching **and** the resolver is given the original text, so a disagreement between our normalization and Windows' cannot hide a protected path: the resolved path from Windows is matched too.

## 4. Patterns (`pattern.rs`)
A pattern is a list of segments. A segment is a literal (lowercase), `*` (any one component), or a glob with `*` inside (`ntuser.dat*`, `onedrive*`). The first segment (`*:`) is the drive. Matching:
- `SelfOnly`: `parts.len() == segments.len()` and every segment matches.
- `Subtree`: `parts.len() >= segments.len()` and the first `segments.len()` match.
- `any_depth` entries (`.git`, `system volume information`, `$recycle.bin`): match if any single component equals the literal; this is a subtree rule by nature.
The glob matcher is a small, iterative function with no recursion and no regex dependency (inputs are short; tested including pathological `*` patterns).

## 5. Check algorithm (`check.rs`)
```
fn check(list, path, resolver) -> Verdict:
  reasons = []
  match normalize(path):
     Err(r) -> return protected(r)
     Ok(n)  -> reasons += list.matches(n)
  match resolver.resolve(path):
     Resolved(real) -> match normalize(real):
                          Err(r)  -> reasons += r        // fail closed
                          Ok(rn)  -> reasons += list.matches(rn)
                                     if short-name flag in n and rn still has it -> reasons += unresolved
     NotFound       -> if n has a short-name flag -> reasons += unresolved
     Failed(_)      -> reasons += unresolved
  dedupe reasons by id (stable order)
  Verdict { protected: !reasons.is_empty(), reasons }
```
Notes:
- "Either path matches" is the escalate-only rule applied to resolution.
- A nonexistent path is judged on its normalized text only (it cannot be a link), unless it contains a short-name look.
- Extra runtime entries are plain subtree entries from `with_extra`; they are matched in the same loop. The caller passes a normalized-or-raw path and the code normalizes it the same way.

## 6. Resolution (`resolve.rs`)
`FsResolver::resolve(path)`:
1. Walk up from the full path until `std::fs::canonicalize` succeeds (using `symlink_metadata` only to decide existence, never to read content).
2. Canonicalize that ancestor (Windows: strips the `\\?\` prefix text afterwards; the result is long names with links resolved).
3. Append the not-yet-existing tail names.
4. Errors other than `NotFound` -> `Failed`. A path with no existing ancestor at all -> `NotFound`.
`FakeResolver` holds a map `input-key -> output` plus a set of failing inputs, for tests of junctions, 8.3 names and failures on every platform.

## 7. The built-in list as data
Entries live in one `const`/`static` table in `list.rs`: `(pattern text, scope, any_depth, reason id, description)`. The table is also what `dc protected --list` prints, so the documentation cannot drift from the code. A test builds each pattern from its text and checks it parses.

## 8. Dynamic additions
`ProtectedList::with_extra(path, reason) -> ProtectedList` returns a new list containing the old entries plus a `Subtree` entry for the (normalized) path. If the path cannot be normalized it is added as a literal text entry that matches nothing but is reported in `skipped_extras()` so the caller can warn. `dc-winmeta` later supplies known folders (system root, profile, ProgramData) as extras.

## 9. CLI
`dc protected <path>...` builds `ProtectedList::builtin()`, adds known-folder extras on Windows, checks each path with `FsResolver` and prints:
```
PROTECTED     C:\Windows\System32          windows_dir
not protected D:\Cache\x                   (not protected does not mean safe)
```
`--list` prints the table from section 7. Both are read-only.

## 10. Risks
| Risk | Mitigation |
|---|---|
| Windows path rules are subtler than our normalization (for example Unicode case, reserved names with trailing spaces) | Match both the normalized input and the OS-resolved path; fail closed on anything odd; large evasion table; QA focus |
| Over-protection makes the cleaner useless | Pinned "must stay NotProtected" cases; the list is small and explicit |
| A new Windows location holds system files (not in our list) | Default-deny already applies to everything unknown; the list only adds hard guarantees |
| `canonicalize` behaves differently on Linux (tests) | Resolver is a trait; real-resolver tests are tagged by platform; logic tests use `FakeResolver` |
| Hardlinks into protected files | Out of scope; noted in `NOTES.md` |

## 11. Steps
1. `normalize`, `pattern`, `verdict` + their tests.
2. `list`, `check`, `FakeResolver` + the evasion table and property tests.
3. `FsResolver` + real-disk tests.
4. CLI `dc protected` + tests; docs, CHANGELOG, DECISIONS, NOTES.

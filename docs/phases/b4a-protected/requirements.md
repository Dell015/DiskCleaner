# Phase B4a: Protected paths - Requirements

Source: `ROADMAP.md` Phase B4 (first half), `SAFETY.md` rules 3, 5, 7, 9. Status: **draft for review**.

**Goal:** a compiled-in engine that answers one question for any path: *is it protected?* The answer is `Protected` (with reasons) or `NotProtected`. **`NotProtected` never means "safe".** Only the rules engine (B4b) can ever say Safe, and unknown always stays `DontTouch`. This crate can only add caution.

## Decisions already made (with you)
| # | Decision |
|---|---|
| 1 | `Windows` is protected on every drive (`*:\Windows`, whole subtree). `Windows.old` is left to rules. |
| 2 | `Program Files` and `Program Files (x86)` are protected as folders themselves only, not their contents. |
| 3 | Documents, Desktop, Pictures, Music, Videos and OneDrive are protected with everything inside, forever. Downloads is not. |
| 4 | UNC/network paths and device paths are protected outright (v1 never scans or cleans them). |
| 5 | Any `.git` folder anywhere is protected with everything inside. |
| 6 | `System Volume Information`, `$Recycle.Bin`, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys` (and other `$`-named system files at a drive root) are protected on every drive. |
| 7 | Fail closed: a path that cannot be resolved properly is protected. |
| 8 | Keys and credentials are protected: `.ssh`, `.gnupg`, `.aws`, `.azure`, `.kube`, and Windows credential stores (`Microsoft\Credentials`, `Microsoft\Protect`, `Vault`). |
| 9 | Extra protected paths can be added at runtime. They can only add protection, never remove it. |
| 10 | `dc protected <path>` and `dc protected --list`. |

## R1. API (`dc-protected`)
1. `ProtectedList::builtin()`: the compiled-in list. `with_extra(path, reason)`: adds a path (subtree) at runtime; returns a new list. There is no way to remove or weaken an entry.
2. `check(&self, path: &str, resolver: &dyn PathResolver) -> Verdict`.
3. `Verdict { protected: bool, reasons: Vec<Reason> }`; `Reason { id, description }`. `protected == true` implies at least one reason. Reason ids are stable strings (`windows_dir`, `git_dir`, `credentials`, `unresolved`, ...).
4. Two types of entry: **subtree** (the folder and everything below it) and **self only** (just that folder or file).
5. The crate has no function that returns "allowed", "safe" or similar. Callers that do not understand a `NotProtected` result must still treat the path as `DontTouch`.

## R2. Normalization (pure text, no disk access)
A path is reduced to a comparison form before matching:
1. Case-insensitive.
2. `/` and `\` are the same; repeated separators collapse; `.` components are dropped; `..` is resolved lexically, and a path that climbs above its root is `Protected` (`invalid_path`).
3. Prefixes `\\?\`, `\??\` are removed. `\\?\UNC\...`, `\\server\share`, and `\\.\` device paths are `Protected` (`network_or_device`).
4. Trailing dots and spaces on a component are ignored (`Windows.` and `Windows ` are `Windows`).
5. NTFS stream suffixes (`name:stream`, `name::$DATA`) are removed from a component.
6. Reserved device names (`CON`, `NUL`, `COM1`, ...) as a component make the path `Protected` (`invalid_path`).
7. A component that looks like a short (8.3) name (`ABCDEF~1`) that is not expanded by the resolver makes the path `Protected` (`unresolved`).
8. Empty or relative paths are `Protected` (`invalid_path`): only absolute drive paths can be judged as `NotProtected`.

## R3. Resolution (junctions, symlinks, short names)
1. `PathResolver` is a trait with one job: return the real, fully resolved path for an input (following links, expanding short names), or say that it does not exist, or fail.
2. `FsResolver` (real): uses `std::fs::canonicalize` for the deepest existing ancestor and appends the remaining names. No `unsafe`, no writes.
3. **Both** the normalized input path and the resolved path are checked; a path is protected if **either** is. A junction that points into `C:\Windows\System32` is therefore protected, and so is a path that reaches a protected folder through a link.
4. If resolution fails with anything other than "does not exist", the path is `Protected` (`unresolved`).
5. A `FakeResolver` (in-memory) is provided for tests, so all of this is tested on every platform.

## R4. The built-in list
Entries are written as component patterns (case-insensitive; `*` matches one component or, inside a component, any text):
- Subtree: `*:\Windows`, `*:\Boot`, `*:\Recovery`, `*:\EFI`, `*:\System Volume Information`, `*:\$Recycle.Bin`, `*:\ProgramData\Microsoft`, `*:\Users\*\AppData\Roaming\Microsoft\{Credentials,Protect,Vault}`, `*:\Users\*\AppData\Local\Microsoft\{Credentials,Vault}`, `*:\Users\*\{Documents,Desktop,Pictures,Music,Videos}`, `*:\Users\*\OneDrive*`, `*:\Users\*\{.ssh,.gnupg,.aws,.azure,.kube}`, `*:\Users\*\NTUSER.DAT*`.
- Subtree, any depth: any `.git` folder; any `System Volume Information` or `$Recycle.Bin`.
- Self only: `*:\`, `*:\Program Files`, `*:\Program Files (x86)`, `*:\ProgramData`, `*:\Users`, `*:\Users\*` (a profile root), `*:\Users\*\AppData`, `*:\Users\*\AppData\{Local,Roaming,LocalLow}`.
- Files at a drive root: `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `bootmgr`, `BOOTNXT`, and any name starting with `$` (`$MFT`, `$Boot`, ...).
- Runtime additions (`with_extra`), for example the system root, the profile and known folders reported by Windows (from `dc-winmeta`), so non-standard locations are covered too.

## R5. CLI (`dc-cli`)
- `dc protected <path>...`: prints for each path the verdict and reason ids; see exit codes below.
- `dc protected --list`: prints the compiled-in list (pattern, kind, reason id).
- Exit codes: 0 = all checked paths were processed (the verdict is in the output, not in the exit code); 1 = error.
- The command only reads (it may look up a path on disk to resolve links); it never writes.

## R6. Safety requirements
1. `dc-protected` stays `#![forbid(unsafe_code)]` and denies `unwrap`, `expect` and `panic!` outside tests (already configured).
2. No function in the crate deletes, moves or writes anything (tripwire test like the other crates).
3. Property: a protected path cannot become `NotProtected` by changing case, separators, adding trailing dots or spaces, doubling separators, inserting `.` components, adding `\..\` detours, adding a stream suffix, or prefixing `\\?\`.
4. Property: no input makes any function panic.
5. Over-protection is acceptable; under-protection is a bug. Tests also pin a small set of paths that must stay `NotProtected` (for example a folder inside `Downloads` and an app cache folder), so the list does not silently grow to cover everything.

## R7. Tests
1. A table of about 100 evasion cases (case, slashes, `..`, trailing dot/space, streams, `\\?\`, `\??\`, UNC, device paths, 8.3 names, junction into System32 via the fake resolver, reserved names, relative paths, empty path, other drives, deep paths).
2. One test per built-in entry (a path that matches it, and a near-miss that does not).
3. Property tests for R6.3 and R6.4.
4. `FsResolver` against a temp folder: a real symlink/junction into a temp "protected" folder (symlink on Unix; on Windows only when the privilege exists, otherwise skipped with a reason).
5. CLI tests for `dc protected` and `--list`.
6. Source-grep tripwire (no destructive calls in `dc-protected`).

## R8. Non-goals
Rules and classification (B4b), user config files, hardlink analysis (a hardlink to a protected file elsewhere), protecting running programs' files, any deletion.

## R9. Definition of done
fmt, clippy `-D warnings`, `cargo test --workspace` on Windows and Linux; CHANGELOG, `DECISIONS.md` and `NOTES.md` updated; summary lists what changed, what was not done and the safety decisions.

## Acceptance check (you)
1. `dc protected --list` and read every entry.
2. `dc protected C:\Windows\System32 c:/WINDOWS/system32/. "C:\Windows.\System32 " C:\PROGRA~1 D:\Windows $env:USERPROFILE\Documents $env:USERPROFILE\.ssh C:\Users\Public` and check each verdict.
3. Create a junction to `C:\Windows` inside a temp folder and check that the junction path is protected.
4. Try five paths you consider safe to clean (for example a browser cache folder): they must say `NotProtected`, and you must still not read that as "safe".

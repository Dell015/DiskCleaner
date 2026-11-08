# Phase B2: Windows metadata - Requirements

Source: `ROADMAP.md` Phase B2, `CLAUDE.md`, `SAFETY.md`. Status: **draft for review**.

**Goal:** learn, read-only, which programs are installed and where Windows keeps well-known folders, so later phases can say "this folder belongs to this app".
**Everything here is read-only and only labels things. B2 never decides what is safe to delete.**

## Decisions already made (with you)
| # | Decision |
|---|---|
| 1 | Read per-user (HKCU) and machine-wide (HKLM, 64-bit and 32-bit views) installed apps. |
| 2 | Classic installed programs only. Microsoft Store (MSIX/UWP) apps are a later phase. Unknown owner always means "don't touch". |
| 3 | Read Start Menu shortcuts (.lnk) to find where apps really live. |
| 4 | Read executable version info (company, product, description). |
| 5 | Check digital signatures: **opt-in, offline only, only for executables already linked to an app**. Built last (step B2c) so it cannot hold up the rest. |
| 6 | Both `dc apps` and `dc scan --apps` (see R6 for what the second one does in B2). |
| 7 | Flag apps whose install folder is missing as "stale registration". Report only. |
| 8 | Write a JSON snapshot of the app list. |
| 9 | Registry/Windows code is verified on your machine and in Windows CI. Logic is tested on every platform with a fake source. |

## R1. Source abstraction (`dc-winmeta`)
1. Trait `SystemInfoSource` with: known folders, uninstall entries, shortcuts, file version info, signature status.
2. A `FakeSource` (portable, in-memory) used by all logic tests. A `WindowsSource` (Windows only) is the real one.
3. All model types (`InstalledApp`, `KnownFolder`, `Shortcut`, `VersionInfo`, `SignatureStatus`, `AppSnapshot`) compile and serialize on every platform.

## R2. Known folders
1. Resolve with the Windows known-folder API (not environment variables alone): LocalAppData, RoamingAppData, LocalAppDataLow, ProgramData, ProgramFiles, ProgramFilesX86, Windows, System, Temp, Downloads, Desktop, Documents, UserProfile, StartMenu, CommonStartMenu.
2. A folder that cannot be resolved is reported as missing, never guessed.

## R3. Installed apps from the registry
1. Read the `Uninstall` keys under HKLM (64-bit view), HKLM (WOW6432Node), and HKCU. Each source is recorded on the entry (`machine64`, `machine32`, `user`).
2. Fields: registry key name, DisplayName, Publisher, DisplayVersion, InstallLocation, DisplayIcon, UninstallString (stored as text only), EstimatedSize (KB), InstallDate, SystemComponent, WindowsInstaller, ParentKeyName / ReleaseType (to mark updates and patches).
3. Entries without a DisplayName are kept but marked `hidden_entry` (they are usually components). `SystemComponent=1` is marked, not dropped.
4. Install folder is derived in this order: InstallLocation, folder of DisplayIcon's exe, folder of the exe in UninstallString. The origin of the path is recorded (`install_location`, `display_icon`, `uninstall_string`) so a guess is never shown as a fact.
5. Registry values are **untrusted input**: quotes, `,0` icon suffixes, arguments, environment variables (`%ProgramFiles%`) and garbage are parsed by pure, tested functions. Nothing from the registry is ever executed or opened as a file for writing.
6. Entries with the same display name in several sources are kept, not merged (merging is B3).

## R4. Shortcuts
1. Read `.lnk` files from the Start Menu (user and common) and Desktop. Resolve the target, arguments and working directory.
2. The `.lnk` binary format is parsed by safe Rust (no COM, no shell execution). A broken or unknown shortcut is skipped and reported, never an error that stops the run.
3. A shortcut is attached to an app only when its target lies inside that app's install folder.

## R5. Version info and signatures
1. Version info (CompanyName, ProductName, FileDescription, FileVersion) read for each app's main executable. Missing info is normal and not an error.
2. Signature status per executable: `signed` (with signer name), `unsigned`, `invalid`, `unknown` (could not check). Offline only: no revocation lookups, no network. Files signed only through a Windows catalog may be reported `unknown` in B2 (documented limitation).
3. Signature checking runs only with `--verify-signatures`, and only on executables already linked to an app.
4. Results go into the snapshot so they are not recomputed.

## R6. CLI (`dc-cli`)
- `dc apps [--out apps.json] [--verify-signatures] [--include-hidden]`: prints a table (name, publisher, version, install folder, source, flags) and writes the snapshot when `--out` is given. The snapshot is created with `create_new` and never overwrites.
- `dc scan <path> --out scan.json --apps [apps.json]`: after the scan, labels each directory that equals or lies inside an app's install folder with that app's name (a plain path-prefix match). Without a snapshot argument it reads the registry itself. The full app -> executables -> folders -> files graph is **Phase B3**, not here.
- `dc apps` flags: `stale_registration` (install folder missing), `no_install_folder`, `system_component`, `update_or_patch`.
- Exit codes as before: 0 ok, 1 error, 2 partial (some sources unreadable).

## R7. Snapshot
`AppSnapshot` JSON with `schema_version`, machine-independent field names, no secrets. Excludes environment-specific values that are not needed (no user name beyond what appears in paths). Same snapshot can be fed back to `dc scan --apps` and to tests.

## R8. Safety requirements
1. Registry opened with read-only rights only. A source-grep test fails the build if `dc-winmeta` mentions registry write/create/delete functions or process-launching calls.
2. `unsafe` stays inside `dc-winmeta`, each block with a `// SAFETY:` comment; every other crate stays `forbid(unsafe_code)`.
3. Files are opened only to read metadata, version resources or signatures. Executables are never run. Uninstall strings are never executed.
4. Any failure of one source (access denied on a key, broken shortcut) is collected and the run continues.

## R9. Tests
1. Pure parsers (icon path, uninstall string, environment expansion, install-folder derivation, `.lnk` parsing from hand-built bytes) on every platform, including property tests that arbitrary text never panics.
2. `FakeSource` end-to-end: snapshot from fake data matches an `insta` snapshot; stale/hidden/update flags are right.
3. Windows-only tests: known folders exist and are absolute; the registry source returns at least one app; reading does not change the registry (key last-write times of a few sampled keys unchanged).
4. Scan labeling test with generated fixtures and a fake snapshot.
5. CI stays green on Windows and Linux.

## R10. Non-goals
Microsoft Store apps, the full attribution graph, risk classification, leftover detection beyond the stale-registration flag, services/scheduled tasks/startup items, any UI, any deletion.

## R11. Definition of done
fmt, clippy `-D warnings`, `cargo test --workspace` on Windows and Linux; CHANGELOG updated; summary lists what changed, what was not done and the safety decisions.

## Acceptance check (you)
1. `dc apps` lists your real programs; compare with Settings > Apps > Installed apps.
2. Spot check five apps: name, publisher, install folder.
3. `dc apps --verify-signatures` on a few apps: Microsoft/Google apps show `signed`.
4. A scan of one app folder with `--apps` labels it with the right app.

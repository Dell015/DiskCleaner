# Findings and follow-ups

Things we learned, surprises, and items to look into later. Unlike `DECISIONS.md` (what we chose),
this file records **what we noticed** and **what still needs attention**. Add an entry whenever
something unexpected shows up. Mark an item `done` (with where) instead of deleting it.

Status: `open` = needs action, `watch` = keep an eye on it, `done` = resolved.

## Open items
| ID | Status | Found in | What | Where to deal with it |
|---|---|---|---|---|
| N1 | open | B2b real run | **System and important files are not protected by code yet.** Nothing can delete anything today (D2), but the compiled-in protected-path engine (`dc-protected`) is still a stub. | Phase B4, before any rule or advisor output. Do not let any phase suggest an action before it exists. |
| N2 | open | B2b real run | Vendor folders that hold several apps (for example one `AMD` folder with many app folders) are unlabelled; they are the biggest folders but have no single owner. | B3: roll up by publisher/parent folder. |
| N3 | open | B2b real run | Apps installed through MSI often record no install folder (database servers, .NET runtimes, redistributables), so their folders stay unlabelled even though they are large. | B3: match by publisher, executables, version info and signature. |
| N4 | open | B2b real run | Subfolders below an app folder (for example the `root` and versioned folders under an Office folder) are not labelled; the biggest sizes sit below the labelled folder. | B3: attribute whole subtrees to the owning app; show app totals. |
| N5 | open | B1/B2b | A scan of a protected tree (Program Files) returns exit code 2 because some entries are unreadable (access denied). This is correct but noisy for normal use. | B4/B10: classify "unreadable because protected" separately from real errors; consider a quieter exit code. B7 (elevated helper) reads more. |
| N6 | open | B2a | The Windows-only tests (`tests/system_windows.rs`) depend on the machine: they expect installed programs, Start Menu shortcuts and an app with a main program. A bare CI runner may have too few, and they could fail there. | Watch the first Windows CI run; if it fails, relax those assertions or skip with a reason, never silently. |
| N7 | open | B2a | The build container (Rust 1.97, Linux) and the user's machine (Rust 1.99, Windows) differ: newer Clippy lints appear only on the user's machine, and Windows-only code cannot be compiled in the container (two small compile/lint errors were found that way). | Add `rust-toolchain.toml` and pin the same toolchain in CI; keep Windows modules thin; rely on Windows CI as the real gate. |
| N8 | open | B2a/B2b | Snapshot and scan files list everything installed or present on the machine. They are private data. | Keep `apps*.json`, `pf.json`, `proj.json`, `scan*.json` in `.gitignore` (verify); never use real snapshots as test fixtures; consider a redaction option for sharing. |
| N9 | open | B1 | `dc` has no Ctrl+C handler; only the library cancel token exists. A long scan cannot be cancelled cleanly from the terminal. | Before B6/B10; also needed for any future UI. |
| N10 | open | B0 | GitHub Actions are not pinned to commit SHAs yet. | Before the first public release. |
| N11 | open | B1 | Cloud placeholder files (OneDrive and similar) are flagged and not hydrated, but this has only been tested on generated fixtures, not on a real synced folder. | Test on a real OneDrive folder in B2/B3 acceptance. |
| N12 | open | docs | The beginner Claude Doc is out of date after B1/B2; an unused chart widget is still stored in it. | Refresh after B3. |
| N13 | open | B2c | Catalog-signed files (most Windows system files) cannot be told apart from unsigned files by the embedded-signature check. | Add catalog lookup (`CryptCATAdmin*`) before signatures are used to judge system files (B4/B5). |
| N14 | watch (first real run: 69 signed, 4 no-embedded, 0 invalid, 0 unknown; names looked right) | B2c | The signer-name extraction walks several Windows structures (`unsafe`); it could only be tested on the real machine. | QA reviewer: audit `windows/signature.rs`; check the signer shown for ~5 known apps. |
| N15 | open | B2c real run | Only 73 of 430 shown apps got a signature check, because most apps have no `main_exe` (no usable icon path or shortcut). MSI components and apps without shortcuts are skipped. | B3: find an app's executables by looking inside its install folder (exe whose name or version info matches the app), not only via icon and shortcuts. |
| N16 | open | B2c real run | The signer often differs from the registry publisher, and that is normal: Audacity is signed by its parent company, Git by an individual maintainer, Scratch by a university, a pen tablet by a related manufacturer, Java by a subsidiary. Names can be non-Latin (Chinese signer names appeared and displayed correctly). | B3/B5: never treat publisher != signer as suspicious; keep an alias/parent-company mapping if we want to group by owner. |

## Watch list
| ID | Status | What |
|---|---|---|
| N17 | open | B4a research | NTFS can mark a folder case-sensitive (WSL), so `Windows` and `windows` could be different folders. We compare case-insensitively, which only over-protects. Fine; never switch to case-sensitive matching. | none |
| N18 | open | B4a research | Hardlinks: files in WinSxS are hardlinked elsewhere; deleting the other name frees nothing and the scanner counts once. Protection is by path, so a hardlink outside `\Windows` to a system file is not caught. | B5: treat link_count>1 files as DontTouch unless proven otherwise. |
| N19 | open | B4a research | Browser profiles ("User Data"), password managers, crypto wallets, game saves, databases live under AppData. Only credentials and virtual disks are hard-protected; the rest is left to default-deny and B4b rules (only caches inside a profile may ever be Safe). | B4b |
| N20 | watch | Size on disk matches Explorer exactly on a normal volume (verified). Volumes mounted on folders, network paths and non-NTFS volumes may use the fallback 512-byte rule or the wrong cluster size (D10). |
| N21 | watch | On 32-bit Windows the 64-bit and 32-bit registry views are the same; entries could appear twice. Rare; not handled. |
| N22 | watch | Registry apps that are really browser web apps (installed from a browser) can have an install folder inside a browser profile. They are labelled as apps; B3 should recognise and treat them as part of the browser. |
| N23 | watch | Hardlinks are common in system folders (87 extra paths in one Program Files scan). Counted once (D5); important for any future "space freed" estimate: deleting one name frees nothing. |
| N24 | watch | Many registry entries are duplicates of one app (MSI components, 32/64-bit pairs, side-by-side versions). They are kept, not merged (D28); B3 must group them. |
| N25 | watch | Version info and registry text are written by the program's author and can be wrong or faked. They are labels only; trust decisions need signatures (B2c) and the protected list (B4). |
| N26 | watch | Apps recorded in the registry whose folder is gone ("stale registrations") are common on a lived-in PC (apps deleted by hand). Good leftover evidence for B5, but removal of registry data is out of scope until B9 and needs its own backup/restore design. |

## Questions to put to the final QA reviewer
- Does every code path that reads untrusted text (registry, shortcut bytes, version strings) avoid panics and path tricks?
- Is there any place where an unknown owner or missing data could lead to a "safe" classification?
- Are all Windows-only `unsafe` blocks justified and closed correctly (registry handles, COM-allocated strings, version blocks)?
| N30 | partly done | B3 slice 1 | Executable discovery inside install folders (the N15 fix) and its `ExeInfoSource` are not built yet. `main_exe` still comes only from icons and shortcuts. | B3 slice 2 |
| N31 | partly done | B3 slice 1 | MSI apps without an install folder are not matched yet (no `publisher_match` rule). | B3 slice 2 |
| N32 | open | B3 slice 1 | Apps whose name is under 4 letters (Git, Go, 7-Zip -> "zip") get no data-folder name matches. Their install folder is still labelled. | Revisit with publisher agreement |
| N33 | open | B3 slice 1 | A folder named like a publisher becomes a vendor owner even when it holds only one app; the design said two or more. Harmless but may add noise. | Review on the real run |
| N34 | open | B3 slice 1 | Name matching compares every candidate folder with every app (folders x apps). Fine for hundreds of each; measure on a big scan. | Benchmark before B5 |
| N35 | open | B3 real run | On `C:\Program Files` (23.55 GiB): 47 app folders owned; 5.25 GiB had no owner, mostly `dotnet` (2.2 GiB, MSI components with no folder: N31), `WSL` (Store package), `ReasonLabs`, `Common Files`. Slice 2 should cover dotnet via MSI/publisher info and add a small, reviewed table of well-known shared folders. | B3 slice 2 |
| N36 | open | B3 real run | `LGHUB.old` (206 MiB) sits next to Logitech G HUB with no owner: a typical leftover. | B5 leftovers: `*.old` siblings of owned folders |
| N37 | open | B3 real run | An app registration can point inside another app's data (Google Drive's web-app entry lives in Chrome's `User Data`). The folder is attributed to that entry. Correct by evidence, but B5 must not treat it as a normal app. | B5 |
| N38 | open | B3 slice 2 | Programs are probed only in direct children of Program Files, Common Files and ProgramData, and in folders of apps without a known main program. AppData programs (Discord, Figma) rely on the registry install folder. Signatures of discovered programs are not checked yet. | Decide after the real run |
| N39 | fixed | B3 slice 2 run | fixture-gen tests named their folders by process id only; Windows reuses ids, so a leftover folder from an old run made 4 tests fail with OutDirNotEmpty. Names now include the clock. Old `target\tmp\fg-*` folders can be deleted. | none |
| N40 | open | B3 slice 2 run | After slice 2, only 141 MiB of Program Files has no owner (was 5.25 GiB), but the big gain went to the publisher "Microsoft" (2.87 GiB, low confidence). Review with `--explain` that dotnet and Reference Assemblies belong there; consider a separate ".NET" group owner. | Review |


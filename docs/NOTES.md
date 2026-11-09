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
| N41 | open | B4b | `classify` walks every node and runs the protected check plus about 45 rules on each; fine on test scans, not yet measured on a whole-drive scan (millions of nodes). Possible speed-up: skip the descent into folders that are protected as a whole. | Measure on the real machine |
| N42 | open | B4b | No rule for `__pycache__`, `node_modules\.cache`, `target` folders of build tools (need a pattern for "anywhere under"). | Later: any-depth rules with extra guards |
| N43 | open | B4b | Rules use only paths and ages. A cache is not checked for being in use right now (running browser). | B9 quarantine must check open files |
| N44 | open | B4b | The built-in pack is a first, conservative draft written without a security review. Every Safe/ProbablySafe rule must be reviewed in the final QA pass. | Final QA |
| N45 | open | B4b | `cargo deny` was not run on the new `toml` dependency tree in my environment. | Run `cargo deny check` / watch CI |


## N46 — classify speed (measured on the real profile scan)
- `cargo run` without `--release` is a debug build; `classify` on 1.46 M entries took 4m38s that way. Always use `cargo run --release -q -p dc-cli -- ...` for big scans.
- A synthetic 1 M-node scan takes about 6 s in a release build (`cargo test -p dc-rules --release --test perf -- --ignored --nocapture`). No algorithm change needed for now.
- The "refused" list now shows only the outermost refused folder per rule (the uv cache gave 3133 lines before).

## N47 — real-profile review of B4b (first run)
- Offered: Safe 4.62 GiB (44 items), ProbablySafe 9.30 GiB (768 items). Biggest: .nuget\packages, npm-cache, .gradle\caches, Unity\cache, Brave/Edge caches, Riot/Discord/Code caches. Nothing unexpected.
- uv cache: archive folders are hardlinked, correctly refused; only the non-linked files are offered.

## N48 — dev build speed
- Added `[profile.dev] opt-level = 1` and `[profile.dev.package."*"] opt-level = 3` to the workspace Cargo.toml. Overflow checks and debug assertions stay on.
- Synthetic 1 M-node scan: debug 34.7 s -> 8.7 s (release 6.7 s). End users get release builds anyway.
- Trade-off: the first build after this change recompiles everything once; debugger stepping is a little less exact.

## N49 — owner lookup in `classify` (real run: 102 s -> expected ~20 s)
- Cause: `dc_attrib::explain` indexed the whole scan (1.46 M entries) once per listed item. Added `dc_attrib::explain_many`, which indexes once. `classify` now uses it. Without `--apps`, a 1 M-node scan takes about 12 s in the dev profile.
- Still to do if needed: `scan.tree()` is built three times (candidates, attribution, owner lookup); a shared tree would save a few more seconds.

## N50 — protected-check speed
- `glob_match` allocated two vectors per call and was the main cost of `ProtectedList::check` (7.7 µs -> 2.4 µs per call). Now allocation-free and byte-based, with a test against the old matcher (D68). On a 1 M-node scan, candidate search went from 9.7 s to 2.8 s; loading the 253 MB JSON (2.7 s) is now the largest part.
- Possible later: faster scan-file format than JSON (binary), a shared tree for the three phases.

## N51 — `dc classify --timing`
- Prints one line per step (read file, protected list, candidates, disk check, attribution, owner lookup) to standard error, so the normal output is unchanged. Meant for finding slow steps on real scans; paste it when something feels slow.

## N52 — parallel candidate search
- Real run (1.46 M entries): classifying items one by one took 9.6 s of the 12 s "find candidates". `find_candidates` now divides the tree into thousands of subtrees (breadth first, first 4096 items on one thread) and searches them with rayon. The answer is sorted and merging is order-independent; the test `splitting_the_work_never_changes_the_answer` compares a single-thread walk with several split points (D68). On 2 cores: 2.65 s -> 1.58 s on the synthetic scan; identical output.
- One deliberate tightening: when the files of one old-file group are labelled with different risks by different overlapping rules, the group takes the more cautious label (before, it took the first file's label, which depended on walk order).
- `find_candidates` now needs `&(dyn PathResolver + Sync)`.
- Remaining (real run): read scan file ~6 s (600 MB JSON), attribution ~2.3 s, owner lookup ~1.6 s (rebuilds the index). Ideas: binary scan format, one shared tree/index for all steps.

## B5 planning notes
| ID | Status | Phase | Note | Next step |
|---|---|---|---|---|
| N53 | open | B5 | `dc-attrib` needs a small public function listing container folders (`container_folders`), and `dc-rules` must make `blocker_inside` public. | B5 slice 1 |
| N54 | open | B5 | Only direct children of containers are examined. A dead app inside a live vendor folder (for example `Roaming\Adobe\OldApp`) is not found. | Later extension, depth 2 under vendor-owned folders |
| N55 | open | B5 | Shortcuts that point to missing programs are good leftover evidence, but the snapshot only keeps shortcuts of apps. Needs `dc-winmeta` to keep broken shortcuts too. | B5 follow-up or B6 |
| N56 | open | B5 | No process or service check (D76). A "leftover" could in rare cases belong to a program that is running but not registered. Quarantine (B9) must restore easily. | B9 |
| N57 | open | B5 | The personal-data check will make many real leftovers `DontTouch` (databases are common in app data). Review on the real run whether the list is too strict. | B5 real run |
| N58 | open | B5 | A shared `&Tree` for classify, attribution and leftovers would save about 2 s of repeated indexing on a 1.5 M-entry scan (see N52). | Before the UI |
| N59 | open | B5 | Real-run check list from the owner: McAfee, Valorant (Riot), The Finals (Embark). Riot Client is still installed (its cache is offered in B4b), so only the game and Vanguard parts can be leftovers. Expected places: `ProgramData\\McAfee`, `Program Files\\McAfee`, `AppData\\Local\\McAfee`, `C:\\Riot Games`, Embark data under `AppData\\Local`. | Compare after slice 1 |
| N60 | open | B5 | The known-apps list starts with 31 entries (antivirus, launchers and games, common utilities). `report_only` marks launchers and games (Steam, Epic, Riot titles, The Finals) because their folders can hold installed games. The folder names for The Finals (Embark) should be checked on the real run. | B5 real run |
| N61 | open | B5 | The list matches folders by exact normalized name, so "Norton 360" and "Norton" are the same name (digits are dropped). Two entries must never share a normalized name (checked on load). | Keep when extending the list |
| N62 | open | B5 | N53 is done (`container_kind`, `child_lists`, `folder_blocker` are public). | none |

- N63: `dc leftovers` accepts several scan files (profile, ProgramData, Program Files, Program Files (x86)). Each is analysed on its own, then `dc_leftovers::merge` combines them: same folder kept once, sorted and totalled again, so the order of the files does not matter (tested).
- N64: Real run over Program Files showed Java, Adobe, Logi, PD, Patriot, PCHealthCheck offered as ProbablySafe on "nobody registered it" + age alone. Portable tools and manually unzipped runtimes look the same, so that evidence alone is now capped at Low (DontTouch). Needs a second kind of evidence (stale registration, backup name, known app) to be offered.
- N65: Frontend prototype in `apps/desktop/` (Vite + TypeScript + Sigma.js WebGL + graphology ForceAtlas2), run with `cd apps/desktop && npm install && npm run dev`. Runs on made-up data (`apps/desktop/src/sample.ts`) that replays a scan as growing nodes. Event contract in `apps/desktop/src/model.ts`. Zoom hides small nodes (threshold grows with zoom-out). Not yet done: Tauri wrapper, real data via dc-api (B10), 3D mode, and aggregation for 1M+ nodes (the real disk has far more than the sample's 1.6k; only folders above a size should become nodes, files appear on zoom/expand).
- N66: Graph view built (F1, `apps/desktop/`). The first prototype (N65) is replaced. Measured in the cloud container: a 1.5 million entry file loads in about 1.7 s (parse and check in a worker), level of detail takes 5-12 ms, search 40-60 ms, the page holds about 270 MB. Frame times there are not meaningful (software WebGL); check smoothness on the PC.
- N67: Later for the view: Tauri wrapper with live scan events (`Tree.append` is already the live path), a 3D mode, "scattered traces" shown as dashed links to an app, colour by age, saving pinned positions, and showing protected folders (the export does not yet include the protected list).
- N68: On Linux the view shows paths with `\` after the scan root (`/usr\lib`). Windows-first, so left as is.
- N69: `npm install` needs `legacy-peer-deps` (set in `apps/desktop/.npmrc`) because npm 10 crashes resolving vitest 4's optional peers. All packages are pinned to exact versions; `npm audit` is clean.
- N70: F2 built so far: three looks, settings dialog (look, theme, graphics), node shapes, glow, scan pulses, focus blur, details drawer, path bar, minimap, size scale, right-click menu, focus mode (Enter or "Focus here"), save picture, H for graph only. Waiting for the owner to pick a look; then: true edge bundling (links that merge like roots), 3D mode, filters by age/type/size, history (back/forward), tree list view, contrast and colour-blind checks in QA.
- N71: Close-up of large folders can show big parent rings overlapping right after a focus jump until the layout settles; revisit collision strength for big nodes.
- N72: Building the picture is paced (D94). In the cloud container frames are limited by software graphics, so smoothness must be judged on a real GPU. The blurred back layer is now switched off entirely when nothing is selected (one WebGL layer less to draw).
- N73: F3 smoke-tested end to end in the cloud: the real Tauri window (Linux, under Xvfb) scanned /usr/share live through the Rust scanner and the IPC channel, grew to about 1,750 nodes, finished and settled; no page errors. Run `DC_SMOKE_SCAN=<folder>` with a debug build to repeat it.
- N74: Later for the app: save a finished live scan (and its view file) from the window; scan several folders into one picture; show scan errors in a panel; installer (NSIS) and code signing; "scattered traces" and Riot entries (C) still open.
- N75: Fixed in F3: scanning a whole drive stopped after the first folder, because "C:\" + "\" + name gave "C:\\name" and later folders were not found. Covered by a unit test with a drive root. The scan readout now shows progress for whole drives (space found against space in use on the drive, from `GetDiskFreeSpaceExW`, read-only) and an open-ended bar for folders.
- N76: Owner report after scanning E: on Windows: selection and greying got stuck (a highlighted chain like $RECYCLE.BIN -> S-1-5-21-... -> $RV02XVQ with no way to clear it), and focusing a folder and going back could leave a screen that did nothing. Causes found: a hover that was never cleared kept the chain lit, and during a live scan the map ignored focus changes and could wait forever for entries it could not place. Fixed (D100, D101) and covered by browser checks. The highlighted chain is simply the path from the drive down to the entry under the pointer or selected ($RECYCLE.BIN is the Recycle Bin, S-1-5-21-... is one user's bin inside it).
- N77: The save dialog on Windows may ask "replace the file?" before DiskCleaner sees the name; the app still refuses to replace and says so. Later: compare two saved scans ("what grew").
- N78: Scattered traces cover named folders in the usual places only. Single files (shortcuts on the desktop, loose logs) and registry entries are not traced yet.
- N79: The UserAssist reader (`dc-winmeta/src/windows/registry.rs`, `read_user_assist`) could not be compiled here (no Windows target). Its decoding is tested; run `cargo test` and `cargo clippy` on Windows. If "last used" shows "not known" for apps you use daily, check that the Start menu setting "Show most used apps" (Track app launches) is on: Windows stops keeping the record when it is off.
- N80: F4 still to do: duplicates on request (job queue, HDD-friendly), live scans reading Downloads/Desktop/Documents first, a Settings switch for "last used".

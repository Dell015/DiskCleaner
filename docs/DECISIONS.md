# Decision log

Why things are the way they are. Read this first when reviewing (for example, the final QA pass
by a different model) so that deliberate choices are not mistaken for mistakes. Add a row
whenever a decision is made; never delete rows, mark them superseded instead.

Format: **ID - decision** / why / where it is enforced.

## Project-wide
| ID | Decision | Why | Enforced by |
|---|---|---|---|
| D1 | Rust workspace, Windows-first, backend only, public and open source | Core goal; UI comes later | `Cargo.toml`, `ROADMAP.md` |
| D2 | Read-only until the quarantine phase (B9, feature `destructive-actions`, off by default) | A cleaner that can delete must first be proven safe | clippy `disallowed-methods`, source-grep tests, `SAFETY.md` |
| D3 | `Risk` is default-deny: Safe < ProbablySafe < DontTouch; unknown always means DontTouch | Wrong "safe" is far worse than wrong "unsafe" | `dc-core`, rules phase |
| D4 | Links and reparse points are never followed; recorded as `Kind::Link`, 0 bytes | Avoids loops and double counting; protects other drives | `dc-scan` walker + tests |
| D5 | Hardlinks counted once, first name wins (deterministic merge) | Exact totals | `dc-scan` merge + tests |
| D6 | Tests use only generated fixtures and temp dirs, never real user data | Safety | `fixture-gen`, CLAUDE.md |
| D7 | Exact-pinned dependencies | Reproducible builds, supply-chain safety | workspace `Cargo.toml`, `deny.toml` |
| D8 | `unsafe` only in `dc-winmeta` (Windows modules) and `dc-mft`; each block has `// SAFETY:` | Keep the unsafe surface small | `forbid/deny(unsafe_code)`, review |
| D9 | Output files are created with `create_new`; nothing is overwritten | Never destroy user files | CLI code + tests |
| D10 | Size on disk follows Explorer: whole clusters only; MFT-resident tiny files count 0 | Numbers must match what the user sees in Explorer | `WindowsProbe`; verified against Explorer on 2026-11-08 |
| D11 | The scanner stays portable: `FileProbe` trait; the Windows probe lives in `dc-winmeta` | Keeps `dc-scan` free of `unsafe` and dependency cycles | design docs |
| D12 | Claude (not Kiro) develops; the user commits to git, Claude does not commit unless asked | User control | CLAUDE.md |
| D13 | When the user says "ask me questions", list all questions at once | User preference | memory/preferences |

## Phase B2 (Windows metadata)
| ID | Decision | Why | Enforced by |
|---|---|---|---|
| D20 | Read per-user (HKCU) and machine-wide (HKLM 64-bit and 32-bit views) apps | Otherwise per-user apps look like unknown folders | `requirements.md` R3 |
| D21 | Classic programs only; Store (MSIX/UWP) apps in a later phase. Unknown owner means don't touch | Smaller step; Store folders are protected anyway | R10 |
| D22 | Read Start Menu shortcuts (.lnk), parsed in safe Rust (no COM) | Find where apps really live without extra unsafe | R4, B2b |
| D23 | Read exe version info | Names owners that have no registry entry | R5, B2b |
| D24 | Signatures: opt-in (`--verify-signatures`), offline only (no revocation lookups), only for executables already linked to an app; built last (B2c) | Privacy/offline design; largest unsafe surface isolated | R5, B2c |
| D25 | Both `dc apps` and `dc scan --apps`; the latter is a plain longest-prefix label, the full graph is B3 | Useful now without pre-empting B3 | R6 |
| D26 | Apps whose install folder is missing are flagged `stale_registration`; report only | Cheap leftover evidence without risk | R6, `apps.rs` |
| D27 | App list is saved as a JSON snapshot (`schema_version`) | Tests and later phases do not need the registry | R7 |
| D28 | Registry opened read-only; uninstall strings stored as text and never executed; entries are never merged in B2 | Registry data is untrusted; merging belongs to B3 | R8, tripwire test |
| D29 | Windows-only code is verified on the user's machine and in Windows CI; all logic is tested with `FakeSource` | The build container is Linux | R9 |
| D30 | Installer caches (`ProgramData\Package Cache`, `InstallShield Installation Information`) are never an app's install folder | They hold setup files, not the app; showing them as the app's home would mislead | `parse::is_too_broad` + test |
| D31 | An app with no registry folder may take the folder of a shortcut whose name equals the app name exactly, marked `folder_origin: shortcut`; the folder must exist and not be too broad | Finds real homes without guessing from loose name matches | `apps.rs` + tests |
| D32 | `scan --apps` labels only directories equal to an app's install folder (plus the scan root if it lies inside one). Labels are additive (`app_labels`, optional in the file, schema version unchanged) and never change sizes | Useful now, graph is B3; old scan files still load | `label.rs`, `dc-core::AppLabel` |
| D33 | Shortcut files are read in safe Rust (bounded size, depth and count; links not followed); version info via `GetFileVersionInfoW` (reads the file as data, never runs it) | No COM, small unsafe surface | `lnk.rs`, `windows/version.rs` |
| D34 | Order of work: B2c (signatures), then the protected-path engine (B4a), then attribution (B3), then rules and the rest of B4 | Safety-critical protection should exist before anything starts classifying or suggesting; B3 can then use it | `ROADMAP.md`, `NOTES.md` N1 |
| D35 | Signature results use `no_embedded_signature` instead of "unsigned" | A file with no embedded signature may be signed through a Windows catalog (most system files). Calling it "unsigned" would be wrong and could mislead trust decisions | `signature.rs` |
| D36 | Signature checks use `WinVerifyTrust` with no revocation checks and cache-only URL retrieval, and only run on each app's `main_exe` | Offline, fast, no network | `windows/signature.rs` |

## Phase B4a (protected paths)
| ID | Decision | Why | Enforced by |
|---|---|---|---|
| D40 | `dc-protected` answers only `Protected` / `NotProtected`; there is no "safe" result. `NotProtected` never means safe | Only rules (B4b) can say Safe; unknown stays DontTouch | `verdict.rs`, API docs, QA brief |
| D41 | `*:\Windows` protected (subtree) on every drive; `Windows.old` left to rules | Second or old installs are as dangerous as the live one; `Windows.old` is a legitimate cleanup target | `list.rs` |
| D42 | Program Files roots protected as folders only, not contents | Apps inside are judged by app/signature/rules | `list.rs` |
| D43 | Documents, Desktop, Pictures, Music, Videos, OneDrive: whole subtree protected forever; Downloads is not | No rule may ever call user documents safe; Downloads is a classic cleanup target | `list.rs`, pinned tests |
| D44 | UNC/network and device paths are protected outright in v1 | A cleaner must not touch shares or raw disks | `normalize.rs` |
| D45 | `.git` anywhere, `System Volume Information`, `$Recycle.Bin`, pagefile/hiberfil/swapfile and `$`-system files are protected | Data loss or system damage | `list.rs` |
| D46 | Fail closed: unresolvable or odd paths are protected | SAFETY.md rule 9 | `check.rs` |
| D47 | Keys and credentials (`.ssh`, `.gnupg`, `.aws`, `.azure`, `.kube`, Windows credential stores) are protected | Hard to recover, tiny to keep | `list.rs` |
| D48 | Both the typed path and the OS-resolved path are checked; protected if either is | Escalate-only applied to links and short names | `check.rs` |
| D49 | Runtime extras can only add protection | Protection cannot be weakened | `ProtectedList::with_extra` |

### Added after web research (B4a)
- **D50** Added to the built-in list: `ProgramData\Package Cache`, `Config.Msi`, `$PatchCache$` (installer rollback/repair data), `Program Files\WindowsApps`, `Common Files` (both Program Files), `\Windows\Installer` (already inside `\Windows`, documented). Why: deleting them breaks uninstall/repair of other programs, and Windows cannot rebuild them.
- **D51** Virtual disks (`*.vhd`, `*.vhdx`, `*.vmdk`, `*.vdi`, `*.qcow2`) and Docker data folders are protected at any depth. Why: one file can be an entire WSL distro, VM or Docker store; deleting one is the biggest single-file loss possible. Cleaning them is a deliberate, separate future feature, never a rule.
- **D52** A folder that *contains* a protected location is itself protected (`contains_protected`), so deleting a parent can never remove a protected child. Why: a recursive delete of `C:\Users\bob\AppData\Roaming` would otherwise hit `Microsoft\Protect`.
- **D53** `\\?\` and `\??\` paths: `.` and `..` are refused (Windows does not resolve them there, so our reading could differ from the file system's). `\Device\...`, `GLOBALROOT`, volume-GUID paths and rootless `\dir` paths are invalid and therefore protected.
- **D54** `$WINDOWS.~BT`, `$WINDOWS.~WS` and other `$*` root folders stay protected for now (fail closed). They are legitimate cleanup targets after a Windows upgrade; that will be done as an explicit, narrow rule exception in B4b, not by loosening protection.

## Phase B3 (attribution)
- **D55** Q1-Q9 all taken at the recommended defaults: pure library function; name matches are exact after normalization (no substrings); vendor folders get an owner of their own with members keeping theirs; ties are `ambiguous` with no owner; Windows areas belong to the built-in owner "Windows".
- **D56** Each byte of the scan is counted in exactly one bucket (an owner, `shared`, or `unattributed`); `footprint` is shown separately and is not additive. Property-tested.
- **D57** Names shorter than 4 normalized characters never match by name (only by registered install folder). Why: `Go`, `Git`-like names would claim too much.
- **D58** Ambiguity margin: claims within fewer than 10 strength points of the best are treated as a tie.
- **D59** First slice of B3 delivered: install-folder, main-program-folder, Windows-area, name-match and publisher-folder evidence, totals, `dc attrib` and `--explain`. Executable discovery (N15) and MSI publisher matching are the next slice (N30, N31).
- **D60** Slice 2: programs inside folders are identified from their version resource (read-only, never run). Budget: 3 programs per folder, depth 4, 3000 in total. An app with a folder gets a `ProgramIdentity` finding when product/description matches its name. Apps with no folder (MSI): one match gives the app (low confidence); several components of one publisher give the *publisher* (low confidence); anything else stays unowned. Company name alone never creates an owner.
- **D61** `dc attrib` reads version info on Windows by default; on other systems it is skipped.

## Phase B4b (rules and classification)
- **D62** Q1-Q11 at the recommended defaults: TOML packs, built-in pack only (no user packs yet), highest risk wins, unknown = DontTouch, protected list first, candidate folders containing anything special are refused, Safe only for self-regenerating things, Windows internals stay protected, upgrade leftovers ProbablySafe, ages as of scan end, `toml` crate pinned.
- **D63** The D54 change: `$WINDOWS.~BT` and `$WINDOWS.~WS` (exact names, drive root) are exempt from the `$*` protection in `dc-protected`; every other `$` name stays protected. Packs still cannot override the protected list.
- **D64** A rule that makes something lenient (Safe/ProbablySafe) only applies when the typed path AND the resolved path both match; a DontTouch rule applies if either does. Links, cloud files and hardlinked files are never offered. Failed resolution = DontTouch.
- **D65** Rule paths with wildcards need at least 3 named components; a path without wildcards is allowed at any depth (it names one place). Examples are mandatory and are checked on load.
- **D66** `dc classify` measures ages as of the end of the scan (not "now") and double-checks candidates against the real file system (`verify`): links above a candidate, unresolvable or protected real locations move it to "refused".
- **D67** Folder candidates that are refused can still leave ordinary files inside as "partial" candidates (the blocked parts are never included).

## Known limitations (accepted on purpose)
- Catalog-signed Windows files report `no_embedded_signature` in B2 (D24, D35).
- Volumes mounted on folders may use the wrong cluster size; falls back to the 512-byte rule (D10).
- Shortcuts without a stored local target (Store apps, network targets) are ignored in B2 (D21).
- Hidden/duplicate registry entries are kept, not merged (D28).
- `dc` has no Ctrl+C handler yet (library cancel token only).
- **D68** Performance rule (from the project owner): speed is pursued, but never by giving up correctness or safety. A speed-up must give the same answers as before, and must be proven with a test that compares the new code with the old behaviour (example: the byte-based `glob_match` is tested against the old character-based version over every short pattern and text). Never skip a safety check to be faster; skip work only when the result is provably unchanged.
- **D69** B5 Q1-Q14 all taken at the recommended defaults (requirements file). Leftover kinds: data of an app whose program folder is gone, program folders nobody registered, backup siblings (`X.old` and similar). Stale registrations are only reported.
- **D70** A leftover needs positive evidence. An unowned folder alone is "unowned", never "leftover". Every finding shows its evidence and a confidence (High >= 80 points, Medium >= 50, otherwise Low).
- **D71** Findings are never `Safe`. High and Medium give `ProbablySafe`; Low is reported as `DontTouch` (possible leftover, not offered). Anything that may hold personal data is `DontTouch`. The final risk is `escalate_all` of every step.
- **D72** Age (untouched 180 / 365 days) only supports other evidence. A change within the last 30 days caps confidence at Low.
- **D73** Only direct children of containers (per-user AppData Local/Roaming/LocalLow, ProgramData, Program Files, Program Files (x86), Common Files) are examined. Never Documents, Desktop, Downloads, Pictures. Depth-2 vendor folders are a later extension (N54).
- **D74** The registry is never written or deleted from. Apps registered inside another app's folder, updates, system components and hidden entries are not used as evidence (N37).
- **D75** Anything protected, owned by Windows or published by Microsoft, or containing a link, cloud placeholder or hardlinked file, is refused, not offered.
- **D76** No running-process check and no built-in list of known apps in B5; evidence comes from this computer's registry snapshot and the scan only.
- **D77** Findings reuse the B4b `Candidate` type so later phases handle rule candidates and leftovers the same way.
- **D78** The personal-data check is deliberately cautious and uses only names, extensions, sizes and times from the scan (no file is opened).
- **D79** B5 is built in one piece (all four kinds, personal-data check, stale-apps list, second look at the disk, `--explain`), not in two slices, because of D81.
- **D80** Accepted by the owner. Q14 named McAfee, Valorant and The Finals, and fully uninstalled apps leave no registry trace, so B5 has a built-in list of well-known apps (`rules/apps/known_apps.toml`, data with mandatory examples, validated on load). It adds a fourth kind, `known_app_removed` (60 points): a folder named exactly like a listed app (after normalization) that is unowned and whose app is not installed now. The list can only add evidence. Entries marked `report_only` (game launchers and games that can hold installed games or saves) are shown but never offered. This reverses the Q9 default.
- **D81** Working rule from the owner: for obvious questions, take the most complete, detailed option without asking. Questions are for real trade-offs only.

- **D82** Graph view (F1) is a web page (Vite, TypeScript, Sigma.js WebGL, graphology, d3-force) so the same code runs in a browser now and inside a Tauri window later. It is read-only: no file system access except a file the user picks or drops, no network (a Content-Security-Policy in the built page allows only its own files).
- **D83** The backend hands the view one compact columnar file, `dc view-data` (schema `dc-view` v1): one array per field instead of one object per entry, so 1.5 million entries fit in a browser (about 50 bytes per entry). Several scans are joined under a virtual root "This PC". Parent index is always smaller than the entry's index. Never overwrites a file.
- **D84** Level of detail: the graph never holds more than a budget of nodes (default 1,500, "Detail" slider 300-5,000; hard cap 6,000 even for requests). The biggest folders open first; folders the user or the zoom opened go before anything else; small children are summed into one "N more" node, so the sizes on screen always add up (tested for every open folder).
- **D85** Zoom is semantic: a folder drawn with a radius of 24 px or more opens; one the zoom opened closes again below 9 px (the gap stops flicker). Node radius grows with size^0.42 (bigger means bigger, but soft enough for the layout); a node is never drawn smaller than a 2.2 px dot and edges keep a fixed pixel width.
- **D86** Layout: d3-force (links, repulsion, collision, weak centering) instead of ForceAtlas2, because tree pictures came out clumped with FA2 and d3-force gives per-link distances and a natural drag. After the first growth, changes are laid out locally: existing nodes hold still for 1.6 s while new ones settle, so the place you zoomed into does not drift away.
- **D87** Colours follow risk only upward: an entry shows the riskiest mark of itself and the folders above it (a "Safe" cache inside a "Don't touch" folder is drawn as "Don't touch"). The findings list shows each mark as the backend wrote it.
- **D88** The view file is checked before anything is shown and refused as a whole on any problem (bad parent, length mismatch, a "don't touch" or refused mark that says it is offered, unknown risk, bad numbers). File names are only ever put on the page as text.
- **D89** The demo disk is generated in the page (made-up names, about 620 GB whatever the entry count), so the view can be tried and tested without anyone's real files.

- **D90** F2 polish choices from the owner: defaults on everything except: graphics settings like a game (Low/Medium/High/Ultra or each effect alone, automatic step-down if slow); a written node design system (`docs/phases/f2-ui-polish/node-design-system.md`); clicking a node opens a details drawer and greys (and, if allowed, blurs) everything unrelated; three looks built first so the owner can pick.
- **D91** Risk is colour and shape (solid = safe, ring = probably safe, bullseye = don't touch); colours from the Okabe-Ito colour-blind-safe set.
- **D92** Font: Atkinson Hyperlegible Next and Mono, bundled (no network), chosen so similar characters in paths never look alike.
- **D93** Focus blur uses a second sigma layer behind the main one, blurred with CSS; glow and scan pulses are drawn on a 2D canvas between the two. Effects can be switched off one by one.

## Final QA pass: brief for the reviewer
Review with fresh eyes, and treat the table above as intentional design unless it is unsafe or wrong.
1. **Safety first:** search every crate for anything that can delete, move, rename, write or execute (including registry writes, `Command`, `ShellExecute`). Confirm only `dc-quarantine` could ever do it and only behind the feature.
2. **Default-deny:** find any code path where "unknown" could end up as Safe.
3. **Untrusted input:** registry text, shortcut bytes, file names, and snapshot JSON must never cause a panic, path escape, or execution. Look for slicing on non-char-boundaries and unchecked arithmetic.
4. **Unsafe audit:** every `unsafe` block has a correct `SAFETY` comment, handles are closed once, buffers are sized correctly, pointers do not outlive their owners.
5. **Numbers:** totals, aggregation, hardlink and link handling, size-on-disk rules (D10).
6. **Tests that cannot fail:** tests that assert nothing, depend on the machine, or skip silently.
7. **Docs vs code:** requirements and design match what was built; `CHANGELOG.md` is accurate.
8. **Windows-only paths:** code that only compiles on Windows gets extra scrutiny because the author could not compile it in the build container.
9. **Graph view (apps/desktop/):** confirm the page cannot write, delete or send anything; that `checkView` rejects every contradiction; that no file name reaches `innerHTML`; that the level of detail never exceeds the hard cap and sizes add up.
Report findings as: severity, file:line, what is wrong, a concrete failing scenario, suggested fix.

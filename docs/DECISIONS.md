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

## Known limitations (accepted on purpose)
- Catalog-signed Windows files report `no_embedded_signature` in B2 (D24, D35).
- Volumes mounted on folders may use the wrong cluster size; falls back to the 512-byte rule (D10).
- Shortcuts without a stored local target (Store apps, network targets) are ignored in B2 (D21).
- Hidden/duplicate registry entries are kept, not merged (D28).
- `dc` has no Ctrl+C handler yet (library cancel token only).

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
Report findings as: severity, file:line, what is wrong, a concrete failing scenario, suggested fix.

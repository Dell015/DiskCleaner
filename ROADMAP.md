# DiskCleaner: Backend Roadmap

Repo location: `E:\CodeShi\DiskCleaner`
Scope: **backend only** (Rust). No UI code. The frontend (Tauri + React + R3F) comes later and will consume the API crate defined here.
> Update: the graph view frontend was started early at the owner's request; it lives in `apps/desktop/` (phases f1, f2 in `docs/phases/`). Rule packs live in `rules/`.


---

## 0. How to use this document

Work through the phases in order (Section 6). Each phase has a goal, tasks, and an acceptance check.
Per-phase requirements and design notes live in `docs/phases/<phase>/`. Project rules for the
coding assistant live in `CLAUDE.md`; the safety rules in `SAFETY.md` always apply.

Rule of thumb: you approve the requirements and design; Claude implements and tests; you run the
acceptance check at the end of each phase.

---

## 1. Decisions locked (change only if you disagree)

| Topic | Decision |
|---|---|
| Language | Rust (stable, edition 2021), Cargo workspace |
| Platform | Windows 11 primary, Windows 10 22H2 best-effort. Core crates must also compile and test on non-Windows where possible (fixtures-based tests), so CI is cheap. |
| License | MIT (you can switch; it's one file) |
| Telemetry | None, ever. Stated in README. |
| Default scan | Parallel directory walk, **no admin required** |
| Fast scan | Optional NTFS fast path via an **elevated helper process**, only on user request (Phase B7) |
| Sizes | Record both logical size and size on disk |
| Links | Never follow junctions/symlinks. Dedupe hardlinks. Flag cloud placeholders. |
| Storage | In-memory tree + SQLite (`rusqlite`, bundled) for saved scans and diffs |
| Rules | Declarative TOML files, deterministic, evaluated in Rust |
| AI | `AdvisorProvider` trait: Claude API (BYO key, opt-in), Ollama (local), Mock (tests). **Escalate-only** merge enforced in code. |
| Destructive actions | **Do not exist until Phase B9**, and then only behind a Cargo feature `destructive-actions` that is off by default |
| Frontend contract | Library crate `dc-api` exposes typed functions and serde types; later wrapped by Tauri commands. Also exposes JSON-RPC over stdio for debugging. |

---

## 2. Division of labor

### Claude does (nearly everything)
- Workspace scaffolding, CI, lint config, `deny.toml`, `rustfmt.toml`, `clippy` config
- All crates in Section 4, with unit, integration, and property tests
- Fixture generator (dummy data trees) and test harnesses
- Rule files (initial set), schema, loader, evaluator, validator
- Windows API integrations (registry, shell known folders, Authenticode, version info, file IDs, reparse points)
- SQLite schema and migrations, scan diffing
- Advisor providers, prompt builder, redaction, preview payload, escalate-only merge
- Quarantine/restore engine (Phase B9)
- CLI (`dc`) for every capability, so you can QA without a UI
- Docs: crate READMEs, architecture doc, rules authoring guide, SAFETY.md, threat model
- Benchmarks (`criterion`) and a synthetic big-tree generator

### You do (needs a human or real hardware)
- Approve each spec's requirements/design
- Run each phase's **Acceptance Check** on your real machine (read-only commands only)
- Confirm UAC/elevation behavior and MFT correctness against WizTree numbers (Phase B7)
- Judge rule quality: "is this really safe to delete?" decisions
- Real-world app coverage: test against your own installs (Steam, Chrome, Brave, HoYoverse)
- Final security review of the elevated helper and the quarantine engine
- Create a restore point/backup before first run on real data
- README polish, screenshots, release (with me later)

### Later
- Frontend (Tauri shell, 2D treemap, R3F 3D graph)
- Polish, performance tuning, release engineering
- Independent review of the safety-critical crates (`dc-protected`, `dc-rules`, `dc-advisor`, `dc-quarantine`, the elevated MFT helper)

---

## 3. Working setup

1. **Rust:** install via rustup (plus Visual Studio C++ Build Tools on Windows) to build and test locally. CI also builds on Windows and Linux.
2. **Safe commands only:** build, test, clippy, fmt and git status/diff/commit are fine. Never run `del`, `rmdir`, `Remove-Item`, `rm`, `robocopy /mir`, `format`, or anything with `/s /q` against real data.
3. **Safety tripwire:** `clippy.toml` bans `remove_file`, `remove_dir`, `remove_dir_all` and `fs::rename` workspace-wide. Only `dc-quarantine` (Phase B9) may opt out, explicitly.
4. **Git:** commit after every completed unit of work and review the diffs.

---

## 4. Architecture (workspace layout)

```
DiskCleaner/
  Cargo.toml                 (workspace)
  deny.toml  rustfmt.toml  clippy.toml  .gitignore  LICENSE  README.md  SAFETY.md  ROADMAP.md
  CLAUDE.md
  .github/workflows/ci.yml
  docs/                      (architecture.md, threat-model.md, rules-guide.md, api.md)
  rules/                     (TOML rule packs, shipped with the app)
    protected.toml           (hard-coded-equivalent protected paths; also compiled in)
    apps/*.toml              (steam, chrome, brave, hoyoverse, ...)
    junk/*.toml              (temp, caches, logs, crash dumps, ...)
  tools/
    fixture-gen/             (builds dummy data trees)
  fixtures/                  (generated, gitignored; only used by tests)
  crates/
    dc-core/        shared types: NodeId, Node, Tree, Size, Timestamps, Risk, errors, serde schemas
    dc-protected/   protected-path engine (path canonicalization + deny list). SAFETY-CRITICAL
    dc-scan/        directory walker, aggregation, progress/cancel, reparse/hardlink/cloud handling
    dc-mft/         NTFS fast path (USN/MFT enumeration) + elevated helper protocol (Phase B7)
    dc-winmeta/     Windows metadata: registry uninstall entries, App Paths, shortcuts, known folders, version info, Authenticode
    dc-attrib/      owner attribution: maps nodes to apps, builds the app -> exe -> folder -> file graph
    dc-rules/       TOML rule schema, loader, validator, evaluator -> Risk + reason codes. SAFETY-CRITICAL
    dc-leftovers/   uninstalled-app leftover detection
    dc-store/       SQLite persistence, migrations, scan snapshots, diffs
    dc-advisor/     metadata-only AI layer, providers, redaction, escalate-only merge. SAFETY-CRITICAL
    dc-quarantine/  (Phase B9) quarantine, manifest, restore, restore-point. SAFETY-CRITICAL, feature-gated
    dc-api/         the façade the frontend will call: typed requests/responses, event stream, JSON-RPC
    dc-cli/         binary `dc`: scan, graph, classify, advise, diff, rules-check, bench, (quarantine)
```

Dependency direction (no cycles): `dc-core` <- everything. `dc-protected` <- `dc-rules`, `dc-quarantine`. `dc-scan`, `dc-winmeta` <- `dc-attrib` <- `dc-leftovers`. `dc-rules` + `dc-attrib` <- `dc-advisor`. All <- `dc-api` <- `dc-cli`.

Suggested crates: `walkdir`/`jwalk` or custom `rayon` walker, `rayon`, `crossbeam-channel`, `windows` (Microsoft's crate), `winreg`, `serde`, `serde_json`, `toml`, `rusqlite` (bundled), `thiserror`, `anyhow` (bins only), `tracing`, `clap`, `reqwest` (rustls, advisor only), `proptest`, `criterion`, `tempfile`, `insta` (snapshot tests), `parselnk` or similar for `.lnk`, `uuid`, `blake3` (manifest hashes, Phase B9).

---

## 5. Global rules for every phase (see SAFETY.md and CLAUDE.md)

1. **No deletion, move, rename, or write** to scanned paths in any phase before B9. Scanner opens with read-only access. A test enforces that no scan changes any fixture file (hash fixture tree before/after).
2. **Default-deny:** unknown path => `DontTouch`.
3. **Escalate-only:** merged risk = max(rule_risk, advisor_risk). `DontTouch` from a rule is final. Property-tested.
4. **Protected paths are compiled in** and checked on canonicalized paths (case-insensitive, `\\?\` stripped, 8.3 expanded, junction/symlink resolved, trailing dots/spaces handled).
5. **Metadata only to AI.** The advisor input type has no field that can hold file contents. Paths pass through redaction (`C:\Users\<name>` -> `%USERPROFILE%`, machine name, SID stripped).
6. **Tests never touch real user directories.** All tests use `tempfile` or `fixtures/`. The CLI refuses to run `scan` on a drive root unless `--i-know` is passed (guard against accidental huge scans during tests).
7. **Every phase ends green:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, docs updated, CHANGELOG entry.
8. **Every classification returns reason codes**, not just a risk level (so the UI and AI can explain it).
9. Public API types are `serde` serializable and versioned (`schema_version`).
10. No `unwrap()`/`expect()` in non-test code in safety-critical crates. `#![forbid(unsafe_code)]` everywhere except `dc-mft` and `dc-winmeta` (document each `unsafe` block).

---

## 6. Phases

Each phase has its own requirements and design in `docs/phases/`.

### Phase B0: Foundations
**Goal:** a green, empty workspace with CI and guardrails.
**Tasks**
- Cargo workspace with all crate stubs in Section 4 (empty but compiling)
- `rustfmt.toml`, `clippy.toml`, `deny.toml` (licenses, advisories), `.gitignore` (target, fixtures, *.db)
- GitHub Actions: build + test on `windows-latest` and `ubuntu-latest`, clippy, fmt, `cargo deny`
- `LICENSE` (MIT), skeleton `README.md`, `SAFETY.md` (the principles from Section 5), `docs/architecture.md`
- `tools/fixture-gen`: generates a deterministic dummy tree: fake "Steam", "Chrome", "Brave", "HoYoverse" app dirs with game files; fake AppData caches; temp junk; an "uninstalled app leftover" folder; a protected-looking fake `Windows\System32`; deep nesting; huge file count dir; zero-byte files; unicode and long-path names; a junction, a symlink, a hardlink pair; sparse files. Seeded RNG so it is reproducible.
- `dc-core` basic types and the `Risk` enum (`Safe < ProbablySafe < DontTouch`) with `max()` semantics + tests
**Acceptance check (you):** `cargo test --workspace` passes; `cargo run -p fixture-gen -- --out fixtures\sample` creates a tree under `E:\CodeShi\DiskCleaner\fixtures\sample`; CI is green on GitHub.

### Phase B1: Scanner v1 (the smallest useful version)
**Goal:** read-only scan of one folder tree -> JSON tree with sizes.
**Tasks**
- `dc-core`: `Node` (id, parent, name, kind: File/Dir/Link/Other, logical_size, size_on_disk, mtime/ctime/atime, attributes flags, extension, depth), arena-based `Tree` (Vec + indices, not Rc/Box), aggregate sizes, child counts, file/dir counts
- `dc-scan`: parallel walker (rayon), long path support (`\\?\`), error collection (access denied, vanished files) without aborting, reparse-point detection (not followed), hardlink dedupe via file ID, cloud placeholder flag (offline/recall attributes), size on disk (cluster size via `GetDiskFreeSpaceW`; compressed/sparse via `GetCompressedFileSizeW`), cancel token, progress events (files, bytes, current dir, throttled)
- Extension/type categorization (video, image, archive, code, game asset, installer, etc.)
- Top-N queries: largest files, largest dirs, by extension, by age
- `dc-cli`: `dc scan <path> --out scan.json [--max-depth N] [--min-size X]`, `dc top <scan.json> --files|--dirs|--ext`
- Tests: exact totals against fixture tree, link handling, cancel, errors, "scan doesn't modify fixtures" hash test, `criterion` bench on a synthetic 1M-node tree
**Acceptance (you):** `dc scan E:\CodeShi\DiskCleaner\fixtures\sample --out sample.json` matches the fixture generator's expected totals. Then scan one small real folder you choose (e.g., a project folder) and sanity-check sizes against Explorer properties / WizTree.
**Prompt:**
> Create the spec for Phase B1 from ROADMAP.md. Prioritize correctness of sizes and link handling over speed; add benchmarks but do not optimize prematurely.

### Phase B2: Windows metadata (`dc-winmeta`)
**Tasks**
- Known folders via `SHGetKnownFolderPath` (AppData Local/Roaming/LocalLow, ProgramData, Program Files x86/x64, Temp, Downloads, etc.), expand env vars
- Installed apps from registry: HKLM/HKCU `...\Uninstall`, WOW6432Node: DisplayName, Publisher, InstallLocation, UninstallString, DisplayIcon, EstimatedSize, InstallDate, DisplayVersion, SystemComponent flag
- `App Paths` entries, Start Menu shortcuts (`.lnk` targets), MSIX/UWP packages (`Get-AppxPackage` equivalent via WinRT or registry; prefer API over spawning PowerShell)
- File version info (company, product name, description), Authenticode signature status + signer (`WinVerifyTrust`), cached
- Abstraction trait `SystemInfoSource` with a fake implementation for tests; real impl is Windows-only
- `dc winmeta apps` CLI dump
**Acceptance (you):** `dc winmeta apps` lists your real installed apps; compare with Settings > Apps. Spot check 5 publishers/signatures.

### Phase B3: Attribution and relationship graph (`dc-attrib`)
**Goal:** app -> executables -> folders -> files graph (your core idea).
**Tasks**
- Graph types: `AppNode`, `ExeNode`, `LocationNode` (folder/file), edges with kind (`installed_at`, `data_in`, `cache_in`, `config_in`, `logs_in`, `shortcut_to`, `registry_ref`), confidence score and evidence list
- Attribution heuristics, in order: install location prefix match; exe path under app dir; Start Menu shortcut target; App Paths; folder name == publisher/product name under AppData/ProgramData/Documents; known-app rule packs; Authenticode publisher match; version-info product match
- Multi-owner and shared folders (e.g., `AppData\Local\Programs`, Common Files) handled explicitly
- Launcher-type apps (Steam, Epic, Battle.net, HoYoPlay) as parent apps with child "games" via rule files (e.g., parse `libraryfolders.vdf`, `appmanifest_*.acf`) as data, not code, where possible
- Last-used signal: file atime if available, Prefetch name presence (read-only), shortcut/jump list (optional), else `unknown`
- Output a graph JSON; `dc graph <scan.json> --apps`, `--app steam`
- Tests on fixture: Steam fixture yields app -> games -> files with expected sizes
**Acceptance (you):** `dc graph` on a real scan of `C:\Program Files` + AppData shows Chrome/Brave/Steam correctly grouped. List mistakes; give them to Claude as rule fixes.

### Phase B4: Protected paths and rules engine (`dc-protected`, `dc-rules`)
**Tasks**
- `dc-protected`: compiled-in protected list (Windows, System32, SysWOW64, WinSxS, Boot, Recovery, EFI, `$Recycle.Bin`, `System Volume Information`, pagefile/hiberfil/swapfile, user profile roots, `NTUSER.DAT*`, Documents/Desktop/Pictures roots, ProgramData\Microsoft, Program Files roots themselves, `.git` dirs, other drives' root system files). Canonicalization pipeline and a test suite of ~100 evasion cases (case, slashes, `..`, 8.3 names, junction into System32, `\\?\`, trailing dot/space, UNC, device paths)
- `dc-rules`: TOML schema for rules: `id`, `description`, `match` (path globs with env vars, extensions, name regex, age, size, owner app), `risk`, `reason_code`, `explanation`, `source` (own research), `regeneratable: bool`, `side_effects`. Loader + validator (`dc rules-check` fails on duplicate ids, missing explanation, a rule claiming Safe on a path that matches protected)
- Evaluator: first-match precedence by specificity; returns `Classification { risk, reasons[], rule_ids[], regeneratable, explanation }`. Default-deny.
- Initial rule packs (your own wording; study BleachBit winapp2 for format and ideas only, no verbatim copying, check its license): temp dirs, browser caches (Chrome, Edge, Brave, Firefox), shader caches (NVIDIA, AMD, DirectX), Windows Update download cache, crash dumps, thumbnails cache, npm/pip/cargo/gradle/NuGet caches, Docker/WSL notes (flag as DontTouch/ProbablySafe with big warning), Steam shader cache, `Downloads` installers (ProbablySafe by age), logs
- Property tests: protected path can never classify below `DontTouch`; classification is deterministic
- `dc classify <scan.json>` outputs risk per node with reasons
**Acceptance (you):** Read every shipped rule; reject or edit ones you wouldn't trust. Classify a real scan; review the top 50 "Safe" results by size; if any makes you nervous, tell Claude to add a rule or demote.

### Phase B5: Leftover detection (`dc-leftovers`)
**Tasks**
- Compare candidate folders in AppData/ProgramData/Program Files/Documents/LocalLow against installed-app evidence (registry, shortcuts, MSIX). A folder is a *leftover candidate* only if: no matching installed app, name/publisher matches a *previously installed* app evidence (e.g., orphaned Uninstall data, registry residue, Start Menu leftovers, `Installer\$PatchCache$`), or matches known-app rule with the app absent
- Confidence levels and evidence list; never above `ProbablySafe` automatically; user-data-looking subfolders (saves, profiles, `Documents`) force `DontTouch`
- Registry residue **detection only** (report keys; no registry writes ever)
- Orphaned installer cache, empty app folders, broken shortcuts
- `dc leftovers <scan.json>`
**Acceptance (you):** run on your machine; verify each reported leftover manually against apps you know you uninstalled.

### Phase B6: Persistence, diffs, multi-drive
**Tasks**
- `dc-store`: SQLite schema (scans, nodes, classifications, attributions), migrations with versioning, WAL mode, bulk insert fast path, query API (children, top-N, search by name, subtree sums)
- Scan diff: what grew/shrank/appeared between two snapshots
- Multi-drive orchestration: enumerate volumes (fixed, removable, network), per-volume scan jobs, combined view, volume metadata (fs type, cluster size, free/total)
- Scheduling-agnostic job manager (queue, cancel, progress events) in `dc-api`
- Large-tree strategy: lazy loading, subtree aggregation for tiny files so UIs can render big trees
**Acceptance (you):** scan a drive twice, install/delete something small in between, `dc diff` shows it.

### Phase B7: NTFS fast scan + elevated helper (`dc-mft`)
**This is the riskiest phase; budget extra review.**
**Tasks**
- Research doc first (`docs/mft-design.md`): compare (a) raw `$MFT` parsing via volume handle vs (b) `FSCTL_ENUM_USN_DATA` enumeration + `FSCTL_GET_NTFS_FILE_RECORD`/file-ID resolution for sizes. Recommend one. Claude must justify with Microsoft documentation.
- Separate binary `dc-helper.exe`, elevated via UAC **on demand only**. Main app never runs as admin.
- IPC: named pipe with a locked-down ACL (current user SID only), versioned protocol, **read-only commands only** (enumerate volume, return records). No command in the protocol can modify the filesystem. Test that the protocol enum contains no write-capable variants.
- Handle: path reconstruction from parent references, hardlinks, deleted records, alternate data streams (decide and document), compressed/sparse sizes, resident data
- Fallback to walker if helper unavailable or volume not NTFS
- Differential test: for a given folder, MFT scan totals == walker totals (within documented tolerances)
- Bench on a large volume
**Acceptance (you):** run fast scan on C:, compare totals to WizTree and to the walker; time it; confirm UAC prompt appears only when requested and helper exits cleanly.
**Prompt:**
> This phase is security-sensitive. Write the design doc first and stop for my approval before implementing the helper.

### Phase B8: Advisor (AI layer, backend only) (`dc-advisor`)
**Tasks**
- `AdvisorInput` struct: names, redacted paths, size, dates, extension/type, owner app, publisher/signature, rule classification and reasons. **No content fields.** Compile-time guarantee by construction and a test that serializes inputs and scans for any known fixture file contents
- Redaction module: user profile, machine name, SIDs, serials; reversible mapping kept locally for display only
- Preview API: returns the exact payload that would be sent; `dc advise --dry-run` prints it
- Providers behind `AdvisorProvider` trait: `ClaudeProvider` (HTTPS, key from env var or Windows Credential Manager, never in config file, never logged), `OllamaProvider` (localhost), `MockProvider`
- Strict JSON output schema (risk, explanation, side_effects, confidence); validate; on invalid output, discard and fall back to rule-only result
- **Escalate-only merge:** `final = max(rule_risk, advisor_risk)`; if rule is `DontTouch`, advisor not consulted at all (no data leaves the machine). Property tests and a mutation-style test proving a malicious/lying provider can't lower risk
- Prompt-injection hardening: file names are untrusted; the prompt tells the model to treat them as data; output is schema-validated; response can't trigger any action
- Batching, rate limiting, caching of answers in SQLite keyed by metadata hash, offline mode, cost estimates, timeouts, retries
- Off by default. Explicit opt-in flag in config; `dc advise` refuses without it.
**Acceptance (you):** `--dry-run` on 20 real items: read the payload; confirm no sensitive info. Then try with MockProvider that tries to downgrade risk; confirm rejected. Then a real call on a few harmless items.

### Phase B9: Quarantine and restore (`dc-quarantine`, feature `destructive-actions`)
**Only start after you have used B1-B8 on your real machine and trust them.**
**Tasks**
- Action plan object: user-selected nodes -> `Plan` (dry-run output listing everything that would move, total bytes, risk per item, blocked items)
- Revalidation at execution: re-canonicalize, re-check protected list, re-check file still matches the plan (size/mtime), refuse if changed
- Move into quarantine dir on the same volume when possible (fast), else copy+verify+remove; collision-safe naming; blake3 hash in manifest; manifest (JSON or SQLite): original path, quarantine path, size, hash, risk, reason, timestamp, plan id
- Restore: single item, whole plan, with conflict handling; verify hash
- Retention policy (user-configured; purge only by explicit user command, and purge has its own confirmation token in the API)
- Restore point: create via Windows System Restore API if available (needs elevation; degrade gracefully with a clear warning) or require explicit user acknowledgment of "no restore point"
- Locked/in-use file handling (skip + report, never kill processes)
- Rate limits: max items and bytes per plan unless overridden; never act on `DontTouch`; `ProbablySafe` requires per-item confirmation flag from the caller
- Crash safety: write-ahead journal so an interrupted operation can be resumed or rolled back
- Exhaustive tests in `tempfile` dirs only. Fault injection (disk full, access denied, power-loss simulation by killing mid-operation).
**Acceptance (you):** in a throwaway folder (or VM), quarantine, restore, interrupt mid-operation; verify state. No real-data use until you've reviewed the code (or I have).

### Phase B10: API façade and hardening (`dc-api`, docs)
**Tasks**
- Finalize `dc-api`: request/response types, event stream (progress, errors, partial results), pagination, cancellation, error taxonomy, `schema_version`, JSON-RPC stdio server, generated JSON Schema + TypeScript types (via `schemars` and `ts-rs` or `specta`) so the frontend gets types for free
- `docs/api.md` with examples; sample JSON fixtures for the frontend developer (me, later)
- Fuzzing (`cargo-fuzz`) for rules parser, path canonicalizer, advisor output parser
- Dependency audit, `cargo deny`, SBOM, reproducible release build, release workflow producing `dc.exe` zips on GitHub Releases
- Threat model doc (`docs/threat-model.md`): helper IPC, prompt injection, malicious rule packs, path tricks, supply chain
- Performance pass: profile scan/classify/store on a 5M-node synthetic tree and write results to `docs/perf.md`

---

## 7. Data model sketch (for the design docs)

```text
Node   { id, parent, name, kind, logical_size, size_on_disk, mtime, ctime, atime, flags{reparse, hardlink, cloud, sparse, compressed, system, hidden}, ext, category }
App    { id, name, publisher, version, install_dir, uninstall_cmd, source{registry|msix|shortcut|rule}, signed, last_used? }
Exe    { id, path, app_id, signer, product_version }
Edge   { from, to, kind, confidence 0..1, evidence[] }
Classification { node_id, risk, reasons[ReasonCode], rule_ids[], regeneratable, explanation, advisor? {risk, text, model, ts}, final_risk }
Risk   Safe < ProbablySafe < DontTouch      (Unknown => DontTouch)
```

---

## 8. Your QA loop per phase (10-20 minutes)

1. `git pull`/review the diff summary (don't read all code).
2. `cargo test --workspace`
3. Run the Acceptance Check commands.
4. Look for these red flags: any `remove_*`/`rename` outside `dc-quarantine`; any `unwrap` in safety crates; network calls outside `dc-advisor`; new dependencies you don't recognize; weakened tests (deleted or `#[ignore]`d).
5. Write findings in `docs/qa-notes.md`; paste them to me later for review.

---

## 9. Out of scope for now (future)

- Tauri shell, React UI, 2D treemap, R3F 3D graph, node visuals, onboarding, settings screens
- Installer/signing, winget manifest
- README screenshots/demo GIF
- Marketing/portfolio write-up

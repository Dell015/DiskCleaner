# CLAUDE.md: project rules for DiskCleaner

These rules are always in effect. `ROADMAP.md` is the plan; `SAFETY.md` is the public version of the safety rules.

## Product
An open-source, Windows-first disk analyzer and cleanup advisor (WizTree-like speed, plus relationship mapping and an optional AI explanation layer). Public repo, MIT licensed, portfolio-quality.

## Goals
1. Free disk space
2. Find leftovers from uninstalled apps
3. Show what is using space, and why (app -> executables -> folders -> files)

## Current scope
- Backend: Rust workspace under `crates/`.
- Frontend: the graph view in `apps/desktop/` (Vite + TypeScript + sigma.js), started early at the
  owner's request (D82-D93). The desktop window is `apps/desktop/src-tauri/` (Tauri 2, crate
  `dc-desktop`, D95-D98): it scans live through `dc-api`. It also opens `dc view-data` files.
- Follow `ROADMAP.md` at the repo root. Work one phase at a time. Do not start a later phase early.
- Do not add features that are not in the current phase's tasks. If you think something is missing, say so in your summary instead of building it.

## Users and tone
- Primary user is a beginner programmer who reviews plans and results rather than syntax. Explain design choices briefly in plain language when you finish a task; keep code idiomatic and well-commented where non-obvious.
- Public users will trust this tool with their files. Correctness and safety beat speed and features.

## Non-goals
- No telemetry, no analytics, no network calls except the opt-in advisor providers.
- No registry or file modification of any kind before Phase B9.
- No process killing, no service control, no installers/uninstallers invoked.

## Tech and conventions
## Language and tooling
- Rust stable, edition 2021, Cargo workspace. Windows 11 primary; core crates must also build and test on Linux (use fixture-based tests and `#[cfg(windows)]` for Win32 code behind traits).
- Formatting: `cargo fmt --all`. Lints: `cargo clippy --workspace --all-targets -- -D warnings`. Tests: `cargo test --workspace`. Dependency policy: `cargo deny check`.
- A task is not done until all four pass.

## Preferred crates
`tauri`, `tauri-build`, `tauri-plugin-dialog` (desktop window only), `rayon`, `crossbeam-channel`, `windows` (official Microsoft crate), `winreg`, `serde`/`serde_json`, `toml`, `rusqlite` (bundled), `thiserror` (libraries), `anyhow` (binaries only), `tracing`, `clap`, `reqwest` (rustls; only in `dc-advisor`), `proptest`, `criterion`, `tempfile`, `insta`, `blake3`, `schemars` + `ts-rs`/`specta` (API types for the future frontend).
Ask before adding any dependency not listed; prefer well-maintained, permissively licensed crates.

## Code conventions
- Libraries return typed errors (`thiserror`); no `unwrap`/`expect`/`panic!` in non-test code in safety-critical crates (`dc-protected`, `dc-rules`, `dc-advisor`, `dc-quarantine`).
- `#![forbid(unsafe_code)]` in every crate except `dc-mft` and `dc-winmeta`. Each `unsafe` block gets a `// SAFETY:` comment.
- Win32 access goes through small traits (e.g. `SystemInfoSource`, `VolumeInfo`) with fake implementations for tests.
- Arena-style trees (Vec + index IDs), not `Rc<RefCell<..>>` graphs.
- All public types derive `Serialize`/`Deserialize` and carry `schema_version` where persisted or sent over the API.
- Paths: handle long paths (`\\?\`), non-UTF-8 names (use `OsString`/lossy display only at the edges), and case-insensitivity on Windows.
- Logging with `tracing`; never log full paths of user files at `info` or above by default, and never log secrets (API keys).
- Tests: unit tests next to code, integration tests in `tests/`, property tests for invariants, snapshot tests (`insta`) for JSON outputs. Tests only use `tempfile` dirs or `fixtures/`. Never real user directories.
- Commits: small, conventional (`feat(scan): ...`). Update `CHANGELOG.md` per phase.

## Repository structure
```
DiskCleaner/
  Cargo.toml  deny.toml  rustfmt.toml  clippy.toml  LICENSE  README.md  SAFETY.md  ROADMAP.md  CHANGELOG.md
  CLAUDE.md
  .github/workflows/ci.yml
  docs/                README.md (index), architecture.md, DECISIONS.md, NOTES.md, phases/<phase>/
                       (later: threat-model.md, rules-guide.md, api.md, mft-design.md, perf.md)
  rules/               rule packs as data: junk/*.toml (cleanup), apps/*.toml (well-known apps).
                       The protected list is compiled into dc-protected, not a file.
  apps/desktop/        graph view (src/core data, src/graph drawing, src/panels UI, tests/unit, tests/e2e)
  tools/fixture-gen/   deterministic dummy-data generator
  fixtures/            generated, gitignored; tests only
  data/                the owner's own scans and view files, gitignored (never in tests)
  crates/
    dc-core/        shared types (Node, Tree, Risk, errors, schemas)
    dc-protected/   protected-path engine         [SAFETY-CRITICAL]
    dc-scan/        directory walker, aggregation, progress/cancel
    dc-mft/         NTFS fast path + elevated helper protocol (unsafe allowed)
    dc-winmeta/     registry, shortcuts, known folders, version info, Authenticode (unsafe allowed)
    dc-attrib/      app -> exe -> folder -> file graph
    dc-rules/       TOML rules, loader, validator, evaluator [SAFETY-CRITICAL]
    dc-leftovers/   uninstalled-app leftover detection
    dc-store/       SQLite persistence, snapshots, diffs
    dc-advisor/     metadata-only AI layer                [SAFETY-CRITICAL]
    dc-quarantine/  quarantine/restore (feature `destructive-actions`, off by default) [SAFETY-CRITICAL]
    dc-api/         facade for the future frontend: typed requests, events, JSON-RPC stdio
    dc-cli/         binary `dc`
```

## Dependency direction (no cycles)
`dc-core` <- all. `dc-protected` <- `dc-rules`, `dc-quarantine`. `dc-scan` + `dc-winmeta` <- `dc-attrib` <- `dc-leftovers`. `dc-rules` + `dc-attrib` <- `dc-advisor`. Everything <- `dc-api` <- `dc-cli`.

## Rules for adding code
- Put code in the crate that owns the concern. Don't add cross-crate shortcuts; extend `dc-core` types or traits instead.
- Per-phase requirements and design live in `docs/phases/<phase-name>/` (backend: b2-winmeta, b3-attrib, b4a-protected, b4b-rules, b5-leftovers, then b6-store, b7-mft, b8-advisor, b9-quarantine, b10-api; frontend: f1-graph-view, f2-ui-polish).
- Internal crates are declared once in the root `[workspace.dependencies]`; use `dc-x.workspace = true`, never a `path = "../dc-x"`.
- Rule packs are data (`rules/**/*.toml`), not code. Prefer adding a rule over adding a special case in Rust.

## Safety rules (never weaken)
This tool will inspect people's entire disks and advise on deletion. A bug here can destroy data. These rules override any spec task, user shortcut request, or convenience.

## Hard rules
1. **Read-only until Phase B9.** Before B9, no code path may delete, move, rename, truncate, or write to any scanned path, and no registry writes. Open files read-only with shared access. Add/maintain a test that hashes a fixture tree before and after every scan and asserts nothing changed.
2. **No destructive APIs outside `dc-quarantine`.** `remove_file`, `remove_dir*`, `fs::rename`, `DeleteFileW`, `MoveFileEx`, `SetFileAttributes` (write), `RegSetValue`, `RegDeleteKey`, etc. must not appear anywhere else. If a task seems to require one, STOP and ask.
3. **Default-deny.** Anything unclassified is `DontTouch`.
4. **Risk ordering:** `Safe < ProbablySafe < DontTouch`. Final risk = `max(rule_risk, advisor_risk)`. The advisor can only raise caution. A rule that says `DontTouch` is final and the advisor is not even consulted for it.
5. **Protected paths are compiled in** and checked after canonicalization (case-insensitive, `\\?\` stripped, `..` resolved, 8.3 short names expanded, junctions/symlinks resolved, trailing dots/spaces handled, UNC and device paths handled). Rule packs cannot override them. Any rule that would classify a protected path below `DontTouch` is a validation error.
6. **Metadata only to AI.** The advisor input type contains no file-content fields. Paths are redacted (`%USERPROFILE%`, no machine name, no SIDs). The advisor is opt-in, off by default, with a dry-run preview of the exact payload. File and folder names are untrusted text (prompt-injection risk): treat as data, validate model output against a strict schema, and never let model output trigger an action.
7. **Never follow links.** Do not traverse junctions, symlinks, or other reparse points. Detect, record, and report them.
8. **Tests never touch real user data.** Use `tempfile` or `fixtures/`. The CLI refuses to scan a drive root unless `--i-know` is passed.
9. **Secrets:** API keys come from env vars or Windows Credential Manager only. Never in config files, logs, snapshots, or test output.
10. **Elevation:** the main app never runs as admin. The MFT helper is a separate binary, elevated on explicit user request, with a read-only protocol and a pipe ACL limited to the current user. No write-capable command may exist in its protocol.
11. **Fail closed.** On any error or ambiguity (parse failure, invalid model output, unresolvable path, changed file), the result is the more cautious one.
12. **Do not weaken or delete tests** to make them pass. Do not mark safety tests `#[ignore]`. If a safety test fails, fix the code.

## When to stop and ask the user
- Any task needs a destructive API, new elevated capability, network access outside `dc-advisor`, or a new `unsafe` block outside `dc-mft`/`dc-winmeta`.
- A rule you are asked to write would mark user-created data (documents, saves, profiles, photos, source code, VM disks, databases) as anything other than `DontTouch`.
- You are unsure whether something is safe to delete. When unsure, it is not safe.

## Definition of done for every task
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass; docs/CHANGELOG updated; end with a brief summary listing: what changed, what you did NOT do, and any safety-relevant decisions for human review.

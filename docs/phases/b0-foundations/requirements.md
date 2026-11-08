# Requirements: Phase B0 — Foundations

## Introduction

Phase B0 produces a green, empty Rust workspace with CI, lint/dependency guardrails, project documents, a deterministic fixture generator, and the `Risk` type in `dc-core`. It is backend only. It contains **no scanner logic**, no Windows metadata access, no rules, no persistence, no advisor, and no destructive capability.

Sources: `ROADMAP.md` §1, §4, §5, Phase B0; `CLAUDE.md`, `SAFETY.md`.

### Scope boundary

In scope: items in Requirements 1–9 only.

Explicitly out of scope for B0 (belongs to later phases): `Node`/`Tree` types and any directory walking (B1), protected-path engine logic (B4), rule packs and the `rules/` directory (B4), `--i-know` drive-root guard in the CLI (B1, with `dc scan`), the "scan does not modify fixtures" hash test (B1, since there is no scan yet), `docs/threat-model.md`, `rules-guide.md`, `api.md`, `mft-design.md`, `perf.md` (later phases), any Cargo feature declarations such as `destructive-actions` (B9).

### Glossary

- **Workspace**: the root `Cargo.toml` and all member packages.
- **Stub crate**: a member crate with a crate-level doc comment and no functional code.
- **Fixture tree**: the dummy directory tree written by `fixture-gen`.
- **Manifest**: the JSON file `fixture-gen` writes describing the fixture tree and its expected totals.
- **Special entry**: a junction, symlink, hardlink, or sparse file in the fixture tree.
- **Four checks**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`.

---

## Requirement 1: Cargo workspace and crate stubs

**User story:** As the developer, I want every crate from ROADMAP §4 to exist as a compiling stub so that later phases only add code and never restructure the workspace.

### Acceptance criteria

1. THE Workspace SHALL contain these members under `crates/`: `dc-core`, `dc-protected`, `dc-scan`, `dc-mft`, `dc-winmeta`, `dc-attrib`, `dc-rules`, `dc-leftovers`, `dc-store`, `dc-advisor`, `dc-quarantine`, `dc-api`, `dc-cli`, and the member `tools/fixture-gen` with package name `fixture-gen`.
2. THE Workspace SHALL use Rust edition 2021, resolver 2, and shared `[workspace.package]` metadata (edition, license `MIT`, repository, `rust-version`). `rust-version` SHALL equal the current stable release at execution time (read from `rustc --version`).
3. THE Workspace SHALL declare shared dependency versions in `[workspace.dependencies]` with exact pinned versions (`=x.y.z`), and members SHALL reference them with `workspace = true`.
4. `dc-cli` SHALL be a binary crate whose binary is named `dc`; WHEN `dc` is run in B0 it SHALL print its name and version and exit with code 0, and SHALL perform no file system access.
5. Every other crate in criterion 1 except `fixture-gen` SHALL be a library crate.
6. Every crate except `dc-mft` and `dc-winmeta` SHALL contain `#![forbid(unsafe_code)]` at its crate root.
7. `dc-mft` and `dc-winmeta` SHALL contain no `unsafe` blocks in B0.
8. Each stub crate SHALL have a crate-level doc comment stating its responsibility (from ROADMAP §4) and, for `dc-protected`, `dc-rules`, `dc-advisor`, `dc-quarantine`, the marker "SAFETY-CRITICAL".
9. Stub crates SHALL NOT declare dependencies on each other in B0 unless needed to compile. The dependency direction from `structure.md` SHALL be documented in `docs/architecture.md` (Requirement 6).
10. No crate SHALL contain any call to `remove_file`, `remove_dir`, `remove_dir_all`, `fs::rename`, `DeleteFileW`, `MoveFileEx`, `SetFileAttributes`, `RegSetValue*`, or `RegDeleteKey*`.
11. WHEN `cargo build --workspace` runs on Windows or Linux THEN the build SHALL succeed with no warnings.

## Requirement 2: Formatting, lint, dependency policy, and ignore files

**User story:** As the developer, I want the guardrail configuration in place so that every later change is checked automatically.

### Acceptance criteria

1. THE repo SHALL contain `rustfmt.toml` using only options available on stable rustfmt.
2. THE repo SHALL contain `clippy.toml`, and the workspace SHALL configure lints (`[workspace.lints]`) so that in non-test code of `dc-protected`, `dc-rules`, `dc-advisor`, `dc-quarantine`, `clippy::unwrap_used`, `clippy::expect_used`, and `clippy::panic` are denied.
3. THE repo SHALL contain `deny.toml` that:
   - allows only permissive licenses (at minimum MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib) and denies all others by default;
   - fails on security advisories from the RustSec database and on yanked crates;
   - restricts crate sources to crates.io (no git or unknown registries);
   - warns on duplicate crate versions.
4. THE repo SHALL contain `.gitignore` that ignores at least `target/`, `fixtures/`, and `*.db`.
5. WHEN the four checks run on a clean checkout THEN all SHALL pass (locally on Windows; in CI as defined in Requirement 3).

## Requirement 3: Continuous integration

**User story:** As the developer, I want CI on GitHub so that I can see a green build without running anything locally.

### Acceptance criteria

1. THE repo SHALL contain `.github/workflows/ci.yml` triggered on push and pull request.
2. THE workflow SHALL run on a matrix of `windows-latest` and `ubuntu-latest`.
3. On each matrix OS, THE workflow SHALL run: build, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all --check`. `cargo deny check` SHALL run on `ubuntu-latest` only, as a separate job (license, advisory, and source results do not depend on the host OS).
4. THE workflow SHALL use the stable Rust toolchain with `rustfmt` and `clippy` components.
5. Every third-party action SHALL be pinned to a full commit SHA, with the version tag in a comment.
6. THE workflow SHALL set `permissions: contents: read` and SHALL NOT use secrets.
7. THE workflow SHALL NOT run `fixture-gen` against any path except a temporary directory inside the workspace (for example under `target/`).
8. IF any step fails THEN the job SHALL fail.

## Requirement 4: License and README

**User story:** As a public user, I want to know what the project is, its license, and its safety posture.

### Acceptance criteria

1. THE repo SHALL contain `LICENSE` with the MIT license text. The copyright holder SHALL be a placeholder (`<COPYRIGHT HOLDER>`) for the developer to fill in.
2. THE repo SHALL contain a skeleton `README.md` stating: the purpose (three goals from `product.md`); current status (pre-alpha, backend only, **read-only, no deletion capability**); **no telemetry, ever**; the build/test commands; how to run `fixture-gen`; links to `SAFETY.md`, `ROADMAP.md`, `docs/architecture.md`; license.
3. THE README SHALL NOT claim features that do not exist yet.

## Requirement 5: SAFETY.md

**User story:** As a public user and reviewer, I want the safety principles documented in one place.

### Acceptance criteria

1. THE repo SHALL contain `SAFETY.md` covering every principle in ROADMAP §5 (items 1–10) and the hard rules in `safety.md` (read-only until B9, no destructive APIs outside `dc-quarantine`, default-deny, risk ordering and escalate-only merge, compiled-in protected paths with canonicalization, metadata-only AI and redaction, never follow links, tests never touch real data, secrets handling, elevation model, fail closed, no weakening of tests).
2. SAFETY.md SHALL state which principles are already enforced in B0 (the `Risk` ordering and default; `forbid(unsafe_code)`; lint denials) and which are planned for later phases, without claiming later protections exist.

## Requirement 6: Architecture document

**User story:** As the developer, I want a short architecture doc so that later specs share one picture.

### Acceptance criteria

1. THE repo SHALL contain `docs/architecture.md` describing: the workspace layout (ROADMAP §4); one-line responsibility per crate; the dependency direction (no cycles) from `structure.md`; which crates are safety-critical; which crates may contain `unsafe`; the data-model sketch from ROADMAP §7 marked as "planned"; the risk model (`Safe < ProbablySafe < DontTouch`, unknown ⇒ `DontTouch`, `final = max(rule, advisor)`).
2. THE doc SHALL mark every component other than B0 deliverables as not yet implemented.

## Requirement 7: CHANGELOG

**User story:** As the developer, I want a changelog entry per phase (required by `tech.md` and the definition of done in `safety.md`).

### Acceptance criteria

1. THE repo SHALL contain `CHANGELOG.md` in Keep a Changelog style with an `Unreleased` section listing the B0 additions.

> Note: CHANGELOG.md is not listed in ROADMAP Phase B0, but the project rules (CLAUDE.md) require it for every phase. Included for that reason.

## Requirement 8: `dc-core` basic types and `Risk`

**User story:** As the developer, I want a single `Risk` type with strict ordering and escalate-only combination so that no later crate can lower caution by accident.

### Acceptance criteria

1. `dc-core` SHALL define a public enum `Risk` with exactly three variants, `Safe`, `ProbablySafe`, `DontTouch`, with total ordering `Safe < ProbablySafe < DontTouch`.
2. `Risk` SHALL derive or implement `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`, `Serialize`, `Deserialize`.
3. `Risk::default()` SHALL return `Risk::DontTouch` (default-deny).
4. `Risk` SHALL provide a combining function (for example `Risk::escalate(self, other) -> Risk`) that returns the more cautious value, equal to `std::cmp::max`. It SHALL also provide an associated function that combines an iterator of risks, returning `DontTouch` for an empty iterator (fail closed).
5. `Risk` SHALL serialize to and from stable string names (`"Safe"`, `"ProbablySafe"`, `"DontTouch"`). WHEN deserializing any other value THEN deserialization SHALL fail with an error; it SHALL NOT map to a less cautious variant.
6. `Risk` SHALL implement `Display` and `FromStr`. `FromStr` SHALL reject unknown strings with a typed error.
7. `dc-core` SHALL define a typed error enum using `thiserror` for its own errors (for example invalid risk string).
8. `dc-core` SHALL define a public `SCHEMA_VERSION` constant (an integer, starting at 1) for later persisted/API types.
9. `dc-core` SHALL contain no `unwrap`, `expect`, or `panic!` in non-test code.
10. `dc-core` SHALL NOT define `Node`, `Tree`, or other scanner types in B0.
11. Tests SHALL verify:
    - the ordering `Safe < ProbablySafe < DontTouch` and the default value;
    - property tests (`proptest`) that the combining function is commutative, associative, idempotent, has `Safe` as identity, and its result is never less than either input;
    - property test that combining any iterator containing `DontTouch` yields `DontTouch`;
    - empty-iterator combination yields `DontTouch`;
    - serde round-trip for all variants, a JSON snapshot test (`insta`) of serialized values, and rejection of unknown/mis-cased strings;
    - `FromStr` / `Display` round-trip and rejection of unknown strings.

## Requirement 9: `tools/fixture-gen` deterministic dummy-data generator

**User story:** As the developer, I want a reproducible dummy disk tree with realistic layouts and edge cases so that every later phase can test against it without touching real data.

### 9.1 Interface

1. `fixture-gen` SHALL be runnable as `cargo run -p fixture-gen -- --out <DIR>` with optional `--seed <u64>` (fixed documented default) and `--file-count <N>` for the huge-file-count directory (fixed documented default).
2. WHEN run with `--out fixtures\sample` from the repo root THEN it SHALL create the fixture tree under `E:\CodeShi\DiskCleaner\fixtures\sample` (the B0 acceptance check).
3. `fixture-gen` SHALL write the fixture tree to `<DIR>/tree/` and the manifest to `<DIR>/manifest.json`, so that the manifest is not part of the tree it describes.

### 9.2 Safety of the generator itself

4. `fixture-gen` SHALL only create new files and directories. It SHALL NOT delete, move, rename, truncate, or overwrite any existing path.
5. IF `<DIR>` exists and is not empty THEN `fixture-gen` SHALL exit with a non-zero code and an error message, and SHALL write nothing.
6. IF `<DIR>` resolves to a drive or file system root (for example `C:\`, `E:\`, `/`) THEN `fixture-gen` SHALL refuse and exit non-zero.
7. IF `<DIR>` cannot be resolved THEN `fixture-gen` SHALL refuse and exit non-zero (fail closed).
8. `fixture-gen` SHALL contain no `unsafe` code and SHALL perform no network access.
9. All generated file content SHALL be dummy data (seeded pseudo-random bytes or marker text). Files with executable-looking extensions (`.exe`, `.dll`, `.sys`) SHALL NOT contain valid PE headers.
10. IF creating a special entry fails because the OS or privilege does not allow it (for example symlinks without Developer Mode on Windows, or junctions on Linux) THEN `fixture-gen` SHALL skip that entry, record it in the manifest as `skipped` with the reason, continue, and exit with code 0. Any other error SHALL abort with a non-zero code.

### 9.3 Determinism

11. WHEN run twice with the same seed, options, and OS THEN the two fixture trees SHALL have identical relative paths, file sizes, and file contents, and the two manifests SHALL be byte-identical.
12. WHEN run with different seeds THEN file contents SHALL differ.
13. The RNG SHALL be an in-tree implementation (SplitMix64 for seeding, xoshiro256** for generation) with no new dependency, and SHALL produce the same sequence on every platform and toolchain version. Tests SHALL pin known output values for a fixed seed.
14. File modification times SHALL be set to deterministic, seed-derived values within a fixed historical range, so later age-based tests are reproducible.
15. The output SHALL NOT depend on the current time, machine name, user name, locale, or environment variables.

### 9.4 Contents

The fixture tree SHALL contain all of the following. Names below are suggested. They SHALL be documented in the manifest.

16. **Fake Steam:** a `Program Files (x86)/Steam` directory with `steam.exe`-named dummy file, `steamapps/common/<at least 2 game dirs>` containing game-asset-like files (`.pak`, `.bin`, `.dll`), `steamapps/appmanifest_<id>.acf` dummy files, `steamapps/shadercache/`, and `steamapps/downloading/`.
17. **Fake Chrome:** `Program Files/Google/Chrome/Application/chrome.exe` dummy, and `Users/<fake user>/AppData/Local/Google/Chrome/User Data/Default/` with `Cache/`, `Code Cache/`, `GPUCache/` and profile-like files (`Bookmarks`, `History`, `Preferences`).
18. **Fake Brave:** the same structure as Chrome under `BraveSoftware/Brave-Browser`.
19. **Fake HoYoverse:** a launcher dir (for example `Program Files/HoYoPlay`) and at least one game dir with large asset files, plus an `AppData/LocalLow/miHoYo/<game>/` data dir.
20. **AppData caches:** under the fake user's `AppData/Local` and `AppData/Roaming`, generic app cache dirs (for example `SomeApp/Cache`), a `CrashDumps/` dir with `.dmp` files, and log files.
21. **Temp junk:** a fake user `AppData/Local/Temp/` and a fake `Windows/Temp/` with `.tmp`, `.log`, and random-named files and dirs.
22. **Uninstalled-app leftover:** a folder for an app that has no install dir and no other evidence (for example `AppData/Roaming/DefunctApp/` and `ProgramData/DefunctApp/`) with config, log, and cache files.
23. **Fake protected System32:** `Windows/System32/` with dummy `.dll`, `.sys`, `.exe` files, a `drivers/` subdir, and `config/` with dummy `SYSTEM`/`SOFTWARE` hive-named files. All are dummy content (criterion 9).
24. **Deep nesting:** a chain of nested dirs at least 64 levels deep with a file at the bottom.
25. **Huge file count:** one directory with `--file-count` files (default at least 10,000). The default SHALL keep a full run under about 60 s on CI hardware.
26. **Zero-byte files:** at least 10 zero-byte files spread across several dirs, including at least one zero-byte file in an otherwise empty dir. The tree SHALL also contain at least one empty directory.
27. **Unicode names:** file and dir names including at least CJK, Cyrillic, Arabic (RTL), emoji (non-BMP), combining characters, and names with spaces and leading dots. All SHALL be valid on both NTFS and ext4 (no `<>:"/\|?*`, no reserved device names like `CON`, no trailing dot/space).
28. **Long paths:** at least one file whose absolute path exceeds 260 characters, and at least one path component of 255 characters. They SHALL be created successfully on Windows (using `\\?\`-prefixed paths internally if needed).
29. **Junction:** on Windows, a directory junction inside the tree pointing to another dir inside the tree, created with `cmd /C mklink /J` via `std::process::Command` with arguments passed as separate array elements (no shell string interpolation). Before spawning, both the link path and the target path SHALL be validated to be inside `<DIR>/tree/`. IF validation fails THEN generation SHALL abort. IF `mklink` fails THEN it SHALL be recorded as `skipped`. On non-Windows it SHALL be recorded as `skipped` (not applicable).
30. **Symlink:** a file symlink and a directory symlink inside the tree, each pointing to a target inside the tree. On Windows without symlink privilege they SHALL be recorded as `skipped` (criterion 10).
31. **Hardlink pair:** two paths in different dirs that are hardlinks to the same file content.
32. **Sparse files:** at least one file with a large logical size (at least 64 MiB) and very little allocated data (a small written region at a known offset). On Linux this SHALL be done by seeking/`set_len`. On Windows the file SHALL be marked sparse with `fsutil sparse setflag` (via `std::process::Command`, array args, path validated inside the tree), with no `unsafe` code. IF `fsutil` fails THEN it SHALL be recorded as `skipped`, and SHALL NOT fall back to writing the full logical size.
33. All link and junction targets SHALL be inside the fixture tree. No special entry SHALL point outside `<DIR>/tree/`.

### 9.5 Manifest

34. The manifest SHALL be JSON with a `schema_version` field and SHALL contain: the seed and options; generator version; and per-category entries listing relative paths.
35. The manifest SHALL contain expected totals: total file count, total dir count, total logical bytes counting each hardlinked file once and also counting every link path, and per-top-level-directory logical bytes and file counts. Size-on-disk SHALL NOT be stated (it is filesystem-dependent).
36. The manifest SHALL list every special entry with kind, relative path, relative target (for links), status (`created` or `skipped`), and skip reason.
37. The manifest SHALL state how special entries are counted in the totals (links counted as entries with zero bytes and not followed; sparse files counted by logical size), so B1 can assert exact totals.
38. Paths in the manifest SHALL be relative to `<DIR>/tree/` and use `/` separators.

### 9.6 Tests

39. All `fixture-gen` tests SHALL write only to temporary dirs created under `CARGO_TARGET_TMPDIR` or `target/` inside the repo, never under user directories and never in `fixtures/`.
40. Tests SHALL verify: determinism (two runs, same seed, compare relative paths, sizes, BLAKE3 or equivalent content hashes, and manifest bytes); different seed gives different content; refusal on a non-empty out dir (and the dir is unchanged afterwards, verified by hashing); refusal on a filesystem root, checked without writing; every category in 9.4 is present; manifest totals match an independent recount of the generated tree; long-path and unicode entries exist; hardlink pair shares a file identity where the OS exposes it; skip behavior is recorded rather than failing.
41. Tests that depend on OS capability (junction, symlink, sparse) SHALL assert `created` where the capability exists and `skipped` with a reason otherwise. They SHALL NOT be `#[ignore]`d.

---

## Non-functional requirements

1. **Platforms:** everything SHALL build and test on Windows 11 and Linux (ubuntu-latest). Windows-only code SHALL be behind `#[cfg(windows)]`.
2. **Dependencies:** only crates from the `tech.md` preferred list SHALL be added without asking, pinned exactly. Anything else needs the developer's approval first.
3. **Logging:** no logging of absolute user paths at `info` or above.
4. **Definition of done:** the four checks pass; README, SAFETY.md, architecture.md, CHANGELOG updated; final summary lists what changed, what was not done, and safety-relevant decisions.
5. **Execution constraints for this phase:** no deletes, moves, or writes outside the repo. Only `cargo` and basic `git` commands are run.

---

## Resolved decisions (approved 2026-10-22)

1. **Junction:** `cmd /C mklink /J` via `std::process::Command`, array args, both paths validated inside the tree (Req 9 #29).
2. **Sparse files:** `fsutil sparse setflag`; on failure record `skipped`. No `unsafe` code (Req 9 #32).
3. **RNG:** in-tree SplitMix64 + xoshiro256**, no new dependency (Req 9 #13).
4. **cargo-deny:** ubuntu only; the other four checks run on both OSes (Req 3 #3).
5. **Manifest totals:** kept (Req 9.5).
6. **Copyright holder:** `<COPYRIGHT HOLDER>` placeholder (Req 4 #1).
7. **Toolchain:** the developer installs rustup stable. MSRV = current stable at execution time (Req 1 #2).
8. **Roadmap:** NTFS fast path is Phase B7 everywhere (fixed in ROADMAP.md; verified).

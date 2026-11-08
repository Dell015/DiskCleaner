# Design: Phase B0 — Foundations

Implements `requirements.md` in this folder. Section numbers like (R9.13) point to requirement 9, criterion 13.

## 1. Overview

B0 delivers four things:

1. **Workspace skeleton:** 13 stub crates plus `tools/fixture-gen`, with shared metadata, pinned dependencies, and lints.
2. **Guardrails:** rustfmt, clippy (including a tripwire that bans destructive `std::fs` calls), cargo-deny, `.gitignore`, and CI.
3. **Documents:** LICENSE, README, SAFETY.md, `docs/architecture.md`, CHANGELOG.
4. **Code:** the `Risk` type in `dc-core`, and a deterministic fixture generator.

The only code that touches the filesystem is `fixture-gen`. It only creates new entries, never overwrites, and refuses unsafe output locations before it writes anything.

## 2. Repository layout after B0

```
DiskCleaner/
  Cargo.toml  Cargo.lock  rustfmt.toml  clippy.toml  deny.toml  .gitignore
  LICENSE  README.md  SAFETY.md  CHANGELOG.md  ROADMAP.md
  .github/workflows/ci.yml
  docs/architecture.md
  crates/
    dc-core/        src/{lib.rs, risk.rs, error.rs}  tests/{risk_props.rs, risk_serde.rs}  tests/snapshots/
    dc-protected/   src/lib.rs
    dc-scan/        src/lib.rs
    dc-mft/         src/lib.rs
    dc-winmeta/     src/lib.rs
    dc-attrib/      src/lib.rs
    dc-rules/       src/lib.rs
    dc-leftovers/   src/lib.rs
    dc-store/       src/lib.rs
    dc-advisor/     src/lib.rs
    dc-quarantine/  src/lib.rs
    dc-api/         src/lib.rs
    dc-cli/         src/main.rs        (binary `dc`)
  tools/fixture-gen/
    src/{main.rs, lib.rs, rng.rs, guard.rs, writer.rs, manifest.rs, platform.rs}
    src/layout/{mod.rs, steam.rs, browsers.rs, hoyoverse.rs, appdata.rs, temp.rs, leftover.rs, system32.rs, edge.rs, special.rs}
    tests/{determinism.rs, guard.rs, contents.rs, totals.rs}
```

`Cargo.lock` is committed because the workspace produces binaries. Nothing is created under `rules/` or `fixtures/` (both are out of scope; `fixtures/` is gitignored).

## 3. Workspace configuration

### 3.1 Root `Cargo.toml` (R1.1–R1.3)

```toml
[workspace]
resolver = "2"
members = ["crates/*", "tools/fixture-gen"]

[workspace.package]
version = "0.0.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/<owner>/DiskCleaner"   # filled from `git remote get-url origin`
rust-version = "<current stable>"                       # from `rustc --version` at execution (R1.2)
publish = false

[workspace.dependencies]          # exact pins, versions chosen at execution time
serde      = { version = "=x.y.z", features = ["derive"] }
serde_json = "=x.y.z"
thiserror  = "=x.y.z"
anyhow     = "=x.y.z"            # binaries only
clap       = { version = "=x.y.z", features = ["derive"] }
proptest   = "=x.y.z"
insta      = { version = "=x.y.z", features = ["json"] }
blake3     = "=x.y.z"
tempfile   = "=x.y.z"

[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "deny"
missing_docs = "warn"            # becomes an error under -D warnings

[workspace.lints.clippy]
dbg_macro = "deny"
todo = "deny"
unimplemented = "deny"
```

Every member has `version.workspace = true`, `edition.workspace = true`, `license.workspace = true`, `rust-version.workspace = true`, `publish.workspace = true`, and `[lints] workspace = true`.

Every dependency is on the `tech.md` preferred list. No other crates are added (NFR2).

### 3.2 Per-crate attributes

| Crate | Crate-root attributes |
|---|---|
| all except `dc-mft`, `dc-winmeta` | `#![forbid(unsafe_code)]` (R1.6) |
| `dc-core`, `dc-protected`, `dc-rules`, `dc-advisor`, `dc-quarantine` | `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` (R2.2, R8.9) |
| `dc-mft`, `dc-winmeta` | no `forbid`; no `unsafe` blocks in B0 (R1.7) |

Each stub `lib.rs` contains only a `//!` doc comment with the crate's ROADMAP §4 responsibility, a "Status: stub (Phase B0). Implemented in Phase Bx." line, "SAFETY-CRITICAL" where applicable (R1.8), and the attributes. Stubs have no dependencies (R1.9).

`dc-cli/src/main.rs` contains `fn main() { println!("dc {}", env!("CARGO_PKG_VERSION")); }` with no dependencies and no filesystem access (R1.4). `clap` is added to `dc-cli` in B1 when it gets subcommands.

### 3.3 `clippy.toml`: test allowances and destructive-API tripwire

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests  = true

# Safety rule 2: destructive std APIs are banned workspace-wide.
# Only dc-quarantine may locally #[allow(clippy::disallowed_methods)] (Phase B9).
disallowed-methods = [
  { path = "std::fs::remove_file",    reason = "destructive; only dc-quarantine (B9)" },
  { path = "std::fs::remove_dir",     reason = "destructive; only dc-quarantine (B9)" },
  { path = "std::fs::remove_dir_all", reason = "destructive; only dc-quarantine (B9)" },
  { path = "std::fs::rename",         reason = "destructive; only dc-quarantine (B9)" },
]
```

`disallowed_methods` is warn-by-default, so it becomes a hard error under `-D warnings`. R1.10 is then enforced on every clippy run. Win32 names (`DeleteFileW`, `RegSetValue`, ...) cannot appear yet because no crate depends on `windows`/`winreg` in B0. They will be added to `disallowed-methods` when B2 introduces those crates.

### 3.4 `rustfmt.toml` (R2.1)

Stable options only: `edition = "2021"`, `max_width = 100`, `use_field_init_shorthand = true`, `use_try_shorthand = true`. `newline_style` is left at the default (`Auto`) so Windows checkouts with `core.autocrlf` don't fail `fmt --check`.

### 3.5 `deny.toml` (R2.3), cargo-deny v2 schema

```toml
[advisories]
version = 2
yanked = "deny"                 # advisories (vulnerability, unsound) error by default in v2

[licenses]
version = 2
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
         "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]
confidence-threshold = 0.8
[licenses.private]
ignore = true                   # our own unpublished workspace crates

[bans]
multiple-versions = "warn"
wildcards = "deny"
allow-wildcard-paths = true     # workspace path deps

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

If a pinned dependency pulls in a transitive license not on the list (for example `CC0-1.0` or `MPL-2.0`), the task stops and asks rather than widening the list.

### 3.6 `.gitignore` (R2.4)

```
/target
/fixtures
*.db
```

## 4. CI (`.github/workflows/ci.yml`) (R3)

```yaml
name: ci
on: [push, pull_request]
permissions:
  contents: read
jobs:
  check:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@<sha>               # vX.Y.Z
      - uses: dtolnay/rust-toolchain@<sha>         # stable
        with: { toolchain: stable, components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@<sha>            # vX.Y.Z
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo build --workspace --all-targets
      - run: cargo test --workspace
  deny:
    runs-on: ubuntu-latest                         # R3.3: ubuntu only
    steps:
      - uses: actions/checkout@<sha>
      - uses: EmbarkStudios/cargo-deny-action@<sha>  # vX.Y.Z
        with: { command: check }
```

- **SHA pins (R3.5):** resolved at execution with `git ls-remote https://github.com/<action> refs/tags/<tag>`. This is a read-only basic git command. It is an outbound request that sends nothing from the repo. The tag goes in a trailing comment.
- No secrets and no `pull_request_target`. Read-only token (R3.6).
- `fixture-gen` runs only inside `cargo test`, writing under `target/` (R3.7). There is no separate CI step that runs the binary.
- Rust-cache is a convenience. It can be dropped if you prefer fewer third-party actions.

## 5. `dc-core` (R8)

### 5.1 API

```rust
// risk.rs
/// Deletion risk. Ordering is load-bearing: Safe < ProbablySafe < DontTouch.
/// DO NOT REORDER VARIANTS. `Ord` is derived from declaration order (tested).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
         Serialize, Deserialize, Default)]
pub enum Risk {
    Safe,
    ProbablySafe,
    #[default]
    DontTouch,               // default-deny (R8.3)
}

impl Risk {
    pub const ALL: [Risk; 3] = [Risk::Safe, Risk::ProbablySafe, Risk::DontTouch];

    /// The more cautious of two risks (= max). The advisor merge in B8 uses this.
    #[must_use]
    pub fn escalate(self, other: Risk) -> Risk { self.max(other) }

    /// Combine many risks. An empty input yields DontTouch (fail closed).
    #[must_use]
    pub fn escalate_all<I: IntoIterator<Item = Risk>>(risks: I) -> Risk {
        risks.into_iter().reduce(Risk::escalate).unwrap_or(Risk::DontTouch)
    }

    pub const fn as_str(self) -> &'static str { /* "Safe" | "ProbablySafe" | "DontTouch" */ }
}

impl fmt::Display for Risk { /* as_str */ }
impl FromStr for Risk { type Err = CoreError; /* exact, case-sensitive match */ }

// error.rs
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    #[error("invalid risk value: {value:?}")]
    InvalidRisk { value: String },
}

// lib.rs
/// Version of persisted / API-visible schemas. Bumped on breaking changes.
pub const SCHEMA_VERSION: u32 = 1;
```

`unwrap_or` is not `unwrap`, so there is no panic path (R8.9).

### 5.2 Design choices

- **Derived `Ord`** instead of a hand-written `cmp`: less code to get wrong. A unit test pins the order, so reordering variants fails the build.
- **Serde default representation** for unit variants is the variant-name string, and deserialization is exact and case-sensitive. `"safe"`, `"SAFE"`, `""`, `0`, and `null` all fail (R8.5). No `#[serde(other)]` or alias is used, because that would map unknown input to some variant.
- **`Default = DontTouch`** gives the default-deny rule (safety rule 3) a single source of truth.
- `escalate` instead of `max` as the public name makes the intent visible at call sites. `Ord::max` still works and is equivalent.

### 5.3 Tests (R8.11)

| File | Kind | Checks |
|---|---|---|
| `src/risk.rs` `#[cfg(test)]` | unit | order `Safe < ProbablySafe < DontTouch`; `ALL` sorted; `default() == DontTouch`; `escalate_all([]) == DontTouch`; `Display`/`FromStr` round-trip; `FromStr` rejects `"safe"`, `"Unknown"`, `""`, `" Safe"` |
| `tests/risk_props.rs` | proptest | commutative, associative, idempotent, `Safe` is identity, `escalate(a,b) >= a && >= b`, `escalate == max`; any non-empty vec containing `DontTouch` gives `DontTouch`; `escalate_all(v) == v.iter().max()` for non-empty `v` |
| `tests/risk_serde.rs` | serde + insta | JSON round-trip for `ALL`; `assert_json_snapshot!(Risk::ALL)` (snapshot committed); rejects mis-cased, unknown, numeric, and null JSON values |

Dependencies: `serde`, `thiserror`. Dev dependencies: `proptest`, `insta`, `serde_json`.

## 6. `tools/fixture-gen` (R9)

### 6.1 Structure

The package has a lib with a thin binary, so integration tests call the library directly.

```rust
// lib.rs
pub struct Options { pub out: PathBuf, pub seed: u64, pub file_count: u32 }
pub const DEFAULT_SEED: u64 = 0x4443_4230_0000_0001;   // "DCB0" + 1
pub const DEFAULT_FILE_COUNT: u32 = 10_000;

/// Validates `opts.out`, then generates `<out>/tree/` and `<out>/manifest.json`.
/// Returns the manifest. Never deletes, renames, or overwrites anything.
pub fn generate(opts: &Options) -> Result<Manifest, GenError>;

// main.rs: clap args `--out <DIR>` (required), `--seed <u64>`, `--file-count <N>`;
// calls generate(); prints a short summary; anyhow for exit code 1 on error.
```

Data flow:

```
Options ─▶ guard::validate_out ─▶ writer::Ctx (tree root, ledger, rng factory)
              │ (refuse: no writes)       │
              ▼                           ▼
           GenError        layout::{steam, browsers, hoyoverse, appdata, temp,
                                    leftover, system32, edge, special}  (fixed order)
                                          │
                                          ▼
                              ledger ─▶ manifest::build ─▶ <out>/manifest.json (create_new)
```

### 6.2 Output guard (`guard.rs`) (R9.4–R9.7)

`validate_out(path) -> Result<ValidatedOut, GenError>` runs before any write. Steps, all fail-closed:

1. Make the path absolute (`std::path::absolute`). Reject any `..` component in the input outright, so there is no lexical surprise.
2. If the path exists, take `symlink_metadata`. A symlink or junction gives `OutDirIsLink`, so we never write through a link. A non-directory gives `OutDirNotADirectory`. A non-empty directory (`read_dir().next().is_some()`) gives `OutDirNotEmpty`.
3. Canonicalize the path, or if it doesn't exist, its nearest existing ancestor with the remaining components appended. Failure gives `OutDirUnresolvable`.
4. If the canonical path is a filesystem root (no parent after stripping the `\\?\` / drive prefix: `C:\`, `\\?\C:\`, `\\server\share\`, `/`), return `OutDirIsRoot`.
5. Create `<out>` with `create_dir_all` if it is missing. Then create `<out>/tree` with `create_dir` (not `_all`), which fails if it already exists. This closes the gap between the check and the use.

Tests call `validate_out` directly for root and link cases. **No test ever calls `generate` with a root path**, so a guard bug can't write to a real drive root.

### 6.3 Writer and ledger (`writer.rs`)

- Every write goes through `Ctx`. Relative paths are `&[&str]` component lists, and each component is validated: non-empty, not `.` or `..`, no `/`, `\`, `:`, or `<>"|?*`, no NUL, no trailing dot or space, not a Windows reserved name (`CON`, `NUL`, `COM1`, ...). A bad component gives `GenError::InvalidName`. This is a bug in the layout, so generation aborts.
- `mkdir` uses `fs::create_dir`, which errors if the path exists.
- Files are opened with `OpenOptions::new().write(true).create_new(true)`. This is an atomic "must not exist" (R9.4). Content is streamed from the RNG in 64 KiB chunks. `File::set_modified` then sets a deterministic mtime (R9.14) before the handle closes.
- Hardlinks use `fs::hard_link`. Symlinks use `std::os::{unix,windows}::fs::symlink*`.
- `inside_tree(abs) -> bool` compares canonicalized prefixes. It is used for link targets and for every path passed to an external command (R9.33).
- The ledger records each created entry: kind (`Dir`, `File`, `Hardlink`, `Symlink`, `Junction`, `Sparse`), relative path, logical size, hardlink group, and status. Totals are computed from the ledger, which reflects what was actually created, so a `skipped` entry can't inflate them.

### 6.4 RNG (`rng.rs`) (R9.11–R9.13)

- `SplitMix64` expands a 64-bit seed into the 256-bit xoshiro state. `Xoshiro256StarStar` provides `next_u64`, `fill_bytes`, and `range(lo, hi)` (Lemire's unbiased bounded method).
- **Independent per-category streams:** `rng_for(category) = Xoshiro::from_seed(seed ^ fnv1a64(category))`. Each layout module gets its own stream, so a later edit to one category doesn't shift every other category's content.
- Little-endian byte order is explicit, so output is identical on all platforms.
- Tests pin the first outputs for seed 0 against the published reference values for SplitMix64 and xoshiro256**.

### 6.5 Determinism rules (R9.11–R9.15)

- Layout modules run in a fixed order. Within a module, entries are created from static tables. Nothing depends on `read_dir` order, the clock, environment variables, the host name, or the locale.
- mtimes: `rng.range(2015-01-01, 2024-12-31)` as Unix seconds, applied to files only. Directory mtimes change when children are added and are not asserted.
- Manifest entries are sorted by path, and JSON is written by `serde_json::to_vec_pretty` from structs and `Vec`s (no `HashMap`), plus a trailing newline. Absolute paths and timestamps never appear in the manifest.
- Outputs are identical across runs on the same OS. Across OSes, only special-entry `status` and `reason` may differ (for example, junctions are `skipped` on Linux).

### 6.6 Tree layout (R9.16–R9.32)

All names are fictional except the vendor and app directory names that ROADMAP requires. The fake user is `FixtureUser`, and no real PII is used. Sizes are drawn from the given ranges. With defaults, about 60–80 MiB is written plus a 64 MiB sparse file. Exact numbers come from the manifest.

```
tree/
├─ Program Files (x86)/Steam/                                   (R9.16)
│  ├─ steam.exe, steamclient.dll, Steam.dll
│  └─ steamapps/
│     ├─ appmanifest_900001.acf, appmanifest_900002.acf       VDF text: appid, name, installdir, SizeOnDisk
│     ├─ common/FakeGameAlpha/{Alpha.exe, data/*.pak ×6 (1–4 MiB), bin/*.dll}
│     ├─ common/FakeGameBeta/{Beta.exe, content/*.bin ×8 (256 KiB–2 MiB)}
│     ├─ shadercache/900001/*.bin
│     └─ downloading/900002/*.tmp
├─ Program Files/
│  ├─ Google/Chrome/Application/{chrome.exe, <ver>/chrome.dll}   (R9.17)
│  ├─ BraveSoftware/Brave-Browser/Application/{brave.exe, <ver>/chrome.dll}  (R9.18)
│  └─ HoYoPlay/{launcher.exe, games/FakeHoyoGame/{FakeHoyoGame.exe, Data/*.blk ×6 (2–6 MiB)}}  (R9.19)
├─ ProgramData/DefunctApp/{config.ini, logs/*.log}               (R9.22)
├─ Users/FixtureUser/AppData/
│  ├─ Local/Google/Chrome/User Data/Default/{Cache/Cache_Data/f_*, Code Cache/js/*, GPUCache/*, Bookmarks, History, Preferences}
│  ├─ Local/BraveSoftware/Brave-Browser/User Data/Default/{same shape}
│  ├─ Local/SomeApp/Cache/*,  Local/CrashDumps/*.dmp,  Local/SomeApp/logs/*.log   (R9.20)
│  ├─ Local/Temp/{*.tmp, ~DF*.tmp, random-named dirs}            (R9.21)
│  ├─ Roaming/SomeApp/{settings.json, Cache/*}
│  ├─ Roaming/DefunctApp/{settings.xml, cache/*}                 (R9.22)
│  └─ LocalLow/miHoYo/FakeHoyoGame/{output_log.txt, cache/*}     (R9.19)
├─ Windows/
│  ├─ System32/{kernel32.dll, ntdll.dll, cmd.exe, drivers/*.sys, config/{SYSTEM, SOFTWARE}}  (R9.23)
│  └─ Temp/{*.tmp, *.log}
├─ edge/
│  ├─ deep/d00/d01/…/d63/bottom.txt                              (R9.24, 64 levels)
│  ├─ many_files/f000000.bin … (file_count files, 0–512 B)       (R9.25)
│  ├─ zero/{a,b,c}/*.empty ×10+, zero/only_zero/zero.dat, empty_dir/   (R9.26)
│  ├─ unicode/{日本語フォルダ, Кириллица.txt, ملف_عربي.txt, 📁emoji🎮/😀.bin,
│  │           e\u{301}combining.txt, "name with spaces.txt", .leading_dot}   (R9.27)
│  └─ long/<5 × 60-char dirs>/<255-char file name>               (R9.28, > 260 chars total)
└─ links/                                                       (R9.29–R9.33)
   ├─ junction_target/payload.bin
   ├─ junction_link        → junction_target   (Windows: mklink /J)
   ├─ symlink_target_file.txt, symlink_target_dir/inner.txt
   ├─ symlink_file         → symlink_target_file.txt
   ├─ symlink_dir          → symlink_target_dir
   ├─ hardlink_a/shared.bin   ┐ same file
   ├─ hardlink_b/shared.bin   ┘ (fs::hard_link)
   └─ sparse/sparse_64MiB.bin (logical 64 MiB, 4 KiB written at offset 32 MiB)
```

Dummy content (R9.9): binary files are RNG bytes, and text files (VDF, ini, json, log) are short deterministic marker text that includes the line `DiskCleaner fixture - dummy data`. RNG bytes can't form a valid PE file in practice, but to be certain every `.exe`, `.dll`, and `.sys` file starts with the bytes `DCFX` instead of `MZ`.

Long paths (R9.28): the generator always uses absolute paths, and Rust std on Windows adds the `\\?\` prefix automatically for paths over `MAX_PATH`. A test asserts that the file exists and that its absolute path is over 260 characters.

### 6.7 Platform-specific special entries (`platform.rs`) (R9.10, R9.29–R9.32)

| Entry | Windows | Linux | On failure |
|---|---|---|---|
| Junction | `cmd.exe /C mklink /J <link> <target>` | — | `skipped` (Linux: "not applicable") |
| Symlink file/dir | `std::os::windows::fs::symlink_file/dir` | `std::os::unix::fs::symlink` | `ERROR_PRIVILEGE_NOT_HELD` (1314) gives `skipped: "symlink privilege not held"`; any other error aborts |
| Hardlink | `fs::hard_link` | `fs::hard_link` | error aborts (both are supported on NTFS and ext4) |
| Sparse | create file with `create_new`, then `fsutil.exe sparse setflag <file>`, then `set_len(64 MiB)`, then write 4 KiB at 32 MiB | `create_new`, `set_len`, write at offset (ext4/tmpfs make it sparse naturally) | `fsutil` non-zero exit gives `skipped`. The already-created file stays at 0 bytes and is recorded as an ordinary zero-byte file. It is never deleted and never filled to 64 MiB. |

**External command hardening** (approved decisions 1 and 2):
- The executable is an absolute path: `%SystemRoot%\System32\cmd.exe` and `%SystemRoot%\System32\fsutil.exe`. `SystemRoot` is read only to locate the binary, and it doesn't affect output. If it is unset, or the file doesn't exist, the entry is `skipped`. There is no PATH or current-directory lookup.
- Arguments are passed with `Command::args([...])`, one element each, never built into a string.
- Before spawning, both paths must pass `inside_tree()`. A failure here means the generator has a bug, so it aborts with `PathEscapesTree`.
- `cmd.exe` has its own quoting rules, so junction paths must also match a conservative character set (`[A-Za-z0-9 _\-.:\\]`). If the absolute base path contains anything else (for example `&`, `%`, `^`, `!`, `"`), the junction is `skipped` with reason "path not safe for cmd.exe". It is never escaped and run.
- stdout and stderr are captured and discarded. Only the exit status is used. `fsutil` may need elevation on some systems, which isn't verified. If it does, the result is `skipped`, which is the approved behavior.
- After creation, the junction is verified with `fs::read_link(link)` resolving to the target. If the junction can't be verified, it is recorded as `skipped` with reason "created but not verifiable", so it isn't silently treated as a real directory. It is not deleted.

### 6.8 Manifest (`manifest.rs`) (R9.34–R9.38)

```json
{
  "schema_version": 1,
  "generator": { "name": "fixture-gen", "version": "0.0.0" },
  "options": { "seed": "<DEFAULT_SEED as u64>", "file_count": 10000 },
  "counting_rules": "Regular files counted by logical size. Hardlink paths each count as a file path; unique_* counts each inode once. Symlinks and junctions are entries of 0 bytes and are not followed. Sparse files are counted by logical size. The tree root is not counted as a dir.",
  "totals": {
    "dirs": 0, "file_paths": 0, "unique_files": 0, "links": 0,
    "logical_bytes_all_paths": 0, "logical_bytes_unique": 0
  },
  "top_level": [
    { "path": "Program Files", "dirs": 0, "file_paths": 0, "logical_bytes_all_paths": 0 }
  ],
  "categories": [
    { "name": "steam", "paths": ["Program Files (x86)/Steam/steam.exe"] }
  ],
  "special": [
    { "kind": "junction", "path": "links/junction_link", "target": "links/junction_target",
      "status": "created", "reason": null }
  ]
}
```

- `paths` lists every entry for small categories. For `many_files` it lists the directory plus a count field, which keeps the manifest small.
- Paths are relative to `tree/` and use `/` separators (R9.38). They are produced from the component lists, never from OS path strings.
- The manifest is written with `create_new` as the last step. If it exists, the generation completed.

### 6.9 Errors

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GenError {
    OutDirNotEmpty(PathBuf), OutDirIsRoot(PathBuf), OutDirIsLink(PathBuf),
    OutDirNotADirectory(PathBuf), OutDirUnresolvable(PathBuf),
    InvalidName(String), PathEscapesTree(String),
    Io { context: &'static str, path: String, source: std::io::Error },
}
```

Skippable failures are not errors. They become manifest `special` entries with `status: "skipped"`. Everything else aborts with a non-zero exit (R9.10). An aborted run leaves a partial tree without `manifest.json`, and nothing is cleaned up, since cleanup would mean deleting.

### 6.10 Tests (R9.39–R9.41)

All integration tests use `tempfile::Builder::new().prefix("fixturegen-").tempdir_in(env!("CARGO_TARGET_TMPDIR"))`. That directory is under `target/` in this repo, never `%TEMP%`, a user directory, or `fixtures/`. `tempfile` removes the directory when the test ends. That removal is the only deletion in B0: it is library code, runs under `target/`, and is not caught by the clippy tripwire because it isn't our call. **Flagged for your review.**

Most tests use `file_count = 200` for speed. One test runs the default 10,000 and asserts the runtime is under 60 s (R9.25).

| Test file | Covers |
|---|---|
| `src/rng.rs` unit | reference vectors for SplitMix64 and xoshiro256**; `range` bounds; per-category streams differ |
| `src/writer.rs` unit | the name validator rejects `..`, `CON`, trailing dot or space, separators, NUL |
| `tests/guard.rs` | non-empty dir refused, and the dir is unchanged afterwards (BLAKE3 over relative paths and contents, before and after); existing file refused; symlink or junction as `--out` refused (skipped where it can't be created); `validate_out` on `/`, `C:\`, `\\?\C:\` returns `OutDirIsRoot` with **no `generate` call**; a path containing `..` is refused |
| `tests/determinism.rs` | two runs with the same seed give equal relative paths, sizes, and BLAKE3 hashes, plus byte-equal manifests; a different seed gives different hashes; mtimes match between runs |
| `tests/contents.rs` | every category in R9.16–R9.32 is present; deep chain depth is 64 or more; long path over 260 and a 255-char component; every unicode name exists; zero-byte count is 10 or more; empty dir exists; `.exe`/`.dll`/`.sys` don't start with `MZ`; special entries are `created` where capable, otherwise `skipped` with a non-empty reason (R9.41, never `#[ignore]`); hardlink pair shares an inode on Unix (`dev`, `ino`); on Windows content is equal (stable std can't read file IDs, so B1 verifies this with the scanner) |
| `tests/totals.rs` | an independent recount (`symlink_metadata` walk, links not followed, hardlinks deduped by `(dev, ino)` on Unix, by manifest group on Windows) equals manifest totals and per-top-level values |

## 7. Documents

| File | Content |
|---|---|
| `LICENSE` | Standard MIT text, `Copyright (c) 2026 <COPYRIGHT HOLDER>` (R4.1) |
| `README.md` | Name and one-line pitch; the 3 goals; **Status: pre-alpha, backend only, read-only, no deletion capability**; **No telemetry, ever**; build/test commands (the four checks); `cargo run -p fixture-gen -- --out fixtures/sample`; workspace overview linking to architecture.md; links to SAFETY.md and ROADMAP.md; license (R4.2–R4.3) |
| `SAFETY.md` | One section per principle (ROADMAP §5 items 1–10 merged with the 12 `safety.md` hard rules, deduplicated). Each has a status table: **Enforced (B0)** for `Risk` order, default, and escalate (tests), `forbid(unsafe_code)`, no unwrap/expect/panic lint in safety crates, the destructive-`std::fs` clippy tripwire, fixture-gen create-only; **Planned (Bx)** for everything else. Includes a "Reporting a safety issue" line (R5) |
| `docs/architecture.md` | Layout tree; crate table (responsibility, phase, safety-critical, `unsafe` allowed); dependency diagram (text/mermaid) from `structure.md`; risk model; the ROADMAP §7 data-model sketch marked *planned*; fixture-gen overview. Every non-B0 component is marked "not yet implemented" (R6) |
| `CHANGELOG.md` | Keep a Changelog format; `## [Unreleased]`, `### Added` with the B0 items (R7) |

## 8. Requirement traceability

| Req | Design section | Verified by |
|---|---|---|
| R1 | 3.1, 3.2 | `cargo build`, clippy (forbid attributes), file review |
| R1.10 | 3.3 | clippy `disallowed_methods` under `-D warnings` |
| R2 | 3.3–3.6 | the four checks |
| R3 | 4 | CI run on GitHub (you check this; it can't be verified locally) |
| R4–R7 | 7 | review |
| R8 | 5 | `dc-core` unit, proptest, and insta tests |
| R9 | 6 | `fixture-gen` unit and integration tests; your acceptance run |

## 9. Risks and limitations

- **Windows symlink privilege:** on a machine without Developer Mode, symlinks are `skipped` locally, but they are `created` on GitHub runners, which run as admin. Tests accept either.
- **`fsutil` behavior without elevation** is not verified. The fallback is `skipped` as approved, so local and CI results may differ.
- **Junction detection via `fs::read_link`** is expected to work for mount-point reparse points in std, but this is unverified until execution. The fallback is `skipped: created but not verifiable`.
- **Hardlink identity on Windows** can't be checked with stable std without `unsafe`, so B1 covers it.
- **Third-party actions** (checkout, rust-toolchain, rust-cache, cargo-deny-action) are pinned by SHA, but you still need to trust them. rust-cache is optional.
- Exact dependency versions and action SHAs are resolved at execution time, so the design doesn't contain them.

## 10. Not in this design (by requirement)

No scanner, `Node`, or `Tree`; no protected-path logic; no rules or `rules/`; no `--i-know` flag; no fixture hash test for scans; no `destructive-actions` feature; no extra docs; no `clap` in `dc-cli` yet; no Win32 dependencies.

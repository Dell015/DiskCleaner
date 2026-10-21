# Tech stack and conventions

## Language and tooling
- Rust stable, edition 2021, Cargo workspace. Windows 11 primary; core crates must also build and test on Linux (use fixture-based tests and `#[cfg(windows)]` for Win32 code behind traits).
- Formatting: `cargo fmt --all`. Lints: `cargo clippy --workspace --all-targets -- -D warnings`. Tests: `cargo test --workspace`. Dependency policy: `cargo deny check`.
- A task is not done until all four pass.

## Preferred crates
`rayon`, `crossbeam-channel`, `windows` (official Microsoft crate), `winreg`, `serde`/`serde_json`, `toml`, `rusqlite` (bundled), `thiserror` (libraries), `anyhow` (binaries only), `tracing`, `clap`, `reqwest` (rustls; only in `dc-advisor`), `proptest`, `criterion`, `tempfile`, `insta`, `blake3`, `schemars` + `ts-rs`/`specta` (API types for the future frontend).
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

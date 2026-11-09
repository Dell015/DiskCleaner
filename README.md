# DiskCleaner

An open-source, Windows-first disk analyzer and cleanup advisor: fast scanning, a map of
**which app owns which files and why**, detection of leftovers from uninstalled apps, and an
optional metadata-only AI explanation layer.

> **Status: early development (Phase B1, backend only).** A read-only command-line scanner exists; there is no UI yet.
> The tool is **read-only**: it never deletes, moves or modifies anything until a later,
> clearly separated phase, and that phase will be off by default. See [SAFETY.md](SAFETY.md).

## Goals
1. Free disk space.
2. Find leftovers from uninstalled apps.
3. Show what is using space, and why (app → executables → folders → files).

## Layout
See [docs/architecture.md](docs/architecture.md) and [ROADMAP.md](ROADMAP.md).

## Try it (read-only)
```
cargo run -p dc-cli -- scan <folder> --out scan.json
cargo run -p dc-cli -- top scan.json --dirs
```
The scan file must not exist yet; nothing in the scanned folder is ever changed.

## Developing
You need Rust (via [rustup](https://rustup.rs)); on Windows also the Visual Studio C++ Build Tools.

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Dummy test data
Tests and experiments must never touch real files. Generate a deterministic fake tree:

```
cargo run -p fixture-gen -- --out fixtures\sample
```

The output directory must be new or empty. The generator only creates files; it never deletes.

## License
MIT. See [LICENSE](LICENSE).

## Project layout

```
crates/         Rust crates (scanner, rules, leftovers, CLI `dc`, ...), one flat folder each
rules/          rule packs as data: junk/ cleanup rules, apps/ well-known apps
apps/desktop/   graph view of the disk (web app; desktop shell later)
tools/          test helpers (fixture-gen)
docs/           architecture, decisions, notes, per-phase plans (start at docs/README.md)
data/           your own scans and view files; ignored by git
```

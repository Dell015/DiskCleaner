# Architecture

Rust Cargo workspace. Backend only for now; a UI will later wrap `dc-api`.

| Crate | Responsibility |
|---|---|
| `dc-core` | Shared types (`Risk`, errors, schema version) |
| `dc-protected` | Compiled-in protected-path engine (safety-critical) |
| `dc-scan` | Directory walker, aggregation, progress/cancel |
| `dc-mft` | NTFS fast path and elevated helper protocol |
| `dc-winmeta` | Registry, shortcuts, known folders, version info |
| `dc-attrib` | App → executable → folder → file graph |
| `dc-rules` | TOML rule packs, validation, evaluation (safety-critical) |
| `dc-leftovers` | Leftovers of uninstalled apps |
| `dc-store` | SQLite persistence, snapshots, diffs |
| `dc-advisor` | Metadata-only AI layer, escalate-only (safety-critical) |
| `dc-quarantine` | Quarantine/restore, feature-gated (safety-critical) |
| `dc-api` | Facade for the future frontend |
| `dc-cli` | Binary `dc` |
| `tools/fixture-gen` | Deterministic dummy-data generator for tests |

Dependency direction (no cycles): `dc-core` ← everything; `dc-protected` ← `dc-rules`, `dc-quarantine`;
`dc-scan` + `dc-winmeta` ← `dc-attrib` ← `dc-leftovers`; `dc-rules` + `dc-attrib` ← `dc-advisor`;
everything ← `dc-api` ← `dc-cli`.

Phases are described in [ROADMAP.md](../ROADMAP.md); per-phase requirements and design live in `docs/phases/`.

## Folders

```
DiskCleaner/
  Cargo.toml          virtual workspace manifest: members, pinned dependencies, lints
  crates/             every Rust crate, flat, folder name = crate name
  rules/              rule packs (data): junk/ cleanup rules, apps/ well-known apps
  apps/desktop/       graph view (Vite + TypeScript + sigma.js); Tauri shell goes in src-tauri/ later
  tools/fixture-gen/  dummy-data generator for tests
  docs/               architecture, decisions, notes, per-phase plans
  data/               your own scans and view files (ignored by git)
  fixtures/           generated test trees (ignored by git)
  .github/workflows/  CI
```

Why this shape: one flat `crates/` folder with the folder named after the crate (easy to find,
nothing nested inside another crate); the root is a virtual manifest; internal crates are declared
once in `[workspace.dependencies]`; non-Rust apps live under `apps/`; data that changes without code
(rule packs) lives in `rules/`; private output never sits next to source code.

# Architecture

Rust Cargo workspace plus the desktop app (`apps/desktop`), which talks to the backend through `dc-api`.

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
| `dc-quarantine` | Removing (holding folder, restore, empty), uninstalling through an app's own uninstaller, moving an app to another drive (`mover`); feature-gated (safety-critical) |
| `dc-api` | What the app needs: live scans with events, the shared checks, the view format, what may be removed (`cleanup`) or moved (`moving`), the AI connection (`mcp`) |
| `dc-cli` | Binary `dc` |
| `tools/fixture-gen` | Deterministic dummy-data generator for tests |

Dependency direction (no cycles): `dc-core` ← everything; `dc-protected` ← `dc-rules`, `dc-quarantine`;
`dc-scan` + `dc-winmeta` ← `dc-attrib` ← `dc-leftovers`; `dc-rules` + `dc-attrib` ← `dc-advisor`;
everything ← `dc-api` ← `dc-cli` and `dc-desktop` (the window).

Phases are described in [ROADMAP.md](../ROADMAP.md); per-phase requirements and design live in `docs/phases/`.

## Folders

```
DiskCleaner/
  Cargo.toml          virtual workspace manifest: members, pinned dependencies, lints
  crates/             every Rust crate, flat, folder name = crate name
  rules/              rule packs (data): junk/ cleanup rules, apps/ well-known apps
  apps/desktop/       graph view (Vite + TypeScript + sigma.js); src-tauri/ is the desktop window (crate dc-desktop)
  tools/fixture-gen/  dummy-data generator for tests
  tools/check-all.ps1 every check in one command, on Windows
  docs/               architecture, decisions, notes, per-phase plans, review/ (start a review here)
  data/               your own scans and view files (ignored by git)
  fixtures/           generated test trees (ignored by git)
  .github/workflows/  CI
```

Why this shape: one flat `crates/` folder with the folder named after the crate (easy to find,
nothing nested inside another crate); the root is a virtual manifest; internal crates are declared
once in `[workspace.dependencies]`; non-Rust apps live under `apps/`; data that changes without code
(rule packs) lives in `rules/`; private output never sits next to source code.

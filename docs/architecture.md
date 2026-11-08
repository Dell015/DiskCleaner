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

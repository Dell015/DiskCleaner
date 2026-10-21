# Repository structure

```
DiskCleaner/
  Cargo.toml  deny.toml  rustfmt.toml  clippy.toml  LICENSE  README.md  SAFETY.md  ROADMAP.md  CHANGELOG.md
  .kiro/steering/      (these files)
  .github/workflows/ci.yml
  docs/                architecture.md, threat-model.md, rules-guide.md, api.md, mft-design.md, perf.md
  rules/               protected.toml, apps/*.toml, junk/*.toml
  tools/fixture-gen/   deterministic dummy-data generator
  fixtures/            generated, gitignored; tests only
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
- Specs live in `.kiro/specs/<phase-name>/`. One spec per roadmap phase (b0-foundations, b1-scanner, b2-winmeta, b3-attribution, b4-rules, b5-leftovers, b6-store, b7-mft, b8-advisor, b9-quarantine, b10-api).
- Rule packs are data (`rules/**/*.toml`), not code. Prefer adding a rule over adding a special case in Rust.

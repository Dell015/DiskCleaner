# Changelog

## Unreleased
### Phase B1 - Scanner v1
- `dc-core`: `Node`, `Kind`, `Flags`, `Category`, arena `Tree` with bottom-up aggregation and pruning, top-N / by-extension / by-category / oldest queries, `ScanResult` (stored form), property tests, 1M-node criterion benchmark.
- `dc-scan`: read-only parallel walker (rayon). Links never followed, hardlinks counted once (deterministic: first name wins), errors collected without aborting, cancel token, throttled progress, `--max-depth` / `--min-size` filters that keep totals exact, drive roots need explicit permission. `FileProbe` trait keeps the crate free of `unsafe`.
- `dc-winmeta`: `WindowsProbe` (file identity, link count and real allocated size via attribute-only handles; never reads file data, never hydrates cloud files).
- `dc-cli`: `dc scan <path> --out scan.json` and `dc top <scan.json> --files|--dirs|--ext|--category|--oldest`. Never overwrites the output file. Exit code 2 = scan finished but partial.

### Phase B0 – Foundations
- Cargo workspace with stub crates for every planned component.
- `dc-core`: `Risk` enum (`Safe < ProbablySafe < DontTouch`, default-deny, escalate-only) with property and snapshot tests.
- `tools/fixture-gen`: deterministic, create-only dummy file-tree generator with output guard and manifest.
- CI (fmt, clippy, build, test on Windows and Linux; cargo-deny).

# Changelog

## Unreleased
### Phase B0 – Foundations
- Cargo workspace with stub crates for every planned component.
- `dc-core`: `Risk` enum (`Safe < ProbablySafe < DontTouch`, default-deny, escalate-only) with property and snapshot tests.
- `tools/fixture-gen`: deterministic, create-only dummy file-tree generator with output guard and manifest.
- CI (fmt, clippy, build, test on Windows and Linux; cargo-deny).

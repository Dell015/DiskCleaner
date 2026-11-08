# Changelog

## Unreleased
### Phase B5 - Leftovers
- `dc-leftovers`: finds folders that apps left behind, with evidence and confidence (points: High >= 80, Medium >= 50). Four kinds: data of an app whose program folder is gone, backup-named copies (`X.old`), well-known apps that are not installed (built-in list of 31, validated data file), program files nobody registered. Never `Safe`; weak evidence, personal-looking data, games launchers and protected/Windows/Microsoft/linked folders are shown only or refused. Second look at the real disk (`verify`), `explain`.
- `dc-cli`: `dc leftovers scan.json --apps apps.json [--limit] [--explain PATH] [--out FILE] [--timing]`.
- `dc-rules`: `child_lists` and `folder_blocker` are public; candidates are serializable. `dc-attrib`: `container_kind` is public.
### Performance (classify)
- Dev profile optimized (D68 rule: speed never at the cost of correctness), allocation-free wildcard matching, parallel candidate search, one owner-lookup index, `--timing`.
### Phase B4b - Rules and classification
- `dc-rules`: TOML rule packs (validated on load, examples required), built-in pack v1 (about 45 rules), classifier (protected list first, highest risk wins, unknown = DontTouch), candidate finder with folder checks and a second check against the real file system.
- `dc-protected`: `$WINDOWS.~BT` / `$WINDOWS.~WS` are no longer protected (decision D63); `Pattern` is public.
- `dc-cli`: `dc classify scan.json [--apps] [--risk] [--limit] [--explain PATH]`, `dc rules --list|--check FILE`.
### Phase B3 - Attribution (slice 1)
- `dc-attrib`: who owns which folder, with evidence and confidence. Install folder, main-program folder, Windows areas, exact-name data folders, publisher (vendor) folders; ties are ambiguous. Every byte lands in one bucket (owner / shared / unattributed).
- `dc-cli`: `dc attrib scan.json --apps apps.json [--out] [--limit] [--explain PATH]`.
### Phase B4a - Protected paths
- `dc-protected`: path normalization (text only), built-in protected list as data, check against both the typed path and the OS-resolved path (links, junctions, 8.3 names), fail closed on anything odd, runtime additions that can only add protection. `Verdict` has no "safe" value.
- `dc-cli`: `dc protected <path>...` and `dc protected --list`.
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

- `dc leftovers` now takes several scan files and merges the results (`dc_leftovers::merge`), so ProgramData and Program Files can be covered.

# Phase B1: Scanner v1 - Requirements

Source: `ROADMAP.md` Phase B1, `CLAUDE.md`, `SAFETY.md`. Status: **draft for review**.

**Goal:** read-only scan of one folder tree, producing a JSON tree with sizes, plus simple top-N reports.
**Priority order:** correct sizes and link handling first, speed second. Benchmarks are added, not optimized for.

## R1. Data model (`dc-core`)
1. `Node`: id, parent, name, kind (`File | Dir | Link | Other`), logical size, size on disk, mtime/ctime/atime, attribute flags, extension, depth.
2. Flags: reparse, hardlink, cloud (offline/recall), sparse, compressed, system, hidden, readonly.
3. `Tree` is an arena: `Vec<Node>` with index ids. No `Rc`, `RefCell` or `Box` graphs.
4. Aggregates per directory: total logical size, total size on disk, file count, dir count, child count. Computed after the scan, bottom-up.
5. Everything derives `Serialize`/`Deserialize`; the persisted scan carries `schema_version`.
6. Category enum (video, image, audio, archive, code, document, game asset, installer, executable, other) derived from the extension by a pure function with a table test.

## R2. Walker (`dc-scan`)
1. Walks one root, in parallel (rayon). **Never writes, renames or deletes.** Files are only queried for metadata; contents are never read.
2. Long paths work (`\\?\` prefix on Windows) and non-UTF-8 names do not crash it (names kept as lossy text for display, original kept for reopening).
3. **Links are never followed.** Junctions, symlinks and any other reparse point are recorded as `Link` (with their target text where readable) and contribute 0 bytes to totals.
4. **Hardlinks are counted once.** Detected by (volume serial, file index). The first path seen carries the size; later paths are marked `hardlink` with size counted as 0 in aggregates, and the count of extra paths is reported.
5. Errors (access denied, vanished file, name too long, I/O error) are collected in a list with path, operation and kind; the scan continues. The result states clearly that it is partial when errors occurred.
6. Size on disk: allocated size rounded to cluster size; compressed and sparse files use the real allocated size. Cloud placeholders (offline/recall-on-open) are flagged and not hydrated: touching their content is forbidden.
7. Cancellation: a cancel token stops the scan promptly and returns the partial tree, marked cancelled.
8. Progress events (files seen, bytes, current directory), throttled (default at most 10 per second), delivered through a channel/callback.
9. Options: `max_depth` and `min_size` filter what is *stored*, but aggregates still include everything that was scanned (so totals stay correct).
10. A scan of a drive root requires an explicit flag (`--i-know`), per CLAUDE.md safety rule 8.

## R3. Queries
Top-N largest files, largest directories, totals by extension and by category, oldest/newest by mtime. Pure functions over `Tree`, tested on small hand-built trees.

## R4. CLI (`dc-cli`)
- `dc scan <path> --out scan.json [--max-depth N] [--min-size SIZE] [--i-know]`
- `dc top <scan.json> --files | --dirs | --ext [--limit N]`
- Human-readable output by default; machine output (JSON) for `scan`. Clear exit codes: 0 ok, 1 error, 2 partial (errors collected).

## R5. Tests (all under temp dirs or generated fixtures, never real data)
1. Totals for `fixture-gen` output equal its `manifest.json` (dirs, file paths, unique files, logical bytes unique, links).
2. Link handling: junction/symlink not followed; hardlink pair counted once; link count matches manifest.
3. "Scan does not modify fixtures": hash/list the tree before and after, assert identical.
4. Cancel: a cancelled scan returns promptly with a partial, flagged result.
5. Errors: an unreadable directory is reported and the scan continues (Unix permission test; Windows ACL test if feasible, else skipped with a reason).
6. Property tests: parent sizes equal the sum of children; no cycles; ids are valid indices.
7. Snapshot (`insta`) of the JSON for a small fixed tree.
8. `criterion` benchmark on a synthetic 1M-node tree (build + aggregate + top-N). Not part of `cargo test`.
9. CI stays green on Windows and Linux. Windows-only probes sit behind a trait with a portable fake.

## R6. Non-goals (not in B1)
App attribution, rules, risk classification, protected paths, persistence/diffing, MFT fast path, any UI, any deletion.

## R7. Definition of done
fmt, clippy `-D warnings`, `cargo test --workspace` pass on Windows and Linux; CHANGELOG updated; final summary lists what changed, what was not done, and safety decisions.

## Acceptance check (you)
1. `dc scan fixtures\sample --out sample.json` matches the manifest's totals.
2. Scan one small real folder (a project) and compare with Explorer Properties and WizTree.

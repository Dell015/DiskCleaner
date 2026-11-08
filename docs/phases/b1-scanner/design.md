# Phase B1: Scanner v1 - Design

Status: **implemented**. Decisions: all defaults approved. Implements `requirements.md` in this folder.

## 1. Where code goes
```
dc-core     Node, Kind, Flags, Category, Tree (arena), aggregates, queries (top-N), ScanResult (serialized form)
dc-scan     walker, options, cancel token, progress, error list; defines trait FileProbe
dc-winmeta  Windows implementation of FileProbe (the only place with unsafe in B1)
dc-cli      wires dc-scan + the right FileProbe; commands scan/top
```
Dependency direction stays acyclic: `dc-scan` does **not** depend on `dc-winmeta`. `dc-scan` defines the small trait; `dc-cli` picks the implementation. This keeps `dc-scan` `#![forbid(unsafe_code)]`.

## 2. The FileProbe trait
Everything std cannot give us portably goes through one trait:
```
trait FileProbe: Sync {
    /// Identity for hardlink dedupe, allocated size, cloud/sparse/compressed flags, reparse info.
    fn probe(&self, path: &Path, meta: &Metadata) -> Result<Probe, ProbeError>;
    fn cluster_size(&self, root: &Path) -> Option<u64>;
}
struct Probe { file_id: Option<(u64 /*volume*/, u128 /*index*/)>, link_count: u32,
               size_on_disk: u64, flags: Flags, link_target: Option<String> }
```
- **Windows impl (`dc-winmeta`)**: opens the file with `FILE_READ_ATTRIBUTES` only (no data access, `FILE_FLAG_OPEN_REPARSE_POINT` so links and placeholders are not followed or recalled), `GetFileInformationByHandle` for volume serial/index/link count, `GetCompressedFileSizeW` for allocated size, `GetDiskFreeSpaceW` for cluster size. Each `unsafe` block gets a `// SAFETY:` comment.
- **Unix impl (in `dc-scan`, safe)**: `MetadataExt` (`dev`, `ino`, `nlink`, `blocks * 512`), used by CI on Linux so the core logic is tested there.
- **Fake impl (tests)**: scripted answers, so hardlink/sparse/cloud logic is tested without special files.

## 3. Walking
- Directory listing with `std::fs::read_dir`; metadata with `symlink_metadata` (never `metadata`) so links are not followed.
- Parallelism: rayon `scope`; each directory is a task that returns its children. Workers append to per-task vectors (no shared lock on the hot path); results are merged into the arena by a single thread, so ids are deterministic after a final sort by path.
- Hardlink dedupe: a concurrent set of `(volume, index)`; first insert wins. Because "first" depends on thread timing, the *sorted-path-first* instance is chosen in the merge step so output is deterministic.
- Cancellation: `AtomicBool` checked before each directory and every N entries.
- Progress: atomic counters sampled by a throttling thread (not by workers).
- Errors: `ScanError { path, op, kind, message }` pushed to a mutex-guarded vec (cold path).

## 4. Sizes
- `logical_size` = file length. `size_on_disk` from the probe (cluster-rounded or real allocated size). Directories have 0 own size; aggregates are separate fields.
- Links: logical 0, on-disk 0, `kind = Link`, flag `reparse`.
- Hardlink duplicates: stored with their own logical size shown, flag `hardlink`, but excluded from aggregates (`counted = false`).
- Cloud placeholders: flagged `cloud`; sizes taken from metadata only.
- Aggregation is a bottom-up pass over the arena (children always have higher ids than parents after the merge sort, so one reverse loop suffices).

## 5. Output format
`ScanResult { schema_version, root, started/finished (UTC), options, status: Complete|Partial|Cancelled, tool_version, nodes: [...], errors: [...], totals }`. Nodes are stored as a flat array with `parent` indices (compact, easy for a UI later). Times are UTC seconds + nanos; no machine names. The root path is stored as given (the user typed it); no other absolute paths.

## 6. CLI
`clap` derive. `scan` prints a short summary (files, dirs, bytes, errors, duration). `top` loads the JSON, rebuilds the aggregates if needed, prints a table. Exit codes as in requirements.

## 7. Safety checks specific to B1
- No destructive std calls (clippy tripwire already in `clippy.toml`).
- Files are opened only inside `dc-winmeta` for attribute queries, with no data-read access rights.
- Drive-root scans need `--i-know`.
- A test walks the source of `dc-scan` and `dc-core` for forbidden calls as a second tripwire (belt and braces).
- Tests only create data via `fixture-gen` or `tempfile`.

## 8. Dependencies to add (all from the approved list in CLAUDE.md)
`rayon`, `crossbeam-channel`, `windows` (only `dc-winmeta`, only the needed features), `criterion` (dev), plus existing `serde`, `serde_json`, `thiserror`, `clap`, `insta`, `proptest`, `tempfile`. Versions pinned exactly, `cargo deny` must pass.

## 9. Work order
1. `dc-core` types + `Tree` + aggregates + queries, with property tests.
2. `FileProbe` trait, fake and Unix impls, walker, tests on `tempfile`/fixtures.
3. Windows probe in `dc-winmeta`; run on Windows CI and on your machine.
4. CLI commands and JSON output + snapshot.
5. Benchmark, docs, CHANGELOG.

## 10. Open questions for you
1. Approve the `FileProbe` trait with the Windows code in `dc-winmeta` (keeps `dc-scan` free of `unsafe`)?
2. Approve adding `rayon`, `crossbeam-channel`, `windows` (minimal features), `criterion` as dependencies?
3. Hardlink duplicates: counted once in totals (my default), as WizTree and Explorer's folder size differ here. OK?
4. Size on disk for directories: sum of children (default) rather than the directory's own allocation. OK?
5. `min_size`/`max_depth` filters only affect what is *stored*, not totals. OK?
6. Scan JSON may be large for big drives. Is plain JSON fine for now (compact binary/SQLite comes in B6)?
7. Exit code 2 for "partial scan with errors". OK?

## 11. As built: differences from the plan above
- Windows probe uses **`windows-sys`** (Microsoft's raw bindings, already in the dependency tree) instead of the `windows` crate. Same API surface, fewer dependencies.
- **Allocated size** comes from `GetFileInformationByHandleEx(FileStandardInfo).AllocationSize` (one call that also covers compressed and sparse files) instead of `GetCompressedFileSizeW` + `GetDiskFreeSpaceW`. This matches what Explorer shows as "Size on disk".
- **`crossbeam-channel` was not needed** (atomics and one helper thread are enough) and was not added.
- Hardlink dedupe happens in the single-threaded merge step, in sorted-name order, so results are deterministic (the first name in sort order is the counted one).
- Reparse-point **directories** (junctions, symlinks, cloud-placeholder folders) are recorded as links and not entered. Reparse-point **files** (for example OneDrive placeholders) are kept as files, flagged `reparse`/`cloud`, with sizes from metadata only.
- The output file is created only after the scan finishes (`create_new`, never overwrites), so writing it cannot affect the scan.
- No Ctrl+C handler yet (it would need another dependency); cancellation is available through the library's `CancelToken`.

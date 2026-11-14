# Changelog

## Unreleased
### Phase B9 - Removing safely (step 1: the engine)
- `dc-quarantine` (feature `destructive-actions`, off by default): remove moves items into a holding folder on the same drive after re-checking each one; restore (keep both on a name clash, parent folders made again); empty only inside the holding folder; empty after the retention time; an append-only journal that survives crashes; folders with files in use moved file by file. Tested on temporary folders only, with injected crashes, full disk and access errors. Not used by the app yet.
- The scanner skips DiskCleaner's holding folders.
- Safety review fixes (all 11 findings, design section 14): the journal survives damaged writes; old cleanups are never emptied on a lost or wrong date, and several at once are asked first; "your own files" is one rule on the full path shared by the page, the rules and the engine (Program Files scanned as a folder is no longer "yours"); Documents, Desktop, Pictures, Music and Videos are shown as protected; a stopped file-by-file restore can be finished; emptying deletes only recorded items; on Windows each move checks the item's real location and never replaces anything (`win.rs`, the crate's only `unsafe`); removing needs a scan from the last hour; one job at a time; several screen fixes. A test for each finding.
- Step 2, the screens (only with `--features cleaning`): tick boxes in Files, Findings and duplicates; the selection bar; the review screen with per-item ticks for Probably safe items and hold-to-confirm for your own files; Undo; the Removed tab (restore, empty now); retention in Settings. `dc_api::cleanup` decides each pick's group from the latest scan.
### Your own AI as storage advisor (MCP) and scan reports
- Each finished scan writes a report (`latest-report.md`) and the scan behind it into DiskCleaner's own data folder; `dc report <view file>` prints the same report.
- `diskcleaner.exe --mcp` (and `dc mcp`): a read-only MCP server so Claude Desktop, Claude Code or any MCP app can answer "is this safe to delete?", "what breaks if I move this?", "which apps don't I use?", "where are my projects?" from the latest scan. Off until switched on in Settings, every question logged; any other AI can be given the report by copy and paste.
- `dc-api`: `inspect` (index over a view file), `advice` (explanations: owner, verdict, what deleting or moving would do, projects, apps), `report`, `mcp`, `datadir`.
### Overview (F4, read-only)
- New **Overview** page, now the default: where the space goes (apps, your files, Windows, everything else); every installed app with size, places, install date and last used (from Windows' record of started programs); programs nobody installed (portable tools, unpacked games); Downloads sorted by type with hints (installers of apps you have, archives already unpacked, untouched for a year); biggest files; files not changed for a year.
- "Sort this folder" in Files shows any folder (a messy drive) sorted the same way.
- Duplicate files on request (Overview): exact copies found by reading the files (same size, then the first and last 64 KiB, then the whole file with BLAKE3), biggest first, one file at a time for hard disks, with progress and Stop; for the whole scan or a sorted folder. Each set shows every copy and the space the extra copies take.
- Scans read Users first at the top of a drive, and Downloads, Desktop and Documents first in a profile (same result, sooner where it matters).
- Settings, Privacy: "Show when apps were last used" can be switched off.
- Backend: `dc-winmeta` reads Windows' record of started programs (UserAssist, current user, read-only; decoding tested, the Windows read itself unverified here); `dc-api` lists every installed app with version, install date, stated size, uninstaller and last use.
- Plans written: `docs/phases/f4-overview/plan.md`, `docs/phases/b9-cleaning/plan.md`.
### Desktop app - Files view and stuck-highlight fixes
- New default **Files** view: a simple, sortable folder list (size bars, share of folder, files, last change, verdict, owner); the node graph is now the **Map** view, one click away in the header.
- The map always shows how to clear a highlight ("Clear selection", "Show everything", Esc, click empty space); left-over hover highlights are cleared.
- Focusing a folder during or after a live scan no longer leaves an unresponsive screen; a live scan can no longer wait forever to finish.
- Settings: choose how sizes are written: like Windows (GB, default), precise (GiB) or like drive boxes (1000s).
- Known apps: Riot Client / Riot Games (report-only).
- Scattered traces: app folders in Documents, My Games, Saved Games and at the top of a drive are linked to their app; leftovers there are shown, never offered. The Apps tab lists every place an app keeps things.
- Save scan (desktop): save a finished scan as a file and open it again later with Open file. Only new files are created.
- "Not read" tab: lists what the scan could not read, with the reason.
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
- `dc view-data`: writes one compact file for the graph view from one or more scans (with `--apps`: owner apps and leftovers). Read-only, never overwrites.
- Graph view (`ui/`): the disk as a live, draggable, zoomable graph. Grows like a scan, opens folders as you zoom in, colours by risk, app or file type, and explains every finding. Loads `dc view-data` files or a built-in demo. Unit tests (vitest) and browser checks (`ui/qa/e2e.cjs`).
- Project layout: rule packs moved to `rules/` (junk/, apps/), the graph view to `apps/desktop/` (src/core, src/graph, src/panels, tests/unit, tests/e2e), your own scan files to `data/` (ignored by git); internal crates declared once in the workspace; docs index; CI checks the frontend too.
- Desktop app (`apps/desktop/src-tauri`, Tauri 2): pick a folder and the graph grows from the live scan; findings and owners arrive when it is done; stop any time; a whole drive needs a yes. `dc-scan` reports folders as they are read; `dc-api` turns that into live events and now holds the checks and the view format shared with `dc view-data`.

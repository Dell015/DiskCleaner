# Phase B3: Attribution graph - Requirements

Source: `ROADMAP.md` Phase B3, `NOTES.md` N2, N3, N4, N15, N16. Status: **draft for review**.

**Goal:** answer "who owns this folder, and why do you think so?" for every folder in a scan. Result: a graph **app -> executables -> folders -> files**, with app totals, vendor totals, and an honest "unknown" bucket. Every claim carries its evidence and a confidence level.

**Attribution is information, never permission.** It does not change `Risk`. A folder with a confident owner is still `DontTouch` until the rules engine (B4b) says otherwise. When evidence conflicts or is thin, the result is *less* certain, never more.

## Decisions to confirm (recommended answer first)
| # | Question | Recommended |
|---|---|---|
| Q1 | Where does the graph run? | A pure library function in `dc-attrib`: scan + app snapshot in, attribution out. No disk writes, no registry access of its own. |
| Q2 | May it read the disk beyond the scan? | Only to look at **executables inside install folders** (version info, optionally signature) through `dc-winmeta`, read-only, behind a trait so tests need no Windows. |
| Q3 | Per-user data folders (`AppData\Local\X`, `Roaming\X`, `ProgramData\X`) | Attribute by **name match** to an app or its publisher, marked `medium` confidence, with the matching rule shown. |
| Q4 | Vendor folders holding several apps (N2) | Group by the folder: it gets a **vendor total** (sum of its children's owners), not a single owner. |
| Q5 | MSI apps with no install folder (N3) | Try **executables, publisher and version info** inside likely folders; result is at most `low` confidence and always shows the evidence. |
| Q6 | Shared components (Common Files, runtimes) | Owned by **several apps** or by none; never split sizes between apps in totals (shown once as "shared"). |
| Q7 | Output | A JSON file (`dc attrib scan.json --apps apps.json --out attrib.json`) plus readable tables; schema-versioned; never overwrites. |
| Q8 | Explain command | `dc attrib ... --explain <path>` prints owner, confidence, and every piece of evidence for one folder. |

## R1. Inputs
1. A `ScanResult` (tree, sizes, `app_labels` if present).
2. An `AppSnapshot` from `dc apps --out` (names, publishers, install folders, main programs, shortcuts, signatures).
3. Optional: per-executable facts (product/company name, signature) supplied through a trait `ExeInfoSource`. A `FakeExeInfo` exists for tests.

## R2. Evidence kinds (strongest first)
| Id | Evidence | Confidence |
|---|---|---|
| `install_folder` | Folder equals the app's registered install folder | high |
| `inside_owned` | Folder is below an attributed folder (inherits owner and confidence) | same as parent |
| `exe_identity` | An executable inside the folder reports the app's product or company name, or is the app's `main_exe` | high / medium |
| `shortcut_target` | A Start Menu/Desktop shortcut of the app points into the folder | medium |
| `data_name_match` | `AppData` / `ProgramData` folder whose name equals the app name, publisher, or main-program stem (normalized) | medium |
| `publisher_folder` | Folder named after the publisher that contains several apps' folders | vendor group only |
| `publisher_match` | MSI app without folder: publisher matches and nothing contradicts | low |

## R3. Rules of the graph
1. A folder can have **several candidate owners**. The primary owner is the one with the strongest evidence; a tie is **ambiguous** and is shown as such, not resolved by guessing.
2. Contradicting evidence (two different apps claim the same folder with equal strength) lowers the result to `ambiguous`.
3. Sizes are never double-counted: an app's total is the sum of its **topmost** attributed folders' totals; a vendor total is the sum of its members' totals plus its own unattributed contents.
4. Anything without evidence is `unattributed`. There is no default owner.
5. Protected paths (from `dc-protected`) are attributed normally but carry a `protected` marker; attribution never offers them as candidates for action.
6. Windows system areas (`\Windows`, `ProgramData\Microsoft`, ...) are attributed to the built-in owner **"Windows"** (confidence high) so they do not clutter the unknown bucket.

## R4. Outputs
1. `Owner { id, name, publisher, kind: app | vendor | windows | shared, key }`.
2. `Assignment { node, owner, evidence: Vec<Evidence>, confidence: high | medium | low | ambiguous }`.
3. Per-owner totals (bytes, size on disk, files), a `shared` total, an `unattributed` total, and the biggest unattributed folders.
4. `schema_version`; additive changes keep it, breaking changes bump it; a file from a newer schema is refused.

## R5. CLI
- `dc attrib <scan.json> --apps <apps.json> [--out FILE] [--limit N]`: top owners by size, then top unattributed folders.
- `--explain <path>`: owner(s), confidence, each evidence line, and whether the path is protected.
- Read-only; `--out` never overwrites. Works on any OS with saved files.

## R6. Safety and quality requirements
1. No `unsafe`, no `unwrap`/`expect`/`panic` in `dc-attrib` (same bar as safety-critical crates, though it is not one).
2. Tripwire test: no write, delete, process or registry-write calls in `dc-attrib/src`.
3. Property tests: sizes never exceed the scan total; no folder is counted under two owners in totals; shuffling input order gives the same output (deterministic).
4. Table-driven tests on generated fixtures for each evidence kind, including the conflicting and ambiguous cases.
5. Name matching is normalized (case, punctuation, `Inc.`/`Ltd`/version suffixes) and tested against false friends (for example an app named `Go` must not claim every folder containing "go").
6. Real-machine acceptance: improve on B2 numbers (N15: more than 73 of 430 apps with a main program) and show the biggest folders under `Program Files`, `AppData` and `ProgramData` with owners.

## Out of scope
Deciding what may be deleted (B4b/B5), leftovers of uninstalled apps (B5), Store/MSIX apps (later), hardlink-aware attribution (N18), network drives.

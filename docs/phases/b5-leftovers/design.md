# Phase B5: Leftovers - Design

Status: **built** (one piece, D79). Requirements: `requirements.md`.

## 1. Crate layout (`dc-leftovers`, safe Rust, no `unsafe`)
```
src/
  lib.rs        API, the "findings are advice, not actions" note
  model.rs      Finding, Kind, Evidence, EvidenceKind, Confidence, Leftovers, StaleApp, Refused
  folders.rs    container detection and the list of direct children to examine
  analyze.rs    containers walk, the four kinds of evidence, age, scoring, vetoes, risk
  facts.rs      one backwards pass: newest modified time, programs inside, personal-data bits
  known.rs      the validated list of well-known apps (packs/known_apps.toml)
  (CLI text tables live in dc-cli: leftovers_cmd.rs)
```
Dependencies: `dc-core`, `dc-protected`, `dc-rules` (for `Candidate`, `Sizes`, the classifier and the folder check), `dc-attrib` (assignments, owners), `dc-winmeta` (model types only: `AppSnapshot`, `InstalledApp`, `AppFlags`). It does not call any Windows API.

## 2. Public API
```rust
pub fn find_leftovers(
    scan: &ScanResult,
    snapshot: &AppSnapshot,
    attribution: &AttribResult,
    classifier: &Classifier,               // protected list + rules (for the checks)
    resolver: &(dyn PathResolver + Sync),
) -> Result<Leftovers, CoreError>;

pub fn explain(scan, snapshot, attribution, classifier, resolver, path) -> Result<Option<Explanation>, CoreError>;
pub fn verify(found: Leftovers, classifier, resolver) -> Leftovers;   // second look at the real disk, like B4b
```
`now` is `scan.finished_unix` (D66). Later: take a shared `&Tree` so the CLI builds it once (N52 follow-up).

## 3. Model (plain serializable data)
```rust
pub struct Leftovers { pub findings: Vec<Finding>, pub refused: Vec<Refused>, pub stale_apps: Vec<StaleApp>,
                       pub offered: Sizes, pub reported_only: Sizes }
pub struct Finding {
    pub candidate: dc_rules::Candidate,     // path, node, risk, rule = "leftover_...", reason, summary, sizes
    pub kind: Kind,                         // MissingApp | BackupCopy | UnregisteredProgram
    pub confidence: Confidence,             // Low | Medium | High
    pub points: u32,                        // 0..=100
    pub evidence: Vec<Evidence>,            // kind + plain-language detail
    pub app: Option<String>,                // the app it probably belonged to (display only)
    pub newest_modified_unix: Option<i64>,
    pub personal_data: Vec<String>,         // what looks personal (R6), empty if nothing
}
pub struct Refused { pub path: String, pub kind: Kind, pub because: String }
pub struct StaleApp { pub name: String, pub publisher: Option<String>, pub registered_folder: String }
```
`Confidence` here is its own small enum (Low/Medium/High), separate from B3's (which has Ambiguous).

## 4. Algorithm
1. Build the tree and child lists once. One reverse pass over nodes (children have larger ids than parents) fills `newest_mtime[node]` and `has_programs[node]` (any `.exe`/`.dll` below) and `personal[node]` flags (R6). No allocation per node.
2. Find containers: for every directory node, the parent path key goes through B3's container rule (`dc_attrib` exposes `container_folders(scan) -> Vec<(NodeId, Container)>`; the rule itself stays in `dc-attrib`). The direct children that are directories are the examined folders (R2).
3. Valid apps (R5): from the snapshot, drop updates/system/hidden entries, and any app whose install folder is inside another app's install folder (N37) or outside program containers. `stale_apps` = valid apps with `flags.stale_registration`.
4. For each examined folder, in a fixed order (by node id), compute the evidence of each applicable kind (R3):
   - `missing_app`: B3's assignment for the folder has `owner` = a stale valid app and confidence >= Medium. Points 60, +20 if B3's evidence includes `NameMatch`.
   - `backup_copy`: name matches the backup pattern (stripped base name equals a sibling directory's name, compared with B3's `normalize`, case-insensitive). Points 50.
   - `unregistered_program`: container is ProgramFiles/CommonFiles, B3 left it unowned (no assignment), `has_programs`. Points 30.
5. Age points and the recent-change cap from `newest_mtime` (R3). A folder with no known modified time gets no age points and no cap.
6. Score -> confidence (>= 80 High, >= 50 Medium). Then the vetoes in R4 order:
   - protected check (typed path via the classifier's protected list; the second look, `verify`, adds the real path);
   - owner Windows / publisher Microsoft;
   - link/cloud/hardlink inside: **reuses the same walk as B4b** (`dc_rules` exposes `blocker_inside`, made `pub` with a stable signature);
   - personal data (R6).
   `risk = escalate_all([base, vetoes...])` where base = `ProbablySafe` for Medium/High and `DontTouch` for Low. `Safe` is not representable here: there is no code path that produces it (checked by a test over the whole result).
7. `Finding.candidate.rule` is `leftover_missing_app` etc., `reason` is the same id, `summary` a one-line English sentence. `refused` keeps only the outermost refusals (same idea as B4b).
8. Sort: ProbablySafe first, then by on-disk size (desc), then path.

## 5. Speed
One pass over the tree for the aggregates (linear, no per-node strings), then work only on direct children of containers (hundreds to a few thousand folders) and the descendant walk for those that reach the link/cloud check (bounded by the offered folders' sizes, as in B4b). Expected cost on the 1.5 M-entry scan: well under a second beyond reading the file. If it is not, the D68 rule applies: measure first (`--timing`), then fix with a test that proves identical answers.

## 6. CLI (`dc leftovers`)
`dc leftovers scan.json --apps apps.json [--limit N] [--explain PATH] [--timing]`. Reuses the loaders and the `Stopwatch` of `classify`. Output sections: summary line (offered / reported only / refused), the findings table (size, confidence, risk, kind, path, then indented evidence lines), "Apps whose folder is gone" list, refused list, reminder.

## 7. Slices (superseded: built in one piece, D79)
1. **Slice 1:** model, tree aggregates, containers, `missing_app` and `backup_copy`, vetoes (protected, owner, links), scoring, CLI with `--explain`, tests. Real run, review.
2. **Slice 2:** `unregistered_program`, personal-data check, age points and the recent-change cap, stale-apps section, second look (`verify`), properties, real run, review.

## 8. Safety notes
- Findings are information; the type has no "delete" meaning. The read-only source test applies to the crate.
- The protected list is consulted first and can only add refusals.
- Personal-data detection and age use only names, extensions, sizes and times already in the scan; no file is opened.
- Fails closed: a missing modified time, an unreadable assignment, or an unknown container gives no finding or `DontTouch`, never a more lenient result.

## 9. As built
- `dc-attrib` now exports `container_kind` and `Container`; `dc-rules` exports `child_lists` and `folder_blocker` (the same link/cloud/hardlink/protected walk that B4b candidates use) and `Candidate`/`Sizes` are serializable.
- The container walk starts at the scan root and only descends along paths that can lead to a container (users, appdata, programdata, program files, common files), so it is cheap on big scans.
- Result: 0.6 s for `find_leftovers` on a synthetic 1 M-node scan (dev profile); attribution (1.1 s) and reading the file (2.5 s) dominate, as in N58.
- Tests: 19 scenarios, 3 property tests, 3 list tests, a read-only tripwire, and a CLI test.

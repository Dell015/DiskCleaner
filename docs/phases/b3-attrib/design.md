# Phase B3: Attribution graph - Design

Status: **draft for review**. Requirements: `requirements.md`.

## 1. Crate layout (`dc-attrib`, safe Rust)
```
src/
  lib.rs        public API, docs ("attribution is not permission")
  model.rs      Owner, Assignment, Evidence, Confidence, AttribResult (+ serde, schema version)
  names.rs      normalization and safe name matching
  index.rs      path index over the scan tree: node -> normalized path, children, sizes
  exe.rs        ExeInfoSource trait, FakeExeInfo, executable discovery inside install folders
  rules.rs      one function per evidence kind; each returns candidate claims
  resolve.rs    merge claims per node: strongest wins, ties -> ambiguous, inheritance downward
  totals.rs     topmost-node rollups, vendor groups, shared and unattributed buckets
  report.rs     text tables for the CLI
```
Dependencies: `dc-core`, `dc-winmeta` (model types only; Windows-specific code stays there), `dc-protected` (marker only), `serde`.

## 2. Pipeline
```
scan + snapshot --index--> normalized paths
   --rules--> claims (node, owner, evidence, strength)
   --resolve--> one Assignment per attributed node (primary, alternatives, confidence)
   --inherit--> descendants take the nearest ancestor's assignment unless they have their own
   --totals--> per owner / vendor / shared / unattributed
```
Each stage is a pure function on plain data, tested alone.

## 3. Claims and strength
`Claim { node, owner, kind, strength: u8, detail: String }`. Strength orders evidence (install_folder 90, exe_identity 80/60, shortcut_target 50, data_name_match 40, publisher_match 20). Confidence is derived: >=80 high, >=40 medium, otherwise low. Two claims on the same node by different owners within 10 points of each other => `ambiguous` (alternatives listed). No claim => unattributed.

## 4. Rules
1. **install_folder**: nodes carrying `app_labels`, or whose normalized path equals an app's install folder (same logic as B2 labels, reused, not copied).
2. **exe_identity** (fixes N15): for each app with an install folder, look for executables inside (depth-limited, from the scan tree, no disk walk). Candidates: `main_exe`, files whose stem matches the normalized app name, otherwise the largest `.exe` at the top level. Ask `ExeInfoSource` for product/company; a match upgrades the app's `main_exe` and signature in the output (not in the snapshot file).
3. **shortcut_target**: folder containing a shortcut target of the app.
4. **data_name_match**: only direct children of `AppData\Local`, `AppData\Roaming`, `AppData\LocalLow`, `ProgramData`, and `Program Files*` roots; match by exact normalized equality of name to app name, publisher, or main-exe stem. Never substring matching. Names under 4 characters need exact publisher+app agreement.
5. **publisher_folder**: a direct child of `Program Files*` / `ProgramData` / `AppData\*` whose name equals a publisher and contains two or more attributed children => a *vendor* owner with the children as members.
6. **publisher_match** for MSI apps: the app has no folder; if exactly one folder's executables carry its publisher/product name, claim it at low strength; otherwise nothing.
7. **windows**: built-in table of system areas to the "Windows" owner.

## 5. Name matching (`names.rs`)
`normalize("Microsoft Visual Studio Code (User)")` lowercases, removes punctuation, trims suffix words (`inc`, `ltd`, `corp`, `corporation`, `llc`, `x64`, `x86`, `64-bit`, version numbers, `(user)`), collapses spaces. Equality is exact on the normalized form; a token-subset match (all tokens of the shorter name appear in order in the longer) is allowed only when the shorter has two or more tokens. Tests include false friends.

## 6. Totals (no double counting)
Walk the tree top-down; when a node has a primary owner, add its **total** to that owner and do not descend further for that owner (children with a *different* explicit owner are subtracted from the parent and added to their own owner). Vendor totals sum member totals plus the vendor folder's own direct files. Shared owner = nodes claimed ambiguously by more than one app.

## 7. Output file
```json
{ "schema_version": 1, "created_by": "...", "scan_total": {..},
  "owners": [{"id":1,"name":"Git","kind":"app","key":"Git_is1", "bytes":..,"on_disk":..,"files":..}],
  "assignments": [{"node":123,"owner":1,"confidence":"high","evidence":[{"kind":"install_folder","detail":"..."}],"alternatives":[]}],
  "shared": {...}, "unattributed": {..., "top": [..]} }
```
Only nodes where ownership starts are stored (not every descendant); inheritance is recomputed on load. Keeps files small.

## 8. CLI
`dc attrib scan.json --apps apps.json [--out attrib.json] [--limit 20]` and `--explain <path>`. `--explain` resolves the path against the scan tree (not the disk), prints the chain of evidence, and calls `dc-protected` for the marker.

## 9. Risks
| Risk | Mitigation |
|---|---|
| Wrong owner shown confidently | Evidence always shown; ties become ambiguous; no substring matching; confidence derived, not set by hand |
| Name collisions (app `Go`, `Java`) | Exact normalized match, minimum token rules, ambiguity on tie |
| Slow on millions of nodes | Single pass index; claims only for nodes near known roots; benchmark on the 1M-node tree |
| Users read "owned" as "safe" | Output and docs state it; risk untouched; CLI prints the reminder line |
| Executable info needs Windows | Trait + `FakeExeInfo`; Windows source lives in `dc-winmeta` |

## 10. Steps
1. `model`, `names`, `index` + tests. 2. `rules` (install_folder, windows, data_name_match, shortcut) + fixtures. 3. `exe` discovery + `ExeInfoSource` (+ Windows source in `dc-winmeta`). 4. `resolve`, `totals` + property tests. 5. MSI/publisher rules. 6. CLI + report + `--explain`. 7. Docs, CHANGELOG, DECISIONS, NOTES; user acceptance run on the real machine.

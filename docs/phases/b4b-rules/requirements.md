# Phase B4b: Rules and classification - Requirements

Source: `SAFETY.md` rules 3, 4, 5, 9; `NOTES.md` N19; `DECISIONS.md` D40, D54. Status: **draft for review**.

**Goal:** give every file and folder in a scan a `Risk` (`Safe < ProbablySafe < DontTouch`) with human-readable reasons, using **deterministic, readable rules**. Then list "cleanup candidates" (Safe / ProbablySafe) with their sizes. **Nothing is deleted.** A candidate is a suggestion; deletion does not exist before phase B9.

**Principles (from SAFETY.md):** unknown = `DontTouch`. Risk only goes up when sources are combined (the maximum wins). The compiled-in protected list (B4a) always wins and no rule can override it. Any error or ambiguity gives the more cautious result.

## Decisions to confirm (recommended answer first)
| # | Question | Recommended |
|---|---|---|
| Q1 | Where do rules live? | TOML rule packs. One **built-in pack compiled into the program**. |
| Q2 | May users add their own packs? | **Not in B4b.** Loading user packs is added later, with the extra restrictions below. `dc rules --check FILE` already validates a pack so authors can try. |
| Q3 | How are rules combined? | Every matching rule counts; the **highest risk wins**. No rules matched = `DontTouch` (unclassified). |
| Q4 | What does the protected list do here? | Always first. Protected = `DontTouch`, reason shown; rules are not even consulted for it. |
| Q5 | Folder that contains something protected or unclassified-risky | The folder is at least as risky as its riskiest part: a candidate folder containing a `.git`, a protected path, or a link/cloud file is **`DontTouch`**. |
| Q6 | Which kinds of things may be `Safe`? | Only things that regenerate by themselves: temp files, caches (browser, shader, thumbnails, package-manager download caches), crash dumps, logs older than N days. |
| Q7 | Windows-internal junk (Windows\Temp, WinSxS, SoftwareDistribution) | **Stays protected.** We point to Windows' own tools (Storage Sense / Disk Cleanup) instead of touching them. |
| Q8 | Upgrade leftovers (`Windows.old`, `$WINDOWS.~BT`) | `ProbablySafe`, never `Safe`, with a clear warning. Needs a narrow, reviewed change in `dc-protected` (D54), not a pack override. |
| Q9 | Age conditions | Rules may require files to be older than N days (default for caches: 7). Age uses the last-modified time in the scan. |
| Q10 | Output | `dc classify scan.json` (candidates by size, totals per risk), `--explain PATH`, `dc rules --list`, `dc rules --check FILE`. All read-only. |
| Q11 | TOML library | The `toml` crate with `serde`, version pinned, checked by `cargo-deny`; unknown fields are errors. |

## R1. Rule pack format (TOML)
```toml
schema = 1
[[rule]]
id       = "browser-cache-chromium"        # unique, kebab-case
risk     = "Safe"                          # Safe | ProbablySafe | DontTouch
reason   = "browser_cache"                 # stable machine id
summary  = "Chromium browser cache; the browser rebuilds it"
path     = '*:\Users\*\AppData\Local\*\User Data\*\Cache'   # pattern, same syntax as dc-protected
scope    = "subtree"                       # subtree | self
min_age_days = 7                           # optional, files only
examples_match    = ['C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Cache\f_000001']
examples_no_match = ['C:\Users\bob\AppData\Local\Google\Chrome\User Data\Default\Login Data']
```
1. `deny_unknown_fields` everywhere: a typo is an error, not a silent no-op.
2. `examples_match` / `examples_no_match` are **required** for every rule and are checked when the pack loads (they double as tests).
3. A rule can only say what risk applies when it matches; it cannot say "unprotect".

## R2. Validation (the pack is rejected as a whole if any rule fails)
1. Valid TOML, `schema = 1`, unique ids, known risk, non-empty reason and summary.
2. The path pattern must have a drive wildcard or a drive letter and **at least three literal components** (so `*:\*` or `C:\Users` cannot be written by mistake).
3. A `Safe` rule must also contain at least one literal component naming the thing (`Temp`, `Cache`, ...), and must not match inside the protected list: if every example match is protected, the rule is useless and rejected.
4. `min_age_days` between 1 and 3650.
5. Every `examples_match` path must match the rule **and** be `NotProtected` (otherwise the rule is dead); every `examples_no_match` must not match.
6. Problems are reported with rule id and line, all at once.

## R3. Classification
1. Input: a path, and facts from the scan (kind, size, modified time, flags: reparse/hardlink/cloud/sparse/system).
2. Steps, in order, any step can only raise risk: (a) protected check (`dc-protected`, typed path and resolved path); (b) reparse points, cloud placeholders, hardlinked files => `DontTouch` (reasons shown); (c) matching rules; (d) none matched => `DontTouch` `unclassified`.
3. Result: `Classification { risk, reasons: Vec<Reason>, rules: Vec<RuleId> }`. Reasons are stable ids with text. A `Safe` result lists the rule that made it so.
4. Combination: `Risk::escalate_all` over all contributions; the result never depends on rule order.
5. **Folder items:** a candidate folder is classified from its own path and also from everything inside it (Q5): any descendant that is protected, a link, cloud-only, or `.git` makes the folder `DontTouch`.

## R4. Candidates
1. A candidate is a topmost folder or file whose classification is `Safe` or `ProbablySafe` and whose parent is not (no listing of a cache folder and its subfolders separately).
2. Totals: size and files per risk level; per reason.
3. Candidates carry attribution (owner, from B3) for display only.
4. Output is sorted by size; ties by path (deterministic).

## R5. Built-in pack v1 (conservative; each rule has examples)
- **Safe:** user temp (`AppData\Local\Temp`, contents older than 7 days), crash dumps, Windows Error Reporting reports in the user profile, thumbnail and icon caches, shader caches (D3D, NVIDIA, AMD), Chromium-family and Firefox caches (not profiles, cookies, passwords, extensions), package-manager download caches (pip, npm), `__pycache__`.
- **ProbablySafe:** `Windows.old` and `$WINDOWS.~BT/~WS` leftovers (after D54 change), `.nuget\packages`, `.gradle\caches`, `.cargo\registry\cache`, application `Logs` folders older than 30 days, installer files (`*.msi`, `*.exe`) in Downloads older than 90 days.
- **DontTouch (explicit, to beat broad rules):** browser `User Data` profile files other than caches, `Login Data`, `Cookies`, `Local Storage`, `IndexedDB`, wallets and password-manager folders.
- The list is reviewed in the final QA pass; every rule has at least one example match and one non-match.

## R6. CLI
- `dc classify <scan.json> [--apps apps.json] [--risk safe|probable] [--limit N] [--explain PATH]`: candidates and totals; `--explain` shows every step for one path. Uses the same pipeline as the library.
- `dc rules --list`: the built-in pack in readable form. `dc rules --check FILE`: validate a pack.
- All read-only. No flag deletes, moves, or even prepares deletion.

## R7. Safety and quality requirements
1. `dc-rules` is safety-critical: `forbid(unsafe_code)`, no `unwrap`/`expect`/`panic`, tripwire test against destructive calls.
2. Property tests: classification never panics; adding any rule never lowers a result; reversing rule order gives the same result; anything protected is `DontTouch` whatever the pack says.
3. Evasion tests: the same path spelled in different ways (case, `..`, short names via the resolver) gets the same classification.
4. A "never Safe" table: a pinned list of paths (Documents, Desktop, `.git`, `User Data\Default\Login Data`, `.ssh`, any `*.vhdx`, the Windows folder, anything on an unknown drive) that must classify `DontTouch` with the built-in pack.
5. Fuzz-style test: random rule packs from a generator never make a protected path non-`DontTouch`.
6. Real-machine acceptance: classify a scan of the profile and of `Program Files`; every Safe candidate is reviewed by the user before it is accepted.

## Out of scope
Deleting or moving anything (B9), leftovers of uninstalled apps (B5), AI advisor (B8), user-supplied packs, per-app rules based on attribution, content inspection of files.

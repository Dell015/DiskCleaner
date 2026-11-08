# Phase B5: Leftovers of uninstalled apps - Requirements

Source: `SAFETY.md` rules 1, 2, 3, 4, 7, 9; `DECISIONS.md` D55-D68; `NOTES.md` N26, N36, N37. Status: **built** (one piece, D79), awaiting the real-run review.

**Goal:** find folders that an app left behind after it was removed, show **why** we think so (evidence and confidence), and offer the strong cases as cleanup candidates in the same format as B4b. **Nothing is deleted** (deletion does not exist before phase B9) and **the registry is never changed**.

**Words:** an *unowned* folder has no owner after B3. A *leftover* is an unowned or former-app folder **with positive evidence** that an app is gone. Unowned is not the same as leftover.

## Decisions (all taken at the recommended defaults, see D69-D79)
| # | Question | Decision |
|---|---|---|
| Q1 | Which kinds? | (a) data of an app whose program folder is gone; (b) program folders nobody registered; (d) backup siblings such as `LGHUB.old`. Stale registrations are only *reported*. Installer caches (e) and empty folders (f) are out of scope. |
| Q2 | Evidence needed | At least one positive sign. A folder with no owner and no sign is just "unowned". Every finding lists its evidence and a confidence (Low / Medium / High). |
| Q3 | Age | Supporting evidence only (untouched for 180 / 365 days). Never creates a leftover alone. A very recent change lowers confidence. |
| Q4 | Risk | Never `Safe`. `High` and `Medium` confidence give `ProbablySafe`; `Low` is reported as `DontTouch` ("possible leftover, not offered"). Anything that looks like personal data is `DontTouch`. |
| Q5 | Places | Direct children of: per-user `AppData\Local`, `Roaming`, `LocalLow`; `ProgramData`; `Program Files`; `Program Files (x86)`; the `Common Files` folders. Never Documents, Desktop, Downloads, Pictures or similar. Never anything protected, and never anything owned by Windows or Microsoft. |
| Q6 | Registry | Never written, never deleted from. Stale registrations are shown for information. |
| Q7 | App registered inside another app's folder (N37) | Never treated as an app of its own, never a reason to call something a leftover. |
| Q8 | Running processes | Not checked in B5. |
| Q9 | Built-in list of known apps | **Yes (D80, changed after Q14).** A short data file of well-known apps; evidence otherwise comes from this computer only. |
| Q10 | Output | `dc leftovers scan.json --apps apps.json [--limit N] [--explain PATH]`, entries grouped by the app they probably belonged to. |
| Q11 | Result type | Findings carry a B4b `Candidate` (path, risk, reason, sizes), so the later UI and quarantine treat rule candidates and leftovers the same way. |
| Q12 | Speed | A few seconds on a 1.5 M-entry scan after loading; no repeated indexing of the scan; answers must not depend on how work is divided (D68). |
| Q13 | Tests | Synthetic scans and snapshots, plus one real run on the author's computer that is reviewed. |
| Q14 | Real apps to check | The author remembers uninstalling **McAfee**, **Valorant** and **The Finals**. The real run checks whether their leftovers appear (see the open question below). |

## Decision taken after Q14 (D80, accepted)
An app that was **fully** uninstalled leaves no registry entry, so with Q9 (no list of known apps) B5 can only find its leftovers when they happen to match a backup name or contain programs inside Program Files. McAfee data in `ProgramData` or `AppData`, or a Riot or Embark data folder, would show up only as "unowned". The fix, now accepted: a **short built-in list of well-known app names and vendors** (a data file, like a rule pack, with examples) that adds one more kind, `known_app_removed`: a folder whose exact normalized name matches a listed app that is **not installed now**. It would never be `Safe`, only `ProbablySafe`/`DontTouch` like the other kinds, and the list can only add evidence, never lower a risk. See D80.

## R1. Inputs
1. A scan (`ScanResult`), the app snapshot (`AppSnapshot`, from `dc apps --out`), and the B3 attribution of the same scan (computed inside the command). The scan's tree is built once and shared.
2. `now` is the end of the scan (as in D66), so ages do not drift.

## R2. Candidate folders (what is looked at)
1. Only **direct children of a container** (Q5) are findings. Their contents are part of the finding; nothing deeper is listed separately.
2. A folder is skipped (not even listed as refused) if it is a container itself, is not a folder, or lies in a container that does not exist in the scan.

## R3. Kinds and their evidence
Points add up (cap 100). **High >= 80, Medium >= 50, Low otherwise.**
| Kind (id) | Applies to | Evidence and points |
|---|---|---|
| `missing_app` (`leftover_missing_app`) | A folder in `UserData` or `ProgramData` that B3 assigned, with confidence Medium or better, to an app whose registration is **stale** (its install folder is gone) | Stale registration 60; exact name match from B3 +20 (only when B3's evidence says name match) |
| `backup_copy` (`leftover_backup_copy`) | A folder named like `X.old`, `X.bak`, `X.backup`, `X_old`, `X-old`, `X (old)`, `X (backup)` where a sibling folder `X` exists in the same container | Backup-style name next to an existing folder 50 |
| `known_app_removed` (`leftover_known_app_removed`) | A folder (unowned by B3) whose normalized name equals a name of an entry in the built-in known-apps list, and no installed app matches that entry (exact name or name prefix) | Named like a removed well-known app 60. Entries with `report_only = true` are never offered |
| `unregistered_program` (`leftover_unregistered_program`) | A folder directly in `Program Files`, `Program Files (x86)`, or `Common Files` that B3 left **unowned** and that contains at least one `.exe` or `.dll` | Contains programs, nobody registered them 30 |
Age (applies to all kinds, using the newest modified time inside the folder): untouched >= 180 days +20; >= 365 days +30 instead of +20; modified within 30 days: confidence is capped at **Low** (the folder is in use or was just copied).

## R4. Exclusions and vetoes (checked in this order, any one stops the offer)
1. **Protected** (typed or resolved path, `dc-protected`): refused, reason shown. Nothing here overrides the protected list.
2. **Owner is Windows, or the vendor/publisher name is Microsoft**: refused (`windows_or_microsoft`).
3. **Link, cloud placeholder or hardlinked file anywhere inside**: refused (same check as B4b candidates).
4. **Registered inside another app** (N37): the registration does not count as evidence (R5).
5. **Looks like personal data** (R6): risk `DontTouch`, reason `may_hold_personal_data`, with the file kinds found.
6. **Low confidence**: reported with risk `DontTouch` ("possible leftover").
The final risk is `Risk::escalate_all` of every step; no step can lower it.

## R5. Which app registrations count
1. An app counts as evidence only if it has an install folder that B3 could place in a program container (`Program Files`, `Program Files (x86)`, or a per-user programs folder) and that is **not inside another app's folder**. Otherwise it is ignored for B5 (N37).
2. Apps flagged `update_or_patch`, `system_component`, or `hidden_entry` are ignored.

## R6. Personal-data check
A folder is `may_hold_personal_data` if it contains any of: documents (`.doc*`, `.xls*`, `.ppt*`, `.pdf`, `.odt`, `.txt` larger than 1 MiB), pictures or media over 1 MiB, databases (`.db`, `.sqlite*`, `.mdb`, `.accdb`, `.kdbx`), key or wallet files (`.pem`, `.key`, `.pfx`, `.ppk`, `.wallet`, `wallet.dat`), mail (`.pst`, `.ost`), virtual disks (`.vhd`, `.vhdx`, `.vmdk`), or a folder named `saves`, `savegames`, `save games` or `profiles`. The kinds found are listed. **This errs on the side of caution: it will often say DontTouch for folders that are in fact junk.** That is intended.

## R7. Output
1. Per finding: path, size (on disk), kind, confidence, risk, evidence lines, the app it probably belonged to (when known), and the B4b `Candidate` fields.
2. Totals: offered (ProbablySafe) size and count; reported-only size and count; refused count with reasons (outermost only).
3. Stale registrations (apps whose install folder is gone): listed in a separate short section, never offered for removal.
4. `--explain PATH`: every step above for one folder (evidence, points, vetoes, final risk).
5. Text only, read-only, ends with the usual reminder that nothing is deleted.

## R8. Non-goals
Looking inside vendor folders (depth 2) for single dead apps (N54); running processes; services and scheduled tasks; Windows Installer caches; a bundled list of known apps; shortcuts pointing to missing programs (N55); registry cleanup; any deletion.

## R9. Tests
1. Scenario tests on synthetic scans for each kind, each veto, and each exclusion; deterministic output; JSON round trip of findings.
2. Property tests: findings are never inside protected paths; a finding's risk is never below the maximum of its vetoes; scores never exceed 100; no panics on odd names.
3. Read-only source check as in the other crates.
4. A real run on the author's computer, reviewed before the phase is accepted.

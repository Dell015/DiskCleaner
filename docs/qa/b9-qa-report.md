# QA report: B9 "Removing safely"

Date: 2026-11-15. Reviewed at commit `91a5f50` (main), in a fresh working copy on Linux.
Scope: `dc-quarantine` (engine, ops, win, journal, check, places, tests), `dc-api::cleanup`,
`apps/desktop/src/core/cleanup.ts`, `dc-protected::own`, `rules/junk/builtin.toml` (168 rules),
the `cleaning` module in `apps/desktop/src-tauri/src/main.rs`, the removal screens in
`apps/desktop/src/main.ts`, and `dc-scan::walk` (links never followed).

Phase 2 (fixes, branch `qa/b9-fixes`) is recorded at the end: "What was fixed".

## Verdict in plain language

**Not yet.** The engine is well built: nothing is ever deleted at removal time, every move is
written down before it happens, links and cloud files are refused, emptying only ever deletes
what the record names, and the person's own files need an extra confirmation. The checks all
pass. But I found two ways the person's own data could be offered as "Probably safe" (the
previous Windows installation's user folders, and Microsoft Teams' uploaded backgrounds), and one
way the holding folder itself can be turned against the app: if the holding folder (or a slot
inside it) is a link, items are moved into the link's target and the app can then neither restore
nor empty them. None of these deletes anything outright, but the first two put someone's files
on a 30-day countdown after a tick, and the third leaves a state the app cannot undo. After the
fixes below, "yes, with the Windows checks in the last section".

## Findings, most severe first

### 1. High — the holding folder's own path is never checked: a link there swallows the items
- `crates/dc-quarantine/src/engine.rs:568-598` (preparing the root) and `:609-639` (the slot).
- **What is wrong.** `remove()` verifies the *source* (`look()`: not a link, real path equals
  the given path) but never the *destination*. The root is only tested with
  `symlink_metadata(&root).is_err()` to decide whether to protect it; a root that exists as a
  link (or a slot swapped for a link after `create_dir_all`) is accepted, and the item is moved
  into the link's target. On Windows `win::move_checked` likewise verifies only the source
  handle; the destination string is resolved by the kernel, junctions included.
- **Failing scenario.** On a second drive the holding folder is `X:\.diskcleaner-holding`, a
  name anyone with write access to `X:\` can create first, as a junction to anywhere (another
  user's folder, a network share, a folder that will be deleted). DiskCleaner moves the items
  into the target, records them as held at the literal path, then: `restore` refuses ("the held
  copy cannot be used: a folder above it is a link"), `empty` refuses ("…is a link; nothing was
  deleted"), and the pre-existing root is never given the owner-only permissions. The same
  happens with a race that swaps the fresh slot for a link between `create_dir_all` and the
  move. Reproduced on Linux (probe tests `probe_holding_root_that_is_a_link_is_refused`,
  `probe_slot_swapped_for_a_link_before_the_move`).
- **Fix.** Use the existing `safe_dir` check (nearest existing ancestor is a real folder whose
  real path equals the given one; create; re-check) for `<root>\<id>\items` and for each slot
  right before the move. A root that is a link is refused with a clear reason, nothing moves.
  The remaining window between the last check and the move is microseconds; closing it on
  Windows fully would mean renaming relative to an opened directory handle in `win.rs`
  (`FILE_RENAME_INFO.RootDirectory`) — listed under decisions, not done here.
- **Test.** `crates/dc-quarantine/tests/engine.rs`: `a_holding_folder_that_is_a_link_is_refused`,
  `a_slot_swapped_for_a_link_before_the_move_is_refused`.

### 2. High — `Windows.old` is offered whole, with the previous installation's user files inside
- `rules/junk/builtin.toml:168` (`windows-old`, ProbablySafe on `*:\Windows.old`);
  `crates/dc-rules/src/candidates.rs:124-164` (`blocker_inside`).
- **What is wrong.** After an upgrade, a reset or a reinstall, `C:\Windows.old\Users\<name>\`
  holds the old profile — Documents, Desktop, Pictures — and for a "keep nothing" reinstall it
  is the only copy. The protected list is anchored at `*:\Users\…`, so
  `C:\Windows.old\Users\bob\Documents\thesis.docx` is not protected, and no DontTouch rule names
  it. The candidate finder therefore offers the whole of `Windows.old` as ProbablySafe (one tick
  on the review screen), and the folder is auto-emptied after the retention time.
- **Failing scenario.** Reproduced with a hand-made scan (probe `probe_windows_old_user_files`):
  `Windows.old` offered as one ProbablySafe item of 2000 bytes containing `Users\bob\Documents`.
- **Fix.** A DontTouch rule `windows-old-users` on `*:\Windows.old\Users` ("the previous
  installation's user files; copy what you need first"), plus finding 3 so that the DontTouch
  folder blocks the whole-folder offer; the rest of `Windows.old` is still offered as a partial
  item (the files outside `Users`).
- **Test.** `crates/dc-rules/tests/candidates.rs`:
  `the_previous_installations_user_files_are_never_offered`.

### 3. High — a "Don't touch" folder inside an offered folder does not block the offer
- `crates/dc-rules/src/candidates.rs:124-164` (`blocker_inside`), used by `visit`, `explain`
  and `folder_blocker` (also B5 leftovers).
- **What is wrong.** When a rule offers a folder, the finder looks inside for links, cloud
  files, hard links and protected places — but not for folders that a *DontTouch rule* names.
  The comment at `builtin.toml:272` says the DontTouch rules "win over the broad cache rules
  above"; that is true for the exact node, not for its parent being offered whole.
- **Failing scenario.** `teams-new-cache` (ProbablySafe on `…\LocalCache\Microsoft\MSTeams`)
  contains `…\MSTeams\Backgrounds` (`teams-backgrounds`, DontTouch: the person's uploaded
  custom backgrounds). The finder offers the whole `MSTeams` folder, `blocked` is empty
  (probe `probe_dont_touch_folder_inside_an_offered_folder`). The same mechanism is what makes
  finding 2 reachable, and will bite any future pair of rules of this shape.
- **Fix.** `blocker_inside` also classifies each child against the DontTouch rules; a hit blocks
  the folder ("something marked Don't touch: <path>"). As with links, the rest is offered as
  partial items. The textual overlap of the two Teams rules stays: it is how they are meant to
  work once the finder honours it.
- **Test.** `crates/dc-rules/tests/candidates.rs`:
  `a_dont_touch_folder_inside_blocks_the_whole_folder`.

### 4. Medium — an inner pick under a folder that moved only partly vanishes from the result
- `crates/dc-quarantine/src/engine.rs:547-553`.
- **What is wrong.** Items are processed outer first; an item inside one already held is
  skipped with `continue` — neither in `held` nor in `skipped`. That is right when the outer
  folder moved whole. When the outer folder was split (some files in use stayed), the inner
  item may still be sitting on the disk, and the person is told nothing about it.
- **Failing scenario.** Pick `Cache` and `Cache\sub`; `Cache\sub\b.bin` is in use. `Cache` is
  held partly (1 file) and reported as "1 files inside were in use"; `Cache\sub` appears
  nowhere, although it is still there (probe `probe_inner_pick_under_a_split_outer_is_reported`).
  The page keeps the row only because it detaches held paths; the result text undercounts.
- **Fix.** When the containing item was split and not every file under the inner item moved,
  report the inner item as skipped: "it is inside <outer>, which moved only partly (files in
  use); scan again".
- **Test.** `crates/dc-quarantine/tests/engine.rs`:
  `an_inner_pick_under_a_partly_moved_folder_is_reported`.

### 5. Low — a failed automatic emptying is silent
- `crates/dc-quarantine/src/engine.rs:1111-1119` (`empty_expired`),
  `apps/desktop/src-tauri/src/main.rs:453-473`, `apps/desktop/src/main.ts:1593-1614`.
- **What is wrong.** `empty_expired` drops every error (`.ok()`), nothing is logged. A cleanup
  that cannot be emptied (its items folder swapped for a link, a file locked) stays "expired"
  for ever: the page shows no error, and the "deleted for good in N days" text keeps counting
  into the negative.
- **Fix.** Log each failure to `cleaning-log.jsonl` ("empty-failed"), so it can be seen. Telling
  the person on screen needs a new return shape for the Tauri command — listed under decisions.
- **Test.** `crates/dc-quarantine/tests/engine.rs`: `a_failed_automatic_emptying_is_logged`.

## Decisions for the owner (design questions, not decided here)

1. **Closing the last race on Windows.** After fix 1 the destination is verified right before
   each move, but a rename by path can still be redirected in the microseconds between check
   and move by someone with write access to the holding root. Closing it means renaming
   relative to an opened, verified directory handle (`FILE_RENAME_INFO.RootDirectory`) in
   `win.rs`. Worth it? It is `unsafe` code and needs Windows testing.
2. **Re-protecting an existing holding root.** `protect_dir` runs only when the root is new.
   A root that already exists (created earlier by DiskCleaner, or by someone else on a shared
   drive) keeps whatever permissions it has. Options: always call it (idempotent; fails
   harmlessly on FAT), or refuse a root DiskCleaner does not own (needs an owner check in
   `win.rs`).
3. **Holding folders on removable or network drives.** `X:\.diskcleaner-holding` is also made
   on USB sticks and mapped network drives (SUBST, `Z:`). Removing from those works, but
   hidden/owner-only is best effort there and the drive may be gone next time. Refuse removable
   and remote drives, or allow?
4. **A drive letter that changes.** A cleanup made on `X:` is invisible in the Removed tab
   while the drive is `Y:` (its root is not found) and comes back when the letter does. Fine as
   is, or should the tab say "a holding folder is missing"?
5. **Telling the person when automatic emptying fails** (finding 5): change `empty_expired` to
   return problems and show them on the Removed tab?
6. **`Windows.old\Users` scope** (finding 2): I made the whole `Users` folder DontTouch. An
   alternative is to protect only the personal folders inside it (Documents, Desktop, …) and
   still offer `Windows.old\Users\<name>\AppData`.
7. **Old installers in Downloads** (`old-installers-exe`/`-msi`, ProbablySafe after 90 days).
   A portable program kept on purpose in Downloads (`tool.exe`, never installed) is offered as
   an "installer". Keep (it needs a tick, and the summary says "probably installed already"), or
   require the matching app to be in the installed list?
8. **Hard links.** A file with another name is refused inside offered folders (good), but an
   item picked by hand that has a second hard link moves under one name and the "freed" number
   counts bytes that are not freed. Accept, or refuse hard-linked items in `look()`?

## What was checked and found correct

- **Baseline (fresh clone of `91a5f50`, Linux, Rust 1.97):** `cargo fmt --all --check` clean;
  `cargo clippy --workspace --all-targets -- -D warnings` clean (after installing GTK/WebKit dev
  packages for Tauri); `cargo clippy -p dc-quarantine --features destructive-actions` clean;
  `cargo clippy -p dc-desktop --features cleaning` clean; `cargo test --workspace` all green;
  `cargo test -p dc-quarantine --features destructive-actions` 24 + 5 green; `npm test` 47
  passed, 1 skipped; `npm run build` ok.
- **Nothing deletes at removal time.** `remove()` only renames; `remove_tree` is reached only
  from `empty_locked`, which verifies root → `<id>` → `items` are real folders (not links, real
  path inside the root), deletes slot by slot only slots the journal names, re-checks each slot,
  and refuses anything unrecorded (`emptying_refuses_anything_the_record_does_not_describe`).
- **Default deny and consent.** `vet()` refuses unticked ProbablySafe, unconfirmed YourCall, the
  protected list, "not your own" places for YourCall, items touching DiskCleaner's own folders,
  anything that is or contains a link/junction/cloud file (`look()`), and your-own or leftover
  items changed since the scan. Rust decides the group from the latest scan
  (`dc_api::cleanup::eligibility`); the page only brings the ticks, and each tick goes only with
  the group it was given for (`main.ts:1348-1351`). The DEV-only `__dcRemove` hook cannot widen
  this: the group is still decided in Rust.
- **Journal.** Write-before-move; a torn line loses no other record; a lost start time means
  "never emptied automatically"; a clock ahead of the record is not trusted; crash after a move
  is read back from disk; split restores can be finished.
- **Locks.** One `cleaning.lock` for remove, restore and empty; the UI cancels duplicate finding
  before removing.
- **Restore.** Never replaces (free name "x (restored)"), refuses a changed drive letter, a now
  protected place, a place inside DiskCleaner's folders, a parent that became a link
  (`safe_dir`), and a held copy whose path is a link.
- **Scan freshness.** Both sides require a scan younger than 60 minutes; the Rust side measures
  with a monotonic `Instant`, so a clock change cannot make a stale scan fresh.
- **Hold-to-confirm.** 1.5 s pointer or keyboard hold with cancel on release, leave, blur,
  visibility change; a plain click on a hold button does nothing; Enter/Space are intercepted.
- **Empty on open.** One small cleanup goes quietly; several, or more than 50 GB, ask first with
  the date shown and a clock warning; cleanups ending within 3 days get a reminder.
- **win.rs (read only, cannot run here).** Source opened with `FILE_FLAG_OPEN_REPARSE_POINT`,
  refused if a reparse point, `GetFinalPathNameByHandleW` (normalized, DOS) compared with the
  given path, rename by handle with `ReplaceIfExists = false`, `MoveFileExW` with flags 0 only as
  a fallback (the documented TOCTOU), hidden attribute and owner/SYSTEM/admins DACL. No new
  issue beyond finding 1's destination side.
- **Scanner.** `walk.rs` uses `symlink_metadata` and records symlinks, junctions and all
  reparse-point directories as `Kind::Link`, never descending.
- **Rules.** All 44 DontTouch rules read; the 124 Safe/ProbablySafe rules checked for textual
  overlap with DontTouch folders (one: Teams, finding 3) and for anything that is someone's own
  data by name (findings 2 and 3; see decision 7 for Downloads installers). Broad patterns
  (`Local\*\Cache`, `Roaming\*\Cache`) are ProbablySafe (a tick each) and do not cover any
  known data folder.
- **Protected list vs your-call.** Documents, Desktop, Pictures, Music, Videos are protected
  on both sides (`places.rs` tests, `cleanup.ts` `PROTECTED_USER`); Downloads is not, by design.
- **8.3 names, `\\?\`, UNC, case.** `same_key` strips `\\?\` and lowercases on Windows; a path
  given as `PROGRA~1` or `\\server\share` fails the real-path comparison closed (refused, not
  mis-moved). UNC and volume-GUID paths have no drive letter → "no holding folder for this
  drive".

## What could not be checked here (Windows only)

- `win.rs` as a whole: it does not compile on Linux. Exact steps are in the final message and
  under "Windows test steps" below.
- Junctions and mount points (only symlinks exist on Linux); cloud placeholders (OneDrive
  "files on demand" attributes); the hidden/DACL protection of the holding root; Developer-Mode
  symlink creation in the tests.
- The real Tauri window (`npm run build` and the TypeScript tests run; the app itself was not
  started).

## Windows test steps for `win.rs`

No change was made to `win.rs` in Phase 2. To re-verify the existing code on Windows:

1. `cargo test -p dc-quarantine --features destructive-actions` with Developer Mode on (so
   the link tests run; they print "links cannot be made here" otherwise).
2. In a temporary folder, make `holding` a junction (`mklink /J holding target`) and run the
   new `a_holding_folder_that_is_a_link_is_refused` test; it must refuse without creating
   anything inside `target`.
3. With the app (`--features cleaning`) on a *copy* of test data in `%TEMP%`: remove a folder,
   check `%LOCALAPPDATA%\DiskCleaner\Holding` is hidden and its security tab lists only you,
   SYSTEM and Administrators; restore; empty.

## What was fixed (Phase 2, branch `qa/b9-fixes`, one commit per finding)

| Finding | Commit | Status |
|---|---|---|
| 1 Holding folder / slot that is a link | `fix(quarantine): never move into a holding folder or slot that is a link` | Fixed; 2 tests |
| 3 Don't touch folder inside an offered folder | `fix(rules): a Don't touch folder inside an offered folder blocks the offer` | Fixed; 1 test; also covers B5 leftovers. Finder speed on the 1M-node perf scan unchanged (1.08 s → 1.07 s) |
| 2 Windows.old user folders | `fix(rules): never offer the previous Windows installation's user folders` | Fixed (rule `windows-old-users`, 169 rules); 1 test |
| 4 Inner pick under a partly moved folder | `fix(quarantine): report a pick inside a folder that moved only partly` | Fixed; 1 test |
| 5 Silent failed automatic emptying | `fix(quarantine): log a cleanup that automatic emptying could not empty` | Fixed (`empty-failed` line in `cleaning-log.jsonl`); 1 test. On-screen message is decision 5 |

After these, all checks pass (fmt, 3× clippy with `-D warnings`, workspace tests, quarantine
tests with `destructive-actions`, `npm test`, `npm run build`). `win.rs` is unchanged.

Known limit noted while fixing 1: a user profile relocated behind a junction (so that
`%LOCALAPPDATA%` resolves elsewhere) now makes every removal refuse with "its holding folder is
a link or is not where it should be" — fail closed, the same way restore already behaved.

**Verdict after Phase 2: yes, once the Windows steps above have been run on a copy of test data**
(the Linux tests cover symlinks; junctions, cloud placeholders and the holding folder's
permissions can only be seen on Windows).

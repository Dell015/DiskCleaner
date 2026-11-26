# B11: Moving an app to another drive (design)

Requirements: `requirements.md` (R1 to R27). Decision: D141. Plan and launcher research:
`plan.md`. Everything below is what was built on 2026-11-26; section 10 lists what is **not**
built yet.

## 1. Where the code is

| Part | File | What it does |
|---|---|---|
| Move engine | `crates/dc-quarantine/src/mover.rs` | Checks, copy, compare, hold, link, "It works", Move back, recovery, journal. Behind `destructive-actions`. |
| Disk operations | `crates/dc-quarantine/src/ops.rs` (`MoveOps`) | Copy a file, make a folder, make/read/remove a link, "is this file in use", drive facts. Kept apart from `Ops` so removing cannot reach them. |
| Windows calls | `crates/dc-quarantine/src/win.rs` | `make_junction`, `read_junction`, `open_alone`, `volume`, `drive_letters` (the crate's only `unsafe`, each block with a `SAFETY` note). |
| What may move, and how | `crates/dc-api/src/moving.rs` | `how_to_move` (Move with or without link, Guide, Never), `dest_refusal`, `default_destination`, `command_runs_from`. Read-only. |
| Windows facts | `crates/dc-api/src/system.rs`, `crates/dc-winmeta/src/windows/registry.rs` | `apps_now` (installed apps now), `background_programs` (services and start-up entries; read-only registry). |
| Desktop commands | `apps/desktop/src-tauri/src/moving.rs` | `moving_available`, `move_options`, `move_check`, `move_start`, `list_moves`, `move_confirm`, `move_back`, `move_recover`. Desktop feature `moving` (off by default). |
| Screens | `apps/desktop/src/panels/move.ts`, `panels/apps.ts`, `panels/home.ts`, `panels/free.ts`, `main.ts` | The move window, the button and note on an app's page, Home's reminder and "Moved apps", Free up space step 3's text. |
| Browser stand-in | `apps/desktop/src/platform.ts` (`usePretendMoving`) | A pretend mover for the browser checks (`?moving`); nothing on a disk. |

## 2. The order of a move (R18 to R20)

Every line is written to `<data>\moves\<id>.jsonl` **before** the step it announces (or right
after a step that cannot be announced, like "copied").

| # | Journal | Disk |
|---|---|---|
| 0 | (none) | All checks (section 4). Nothing changed. |
| 1 | `start` (app, source, dest, link, bytes, files, the temporary name, where the original will wait) | The holding folder `...\move-<id>\original` and the new place's parent (`D:\Apps`) are made, each checked to be a real folder. |
| 2 | — | Copy the folder to `D:\Apps\.rhizome-moving-<id>`, file by file (`CopyFileExW` through `std::fs::copy`). |
| 3 | `copied` | — |
| 4 | `verified` | Compare: the same entries both ways, and every file the same bytes (read in 1 MB pieces). |
| 5 | `name`, then `named` | Rename the copy to its real name (`D:\Apps\<name>`), never replacing anything. |
| 6 | `hold`, then `held` | Rename the original into the holding folder of **its own** drive (instant, no copy). On Windows the item is opened first and its real location checked (`move_checked`). |
| 7 | `link`, then `linked` (apps only) | Make a junction at the old path pointing to the new place. |
| — | (state: waiting) | The person opens the app. |
| 8 | `confirm`, then `confirmed` | "It works": delete `move-<id>` inside the holding folder (checked to be a real folder inside it). |

Any failure in steps 2 to 7 puts everything back: the copy is discarded (section 5), and after
step 6 the original is renamed back first. The person sees why (in use, no room, ...).

## 3. What may move, and how (R9 to R17)

Decided in Rust from Windows' list **at that moment** (`dc_api::system::apps_now`), never from
the page:

1. Never: Riot Vanguard; Office and Microsoft 365 (Microsoft); Visual Studio (not VS Code).
2. A folder under `WindowsApps`: Guide (Windows' own Move in Settings).
3. The folder must be the app's registered install folder or a launcher's recorded folder for it,
   as Windows lists it now; otherwise Never ("scan again").
4. By launcher: Steam and Store: Guide (their own Move button). Epic, EA app, GOG: Guide ("cannot
   move; uninstall and install again on the other drive"). HoYoPlay, Riot, Battle.net: Move
   **without** a link, then their "Locate game" steps. Others: Move **with** a link.
5. For a Move: not a drive's top, not a user's own folder, not on the protected list, known to
   the latest scan, no file with two names (hard link) inside (from the scan's flags), no other
   app's registered folder inside.
6. Warnings (shown, not blocking): every app gets "Rhizome has not seen it moved before"
   (no list of known-good apps exists yet); a background part (a service or a start-up command
   whose program is inside the folder) adds "restart the PC before testing".

## 4. The engine's own checks (every move; any doubt refuses)

`Mover::check` (also run by `move_check` without changing anything):

- Source and new place are on different drives with letters (`MoveOps::drive`).
- Source: not a drive's top, not protected, not Rhizome's holding or data folder; `look` (a real
  folder, no link, junction or cloud-only file inside, at most 100,000 entries); no `.sys`
  driver inside; every file can be opened **alone** (`open_alone`: a sharing violation means a
  program has it open → "Close <app> and try again").
- New place: a plain full path (no `.` or `..`), does not exist, its nearest existing folder is
  a real folder (not a link) and not protected (a drive's top is protected only as itself);
  `dest_refusal` (text rules: a drive letter, below the top, no forbidden characters, not under
  Users or Windows, not protected).
- Other drive: not a network drive, **NTFS** (junctions need NTFS), free space ≥ size + 10%.
- The source's drive is not a network drive and has a holding folder on itself; the new place's
  drive has a holding folder (used to delete copies).
- The latest scan is less than an hour old (desktop command), as for removing.
- One disk job at a time: the same lock file as removing (`cleaning.lock`).
- (D145) This user may move the folder (`may_move`: access denied means administrator only), and
  neither the folder nor any folder inside is open in another program; apps whose folder is a
  version folder (`version-<hex>`, `app-<version>`) are never moved.

## 5. Deleting copies (R21)

A copy that is not needed (a failed copy, the copy on the other drive after Move back) is never
deleted where it is: it is renamed into its own drive's holding folder
(`<holding>\move-<id>\<label>`, journal `discard` first) and deleted there
(`delete_in_holding`: only a real folder inside a holding folder). If that fails the journal
says `discard_failed` and the move's notes tell the person to delete it by hand.

## 6. "It works" and "Move back" (R5, R6, R22)

- **It works** (state waiting): only when the old path is still Rhizome's junction to the
  recorded new place (apps) and the new place is a real folder; then the held original is
  deleted inside the holding folder.
- **Move back from waiting**: the junction is checked and removed (`remove_link`: refuses
  anything that is not a link; `RemoveDirectoryW` removes the link, never its target), the
  original is renamed back from the holding folder, then the new copy is discarded. If putting
  it back fails, the junction is made again so the app keeps working, and the reason is shown.
- **Move back from done**: room and "in use" are checked on the old drive, the app is copied
  back next to the old path under a temporary name and compared byte for byte, the junction is
  removed, the copy is renamed into place, then the new copy is discarded. Any failure before
  the junction is removed changes nothing; after, the junction is made again.
- Launcher games (no link): Move back needs the old path to be empty; afterwards the person
  points the launcher back (the window says so).

## 7. After a crash (R20): `Mover::recover`

Rhizome lists a move whose journal ends in the middle as "stopped" (state `moving` or
`moving_back`); Home and the app's page offer **Finish safely**, which calls `recover`:

| Journal ends during | What recovery does |
|---|---|
| a discard | Finishes deleting that copy (inside the holding folder). |
| copy, check or naming (original untouched) | Deletes the temporary copy and the named copy (if `name` was written), marks the move Not moved. |
| hold, original already aside, apps: junction to the new place exists | Marks it Waiting (the move had finished). |
| hold, original aside, no junction | Puts the original back, deletes the copy, Not moved. |
| hold, original aside, something unknown at the old path | Stops and says where everything is (nothing deleted). |
| confirm | Finishes deleting the held original. |
| moving back from waiting | Carries on: removes Rhizome's junction if there, puts the original back, deletes the copy. |
| moving back from done, before the copy back was checked | Deletes the partial copy back; the app stays on the new drive. |
| moving back from done, after the check | Carries on as above. |

Before deleting any copy, recovery checks the app is back at the old path as a real folder;
otherwise it stops and deletes nothing.

## 8. Screens

- **App page** (Library): "Move to another drive" for apps (not Windows parts) when moving is
  on; a note when the app waits for a check, is moved ("Move it back"), or stopped.
- **Move window**: Never → the reason. Guide → the launcher's numbered steps. Move → drives
  (non-NTFS shown greyed with the reason; USB marked), the new folder (editable, default
  `<drive>:\Apps\<name>` or `\Games\<name>`), every check again after each change, the size and
  free space, warnings, "What happens", "Close <app> first". Moving: progress for copying and
  checking; the window cannot be closed. Then "Does it work?" with **It works**, **Move it
  back**, **Later** (and the launcher's steps for games).
- **Home**: a "Waiting for your check" (or "A move did not finish") card per move, first in the
  side column; a "Moved apps" row opening the list (every move, its state, Check it / Move it
  back / Finish safely).
- **Free up space, step 3**: says how to move a game (open it, Move to another drive).

## 9. Tests (what each proves)

Rust, `crates/dc-quarantine/tests/mover.rs` (temporary folders; "drives" are two folders):

| Test | Proves |
|---|---|
| `an_app_moves_keeps_its_path_and_the_old_copy_goes_only_after_it_works` | Bytes identical at the new place, the old path reads through the link, the original waits in the holding folder, no temporary copy left, It works deletes only the original. |
| `move_back_before_it_works_puts_the_original_back_and_deletes_the_copy` | R6 from waiting. |
| `move_back_after_it_works_copies_it_back_and_checks_it` | R6 from done. |
| `a_launcher_game_moves_without_a_link_and_can_move_back` | R16; something at the old path blocks Move back. |
| `every_refusal_leaves_everything_as_it_was` | Same drive, in use, not NTFS, too little room, already there, a link inside, a driver, protected, a drive's top: refused with nothing changed. |
| `a_removable_drive_is_allowed_and_reported` | R12. |
| `a_failed_or_wrong_copy_is_deleted_and_the_app_stays` | A failed copy and a copy with different bytes are caught and deleted. |
| `when_the_original_cannot_move_aside_the_copy_is_deleted` | In use at the last moment. |
| `when_the_link_cannot_be_made_the_original_comes_back` | Step 7 fails. |
| `a_crash_before_the_link_is_undone_by_recovery`, `a_crash_after_the_link_finishes_forward`, `a_crash_after_naming_the_copy_deletes_it`, `a_crash_during_the_copy_leaves_no_copy_after_recovery`, `a_crash_while_moving_back_finishes_moving_back` | Section 7. |
| `it_works_is_refused_when_the_link_was_changed` | R22. |
| `one_disk_job_at_a_time` | R23. |

Unit tests in `mover.rs` (the junction data written and read back; other links and damaged data
refused; the journal survives damage) and `dc-api/src/moving.rs` (background commands matched on
their program; Office, Visual Studio, Vanguard refused and VS Code not; default places; new-place
rules; launcher routes; every refusal of `how_to_move`).

Browser checks (`tests/e2e/checks.cjs`, pretend mover): the button, non-NTFS drives greyed, the
default place, warnings, the move, "Does it work?", the page note, Home's reminder, It works,
Move back from the list.

## 10. Not built yet, and what to check on Windows

- Not compiled for Windows here (the Windows toolchain download is blocked in the build
  container): `win.rs` (junctions, `open_alone`, `volume`) and `registry.rs`
  (`read_background`) need `cargo build` and `cargo clippy` on Windows first.
- Moving apps from `Program Files` needs administrator rights, which Rhizome never has (hard
  rule 10). Such moves fail at step 6 with "needs administrator rights" and everything is put
  back. A separate elevated helper would be a new capability: the owner decides later.
- No list of apps known to move well yet: every app gets the warning.
- Epic, EA and GOG workarounds (only the "reinstall" advice is shown).
- Battle.net games are not found by Rhizome yet (its launcher record is not read), so the
  Battle.net route is not reached in practice.
- The scan after a move does not follow the junction (links are never followed), so the moved
  app shows on the other drive only when that drive is scanned.
- **Owner's test plan before switching `moving` on** (R26): 1. a throwaway folder app
  (`C:\dc-move-test\App` registered nowhere → it must be refused as "not registered"); 2. a
  small app installed for the current user (for example one in `AppData\Local\Programs`):
  move to D:, start it from its shortcut, It works; 3. Move back; 4. move again, close Rhizome
  during the copy, open Rhizome, Finish safely; 5. a USB drive formatted exFAT must be greyed out.

# B9 design: removing safely (hold, restore, empty)

Status: approved by the owner (2026-11-11). Step 1 (the engine) built: `crates/dc-quarantine`,
feature `destructive-actions`, tested on temporary folders only.
Builds on `plan.md` (the owner's answers) and SAFETY.md. This is the first part of DiskCleaner
that changes files, so every choice below leans to the careful side.

## 1. What "remove" means

Removing never deletes. It **moves** the item into a hidden holding folder **on the same drive**.
A move on the same drive is a rename: instant, needs no extra space, and the bytes are not
touched, so nothing can be damaged on the way. The space is freed for real only when the holding
folder is emptied (after the retention time, or "Empty now").

- Holding folder on the drive of your user profile (usually C:):
  `C:\Users\<you>\AppData\Local\DiskCleaner\Holding`
- Holding folder on any other drive (for example E:): `E:\.diskcleaner-holding` (hidden)
- If an item cannot be moved within its own drive, it is **skipped and listed**. There is no
  copy-then-delete fallback: that would need free space, take long on a hard disk, and add a
  window where a crash leaves two half copies.
- The scanner skips holding folders, so held items do not show up as "found" again.

## 2. What can be removed (three groups, decided again right before moving)

| Group | What | How it is confirmed |
|---|---|---|
| **Suggested** | DiskCleaner's findings marked Safe or Probably safe (caches, temp, crash dumps, leftovers of removed apps) | Safe: picked freely. Probably safe: each one needs its own tick ("I checked this") |
| **Your call** | Things you pick yourself in your own places: your Users folders (Downloads, Documents, Desktop, Pictures, Videos, Music) and data drives / folders no app owns (your 900 GB E: drive) | A stronger confirmation (see 4) |
| **Never** | The protected list, anything Windows owns, the inside of an installed app's folder (use its uninstaller: step 5 of the plan), Program Files, ProgramData and AppData of installed apps, anything a check refused, browser profiles and wallets, links and junctions, cloud-only files | Cannot be picked; the reason is shown |

"Your call" is new: DiskCleaner still never *suggests* your own files, but it no longer stops you
from removing them. Unknown things outside your own places stay "Never" (default-deny).

## 3. Picking

- Tick boxes in Findings, in the Files list, and in the Overview lists (Downloads groups, big
  files, old files, duplicates: "keep this one, remove the other copies").
- Right-click "Remove" on the Map.
- A selection bar at the bottom: "14 items, 21.5 GB selected · Review".
- Only items from a **finished** scan can be picked; while a folder is still being read, its
  items cannot.

## 4. The review screen (nothing moves before this)

- Every item with size, group, verdict and what removing it does (the same texts the AI gets).
- Totals per group. Items that would be skipped are listed with the reason.
- "Removed items are kept for 30 days on the same drive, then deleted for good. You can restore
  them any time before that."
- Confirm:
  - Only Suggested Safe items: one button.
  - Any Probably safe item: each needs its tick before the button works.
  - Any Your call item: tick "I understand these are my own files" **and** hold the button for
    1.5 seconds (hard to do by accident, easy on purpose).
- Limits per cleanup: 100,000 entries; above 200 GB, an extra line repeats the total.

## 5. Re-checking right before each item moves

Each check repeats on the disk as it is now; any doubt skips the item (fail closed):

1. The path still exists and is the same kind (file / folder).
2. Its real location (links resolved) is the path the scan saw, and it is not a link itself.
3. The protected list and the group rules (2) still allow it, on the resolved path.
4. Nothing inside is a link or junction (a quick read-only walk).
5. "Your call" items: nothing inside changed after the scan (newest change time). Suggested
   items may have changed (caches change all the time); the rule is checked again instead.
6. A folder is moved in one step. If Windows refuses because a file inside is in use, the
   folder is moved file by file and the files in use stay where they are (listed as skipped).
   No program is ever closed or killed.
7. Access denied (needs administrator) means skipped and listed, with "Try again as
   administrator" for the whole list later.

## 6. Crash safety

- A **journal** per cleanup (`Holding\journal\<cleanup id>.jsonl`): before each move a line
  "about to move A to B", after it "moved". Written and flushed before the move happens.
- A **manifest** per cleanup: every held item with original path, holding path, size, file
  count, newest change, group, verdict, reason, time.
- On start, an unfinished journal is completed: for each "about to move" without "moved",
  DiskCleaner looks where the item actually is (still at A, or already at B) and records that.
  Because a move is a single rename, there is no half state to repair.

## 7. Restore ("Recently removed" page)

- Every cleanup with date, items, size and days left; restore one item, several, or the whole
  cleanup.
- Restoring moves the item back. If its original place now has something with the same name,
  both are kept: the restored one is named "name (restored)" (and "(restored 2)" ...). A missing
  parent folder is created again.
- Restoring checks the item is still complete (same size and file count as recorded).

## 8. Emptying (the only real deletion)

- Automatic: when DiskCleaner opens, cleanups older than the retention time (default 30 days;
  7, 14, 30 or 60 in Settings) are deleted for good, and a note says how much space was freed.
  A reminder shows a few days before.
- "Empty now" on the Recently removed page, with its own confirmation, for one cleanup or all.
- Deleting only ever happens **inside a holding folder**, after checking that the resolved path
  is inside it; links are never followed. It is the only place in DiskCleaner that may call a
  delete function, enforced by the build (Clippy rule) and a source test.

## 9. Logs and the AI

- A plain log file of every removal, restore and emptying (date, path, size, result) in
  DiskCleaner's data folder; shown on the Recently removed page.
- The AI connection stays read-only. Later, the AI may *propose* a cleanup that appears on the
  review screen like any other; it never removes anything itself.

## 10. How it is switched on (step by step, as agreed)

1. Engine in `dc-quarantine` behind the Cargo feature `destructive-actions`, off by default.
   Tested only on generated folders: move, restore, empty, conflicts, in-use files, access
   denied, disk full, links inside, killing the process mid-way.
2. Screens in the app, still off in normal builds: they only exist in a build made with the
   feature (`npm run tauri dev -- --features cleaning`).
3. Your test on a throwaway folder (a copy of something you do not need), with that build.
4. Only after your okay: on in normal builds.
5. Later: installed apps through their own uninstaller, then sweeping what they leave.

## 11. Choices I made that you may want to change

1. Holding folder places (1), and no copying across drives.
2. "Your call" covers your Users folders and data drives no app owns; app data stays Never
   for now.
3. Hold-to-confirm (1.5 s) for your own files; a tick per Probably safe item.
4. Files in use are skipped, never closed.
5. Emptying happens when the app opens (no background service).
6. Restore conflicts: keep both, "(restored)".
7. Limits: 100,000 entries per cleanup; an extra line above 200 GB.

## 12. Built in step 1 (engine)

- `Engine::remove` / `restore` / `empty` / `empty_expired` / `cleanups`, all disk changes through
  the `Ops` trait (`RealOps`; tests inject faults). Only `ops.rs` may call rename or remove.
- The journal (`<holding>\<cleanup id>\journal.jsonl`) is the only record; there is no separate
  manifest to get out of step. A step announced but not finished is judged by looking at the disk
  (a rename is all or nothing).
- Checks before each move, in order: consent, protected list, holding folder for the drive, not
  DiskCleaner's own folders, the item and everything inside (no links, junctions or cloud-only
  files; its real path is the path given; at most 100,000 entries), "your call" unchanged since
  the scan.
- Tests (13, temporary folders only): move and restore byte for byte; every refusal; changed
  items; restore conflicts and missing parents; emptying only inside the holding folder, refusing
  a swapped-in link; retention; a crash right after a move; a full disk before a move; folders
  with files in use moved file by file and restored; a failed move leaving the item in place; a
  source check that only `ops.rs` changes the disk.
- The scanner skips holding folders.
- On Windows, the link tests need Developer Mode (or an administrator terminal) to create links;
  without it those checks are skipped and say so.

## 13. Built in step 2 (screens)

- Tick boxes in the Files list, the Findings list and each set of duplicates ("Select the extra
  copies (keep the oldest)"); "Select for removing" in the right-click menu. Items that can never
  be removed have a disabled box that says why.
- The selection bar ("N selected · size · Review and remove…") and the review screen: groups,
  a tick per Probably safe item, "I understand these are my own files" and hold-to-confirm
  (1.5 s, mouse or keyboard) when own files are included; progress; a result list of what was
  skipped and why. Removed items leave the views; "Undo" puts everything back.
- The Removed tab: every cleanup with what is held and when it will be deleted for good;
  Restore all, Restore per item, and Empty now (asks twice).
- Settings, Removing: keep removed items 7, 14, 30 or 60 days. Old cleanups are emptied when the
  app opens.
- Only in a build made with `npm run tauri dev -- --features cleaning`.

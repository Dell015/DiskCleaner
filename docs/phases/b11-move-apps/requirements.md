# B11: Moving an app to another drive (requirements)

Owner's request 2026-11-15 (`plan.md`); the open questions were answered on 2026-11-26 with
"go with default for everything for now" (D141). Each requirement has an id so the review,
the design and the tests can point at it.

## What the person gets

- **R1** On an app's page in the Library, **Move to another drive** moves the app's program
  folder to another drive. The app keeps working from its old shortcuts, its uninstaller and
  Windows' Installed apps list, because a link (a junction) is left where the folder was.
- **R2** The new place is `<drive>:\Apps\<App name>` for apps and `<drive>:\Games\<Game name>`
  for games (kind "game"), and the person can change it before starting (Q1).
- **R3** Before anything is copied, the person sees: what moves (the folder, its size), where it
  goes, how much space the other drive has, every warning (R12, R13), and what happens next
  (copy, check, link, "test it", then the old copy is deleted).
- **R4** After the move the app waits for the person's check: "Open the app, then press **It
  works** or **Move it back**". Home shows a reminder for every app waiting for its check (Q9);
  Rhizome never finishes a move on its own.
- **R5** **It works** deletes the old copy (it waited in the holding folder on the old drive)
  right away and frees the space (Q8a).
- **R6** **Move back** is offered at any time, before and after "It works" (Q10). Before: the old
  copy comes back from the holding folder. After: the app is copied back from the other drive and
  checked, the same way.
- **R7** Free up space step 3 (big games) offers the move for each game, or the launcher's own
  way (R15 to R17).
- **R8** Every move and every step is written down (a journal) and listed on a **Moved apps**
  list with its state, so the person and a reviewer can see what happened.

## What is checked before a move (all must pass; any doubt refuses)

- **R9** The folder is the app's registered install folder as Windows lists it *now* (read
  again at the moment of the move), and it is the folder the latest scan saw for that app.
- **R10** Never moved: Windows and anything on the protected list; Microsoft Store apps
  (`WindowsApps`: Windows moves those itself); Office and Visual Studio; anything with a driver
  (`.sys` file) inside; a folder that is a drive root, a user's profile, or holds another app's
  registered folder; a folder that is or contains a link, junction or cloud-only file; a folder
  with a file that has more than one name (hard link).
- **R11** The other drive: a different drive with a letter (not the same drive, not a network
  drive), formatted NTFS (a junction can only point to NTFS; exFAT and FAT32 are refused with
  that reason, Q3), with at least the folder's size plus 10% free (Q7). The new place must not
  exist yet.
- **R12** USB and other removable drives are allowed with a clear warning: "if this drive is
  unplugged, <app> will not start until it is plugged in again" (Q2).
- **R13** Apps with parts that run in the background (a Windows service or a start-with-Windows
  entry pointing into the folder) are allowed with a warning and the advice to restart the PC
  before testing (Q6). Apps not on the list of apps known to move well get a plain warning too
  (plan).
- **R14** The app must be closed: if any file in the folder is open (in use), nothing is copied
  and the person is asked to close the app and press "Try again". Rhizome never closes programs
  (Q5).

## Games in launchers

- **R15** Steam and Microsoft Store / Xbox games: Rhizome shows the launcher's own move button,
  step by step, and moves nothing itself (Q12).
- **R16** HoYoPlay, Riot Client and Battle.net games: Rhizome moves the game folder (same copy,
  check and holding folder as R18) **without** a link, then shows the launcher's "locate the
  game" step; the move is finished only after the person confirms the launcher found it
  ("It works"), and Move back is offered like any move (Q11). Riot Vanguard is never moved.
- **R17** Epic, EA app and GOG games: Rhizome says the launcher cannot move games and suggests
  uninstalling from the launcher and installing again on the other drive (Q13, Q14: the
  workaround steps come later).

## How a move is done (safety)

- **R18** Copy, then check the copy byte for byte against the original (Q4), and only then touch
  the original. A copy is first written under a temporary name and gets its real name only when
  it is complete and checked.
- **R19** The original is never deleted while the move is not confirmed: it is moved (renamed,
  same drive) into the old drive's holding folder, where it waits until "It works" (R5).
- **R20** Every step that changes the disk is written to the move's journal before it happens;
  after a crash, power cut or closing Rhizome at any point, Rhizome reads the journal and the
  disk and either finishes the step safely or puts everything back as it was, and says which.
- **R21** Copies that are thrown away (a failed or cancelled copy, the other drive's copy after
  Move back) are first moved into *that* drive's holding folder and deleted only there: the
  only deletion is still inside a holding folder (SAFETY.md).
- **R22** The link Rhizome leaves is removed only by Rhizome's Move back, and only after checking
  that it is a junction pointing exactly to the recorded new place.
- **R23** One disk job at a time (removing, restoring, emptying or moving), also across two
  Rhizome windows.
- **R24** Nothing here edits the registry, runs a program, or closes a program. Moving is a
  separate switch (desktop feature `moving`, off by default) until the owner's tests on his PC
  pass, as was done for removing (D135).
- **R25** Moves are refused for a scan older than one hour, as for removing.

## Testing and review

- **R26** Every step and failure is tested on temporary folders with injected failures (disk
  full, access denied, in use, a crash after each step), and on Windows by the owner on a
  throwaway folder and then a small app (Q15), before the switch is turned on.
- **R27** Everything is reviewable at the end of the project: this file, `design.md`, the test
  list in `design.md` section 9, the decision D141, and `docs/review/README.md` (the project-wide
  review guide).

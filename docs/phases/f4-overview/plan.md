# F4: Overview, sorted views, duplicates (read-only)

Status: built (2026-11-10). Steps 1-3 done; the job "queue" is one disk job at a time (a new
scan stops a duplicate search).

Goal (owner, 2026-11-09): after a scan, the app tells you everything on the PC: every app
(installed or just downloaded and run), what it takes, when it was last used; your Downloads and
other messy folders sorted so they are easy to judge; big, old and duplicate files. "Much better
than Windows' Storage settings." You decide what goes; removing comes in B9.

Everything in F4 is read-only. Nothing is moved, changed or deleted.

## Order
1. **Overview page** (this step)
   - Apps: installed apps with total size (own folder plus data elsewhere), publisher, version,
     install date, last used, whether it has an uninstaller; sorted biggest first; filter "not
     used for 6 months".
   - Programs not installed: folders or files with programs that no installed app claims, outside
     Windows and Program Files (portable tools, games unzipped from the internet). Installers are
     listed separately.
   - Downloads (every `Users\*\Downloads`), sorted into groups: installers, archives, disk images,
     videos, pictures, music, documents, programs, other; each with total, count, and age buckets.
     Hints: "installer of an app you have" (installer name matches an installed app), "already
     unpacked" (archive next to a folder of the same name), "untouched for a year".
   - Big files (top 200 over the whole scan) and old files (not changed for a year or more,
     biggest first).
   - The same groups work for any folder ("Sort this folder" in Files): the 900 GB hard disk case.
2. **Duplicates on request**: per folder or drive, a job: group by size, then a quick hash of the
   first and last 64 KiB, then a full hash only of the files still alike. Biggest first, one disk
   job at a time (hard disks), cancellable, results stream in. Reading contents is read-only.
3. **Jobs and acting while scanning**: a job queue per drive (scan, duplicates, later cleaning and
   organizing), one at a time per drive. A folder is "done" when its whole subtree was read; only
   done folders can be acted on. Live scans read Downloads, Desktop and Documents first.

## Last used
- Primary: Windows' own record of programs started from Explorer and Start ("UserAssist", in the
  current user's registry, read-only, no admin). Holds a last-run time per program.
- Fallback: none silently. If there is no record, the app says "not known" (never guesses "never").
- A Settings switch turns reading it off. The data never leaves the PC.

## Not in F4
Removing (B9), organizing (B10-organize), registry entries (never: tiny and risky, D107).

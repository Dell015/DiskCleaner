# B9: Cleaning (remove, hold, restore), then Organize

Owner's answers, 2026-11-09 (questions 1-19 and A-H). Status: planned; built after F4.

## What can be removed
- **Suggested** by DiskCleaner: Safe items can be picked; Probably safe items need an extra
  "I checked these" tick.
- **Your call**: your own files (Downloads, Documents, videos, a messy drive). DiskCleaner has no
  opinion and never suggests them, but you can pick them, with a stronger confirmation.
- **Installed apps**: removed with the app's own uninstaller (started only after you confirm; you
  see its window), then what it left behind is offered as leftovers. This is the only time
  DiskCleaner starts another program.
- **Never**: Windows, Program Files contents of installed apps (use their uninstaller), the
  protected list, anything in use, anything that changed since the scan.
- The registry is never edited (D107).

## How removing works
1. Pick items (Overview, Files, Findings, or right-click on the Map).
2. Summary screen: every item, sizes, totals; tick "I looked at the list" for big or riskier sets.
3. Re-check each item right before it moves (same size and time, not protected, not a link, not
   in use, folder fully read). Changed or in-use items are skipped and listed.
4. Move into a hidden holding folder on the same drive (instant, no extra space). Manifest with
   original path, size, hash, time, reason. Write-ahead journal so a crash can be finished or
   rolled back.
5. After the retention time (default 30 days; 7/14/30/60) the held items are deleted for good
   when the app opens; a reminder says when. "Empty now" frees the space at once.
6. "Recently removed": restore one item or a whole cleanup. If the original place has a new item
   with the same name, both are kept. A log file records every removal and restore.
- Folders that need administrator rights are skipped and listed ("Try again as admin").
- Only when you click; no automatic cleaning. Command line later.
- Tests: generated fixtures, then fault injection (disk full, access denied, killed mid-way); then
  a throwaway folder the owner chooses, before any real use.

## Organize (after cleaning works)
Move files of a messy folder into tidy folders (Installers, Archives, Videos\2024, ...), with a
preview of every move and one-click Undo of the whole run (same journal and manifest as cleaning).

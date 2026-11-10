# Future ideas (owner, 2026-11-10)

Not scheduled yet. Both come after B9 (cleaning). Written down so later phases leave room for them.

## 1. Restore points and backups ("version control" for your files)
Goal: make a point in history, and bring things back if anything goes wrong.

Two different needs:
- **Undo a mistake** (deleted, overwritten, a cleanup gone wrong): a snapshot of the drive at a
  moment. Windows already has the machinery (Volume Shadow Copy, "Previous Versions"): cheap,
  copy-on-write, but needs admin, lives on the same drive, and Windows drops old ones when space
  runs low. Plan: offer "Make a restore point" by hand and automatically before every cleanup
  (B9 already asks for a restore point), list them, and restore one file, a folder, or everything
  in a folder as of that moment.
- **Survive a broken drive or ransomware**: a real backup on another drive (USB disk, second
  drive, later cloud). Chosen folders, versioned (every run is a point in history), deduplicated
  (an unchanged file is stored once, BLAKE3 is already in the project), checked, scheduled.
  Restore by browsing a date.
- Snapshots on the same drive are not backups; the app must say so plainly.

## 2. Recovering deleted files
How it works: a deleted file's space is only marked free. Until something new is written there,
the bytes are still on the disk, and on NTFS the file's record (name, size, where its pieces are)
usually still exists in the Master File Table.
- Hard disks: good chances if little was written since. SSDs: TRIM usually wipes freed space
  within minutes, so recovery mostly fails; the app must say so, not raise false hope.
- Easy wins first: Recycle Bin, Previous Versions (restore points above), OneDrive's recycle bin,
  File History.
- Then: read the drive's MFT (read-only; this is the same work as B7's fast scan and its admin
  helper), list deleted files with name, size, folder, date and a chance estimate (are its
  pieces overwritten?), and copy chosen ones out.
- Hard rules: never write to the drive being recovered from; recovered files always go to a
  different drive; the drive is opened read-only.
- Later: "carving" (finding photos, videos, documents by their file signatures when the record
  itself is gone).
- Privacy: this shows other users' deleted files too, so it needs admin and says so.

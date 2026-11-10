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

## 3. Tidy-up coach (owner, 2026-11-11)
When: after each scan the app shows how many things there are to tidy; the coach runs only
when the user opens it (D116).

Folders grow messy: "New folder (3)", "untitled", "asdf", copies of copies, files in the wrong
place. A guided mode that walks the user through problems **one at a time**, each with a
suggested fix and Skip / Fix / Never ask again:
- Names: "New folder", "Untitled", "Copy of", "(1)", keyboard mash; suggest a name from what is
  inside (mostly photos from 2023 -> "Photos 2023").
- Empty folders, and folders nobody opened or changed for years.
- Misplaced files: installers in Documents, photos and videos in Downloads, a lone document in a
  game folder; suggest where they belong (the matching user folder, or a folder of the same kind).
- Folders that could be merged (two "Photos" folders, "Resume" and "CV").
- Deep nesting with single-child folders ("A\A\A\file").
Fixes are moves and renames, so they use the Organize engine (preview, journal, one-click Undo).
Never inside Windows, Program Files or app data folders; your own folders only.

## 4. Your own AI as the storage manager, through MCP (owner, 2026-11-11)
Status: the read-only part is built (D114, D115). Still to do: AI-proposed plans the user
confirms in DiskCleaner (after B9 and Organize).

No API key or paid credits: the user's own AI assistant (Claude Desktop, Claude Code, or any app
that supports MCP) connects to DiskCleaner on the same computer and asks it questions.
- `dc mcp`: a local MCP server (stdio, started by the AI app; no network, no open port). It reads
  the latest scan the desktop app saved (auto-saved to the app's data folder) and answers with
  tools: overview, where the space goes, search, details of a path (owner, verdict, evidence),
  apps and last use, Downloads sorted, duplicates, programs nobody installed, explain why.
- Read-only by default. The AI can only **propose** a plan (remove these, move those, rename
  that); the plan appears in DiskCleaner for the user to review and confirm. The AI never deletes
  or moves anything itself.
- Metadata only (names, sizes, dates, owners), never file contents (SAFETY rule 6). The user
  switches it on in Settings and can see exactly what was asked and answered (a log).
- File names are untrusted text: the answers mark them as data, so a file named like an
  instruction cannot steer the AI into an action (and actions need the user's confirmation anyway).
- This replaces B8's "advisor with an API key" as the main path; an API-key provider can stay
  optional.

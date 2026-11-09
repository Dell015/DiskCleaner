# F3: Desktop app with live scanning

Status: in progress. Read-only, like everything before B9.

## Goal

A desktop window (Tauri 2) where you pick a folder, the scanner runs, and the graph grows from the
real scan as it happens. No command-line steps, no JSON files (opening `dc view-data` files and the
demo still work).

## Pieces

| Piece | Where | What |
|---|---|---|
| Live listings | `dc-scan` | An optional callback gets each folder's entries right after the folder is read and before anything inside it is read. Results are identical with or without it (tested). |
| Live scan | `dc-api::live` | Runs the scan on a thread, sends `Started`, `Batch` (folders, every 60 ms), `Progress`, `Analysing`, `Finished` (marks, owners, assignments by path; hard-link second names to uncount) or `Failed`. |
| Shared checks | `dc-api::analysis`, `dc-api::view` | Rules, attribution, leftovers and the view format moved out of `dc-cli` so the CLI and the app give the same answers. |
| Desktop shell | `apps/desktop/src-tauri` | Commands: `start_scan` (streams events over a Tauri channel), `cancel_scan`, `is_drive_root`, `suggested_folders`. Folder picker from the dialog plugin. No file-system or shell plugin: the window can read only what the scanner sends. |
| Live graph | `apps/desktop/src/live.ts` | Entries are added to the tree as they arrive; every 400 ms the level of detail is worked out again and new nodes join a reveal queue drained at a steady pace; nothing shown is removed until the scan ends. Then marks and owners are applied and the picture settles. |

## Safety

- Read-only: the shell has no command that writes, moves or deletes; the only plugin is the open-folder dialog.
- A whole drive needs an explicit "yes" (as `--i-know` on the command line).
- Content-Security-Policy: own files and the Tauri IPC only.
- Installed apps are read from the registry (read-only) for owners and leftovers; on failure the scan still works without them.
- A cancelled scan shows what was read, and runs no checks (an incomplete scan would give unreliable verdicts).

## QA

- Rust: live events rebuild exactly the finished scan; folders always before contents; cancel; missing folder fails cleanly.
- Browser: a simulated live source (`?live=demo`) drives the same live code path in the e2e checks.
- The Tauri shell builds in the cloud container (Linux); the owner runs it on Windows.

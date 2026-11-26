# Safety

DiskCleaner inspects entire disks and advises on deletion, so safety rules come before features.

1. **Read-only until the quarantine phase (B9).** No code path deletes, moves, renames or writes
   to scanned paths. The quarantine crate is behind the `destructive-actions` feature, off by default.
   Since B9 the removal engine exists there: it only *moves* items into a holding folder on the
   same drive, re-checks each one first, keeps an append-only journal, and deletes only when a
   cleanup is emptied, only inside a holding folder. The desktop app includes it since the
   owner's tests on a real PC (D135); the `dc` command line and the AI connection never do. On Windows a
   move opens the item first and checks its real location, never replaces anything and never
   copies between drives (`dc-quarantine/src/win.rs`, the crate's only `unsafe`, D119).
2. **No destructive APIs outside `dc-quarantine`.** A Clippy `disallowed-methods` rule fails the
   build if `remove_file`, `remove_dir`, `remove_dir_all` or `rename` appear.
3. **Default-deny.** Anything unclassified is `DontTouch`.
4. **Risk can only go up.** `Safe < ProbablySafe < DontTouch`; the final risk is the maximum of
   the rule's and the advisor's. The AI advisor can only make a result more cautious.
5. **Protected Windows paths are compiled in** and cannot be overridden by rule packs.
6. **Metadata only to AI**, opt-in, off by default, with a preview of the exact payload.
7. **Links are never followed** (junctions, symlinks, reparse points).
8. **Tests never touch real user data**; they use temporary directories or generated fixtures.
9. **Fail closed.** Any error or ambiguity produces the more cautious result.
10. **Uninstalling only through the app's own uninstaller** (D133). An app's program files are never
    removed by DiskCleaner itself. On request, and only after the person confirms, it starts the
    uninstaller Windows records for that app (the one Settings → Apps runs), with its own window,
    never silently and never through a shell (`cmd.exe`, PowerShell and script hosts are refused).
    When the recorded command is doubtful it opens Windows' Installed apps page instead. Windows
    parts and runtimes other apps need are never offered. Only in the desktop app.
11. **Moving an app to another drive copies first and deletes last** (B11, D141). The app is
    copied to the other drive and every byte is compared before the original is touched; the
    original is then moved (a rename on its own drive) into the holding folder, and a junction
    is left at the old place. The original is deleted only when the person says the app works,
    and only inside the holding folder; a copy that is not needed is moved into its own drive's
    holding folder and deleted there. "Move back" is offered at any time. Each step is written
    to a journal first, so a crash is finished or undone safely. Never moved: Windows, protected
    places, drivers, Microsoft Store apps, Office, Visual Studio, folders with links or hard
    links, apps that are in use. Off unless the desktop app is built with `--features moving`.

## Reporting a problem
Please open a private security advisory on the repository rather than a public issue for anything
that could cause data loss.

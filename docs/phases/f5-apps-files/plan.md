# F5: Apps and Your files (plan)

Owner's request (2026-11-14): show every app people run, with its icon; clicking one shows its
network on the map and which parts can go without harming it and which it needs. Personal files
(Downloads, Pictures, Videos, Documents, coding projects, work files) get their own section,
managed the same way. Owner accepted these defaults (2026-11-15):

1. **Apps** and **Your files** are new main views next to Overview, Files and Map; the Apps tab
   in the side panel goes away.
2. Apps lists what people run: installed apps, games inside launchers, portable programs. Windows
   parts (drivers, runtimes such as Visual C++ and .NET) are folded into a closed "System parts".
3. Picking an app fades everything else on the map; the app sits in the middle, linked to every
   place it keeps things.
4. An app's parts, by what deleting them does: safe to clear (caches, shader caches, logs, crash
   dumps, update downloads), probably safe (old versions, installers), needed to run (program
   files, game data: only through its uninstaller), your saves and settings (never offered).
   Anything not recognised counts as needed (default-deny). Safe parts can be picked and removed
   through the usual review screen and holding.
5. Coding projects inside Documents are shown, but Documents stays protected (D122): their
   rebuildable folders (node_modules, target, .venv, build) are listed, not offered.
6. Project kinds are found automatically by markers: `.git`, `package.json`, `Cargo.toml`,
   `pyproject.toml`/`requirements.txt`, `*.sln`/`*.csproj`, `pom.xml`/`build.gradle`,
   `pubspec.yaml`, Unity (`Assets` + `ProjectSettings`), Unreal (`*.uproject`); work files by
   type (Photoshop, Premiere, After Effects, Blender, FL Studio, CapCut, Office, OBS recordings).
7. Your own folders outside the profile and on data drives get cards too.
8. Built now (read-only); the removing test on a throwaway folder comes after.

Done first (2026-11-15): launcher games found in a shared folder (D124), every installed app
listed, app icons (D125).

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

Step 1 done (2026-11-15): the **Apps** view. Every installed app as a card with its icon (desktop
app; a letter otherwise), size, number of places and last use; search; sort by size, least used
or name; Windows parts and runtimes folded into "System parts"; apps with nothing in the scan in
their own closed section. An app's page: "Can be cleared" (what the checks offered inside its
places, with pick boxes), then its places by role (needed to run, settings and data, your saves,
temporary files), each without the bytes already counted as clearable; "Show on the map" opens
the map with only that app lit.

Next: games inside launchers as their own apps (Steam libraries, Riot, HoYoPlay, Epic), rules
for app parts from the research (Prompt 4), the hub-and-lines map look for one app, then the
**Your files** view, and removing the old Apps tab from the side panel.

Step 2 done (2026-11-15): games found through their launchers' own records (D128): Steam,
Epic, Riot and HoYoPlay. Genshin Impact's 174 GB in `C:\Games` is now Genshin's, whatever the
folder is called. Not covered yet: Battle.net, EA app, Ubisoft Connect, GOG, Xbox / Microsoft
Store games.

Step 3 done (2026-11-15): each app's page draws its network: the app in the middle, a line to
every place it keeps things (circle size follows space, colour follows what it is for), and what
can be cleared inside each place hanging further out (green safe, orange probably safe). At most
12 places get their own circle; the rest are one "more places" circle. Clicking a circle opens it
in the Files view. "Show on the map" still opens the full map with only the app lit.

Step 4 done (2026-11-15): app-specific rules from the research (D129), so app pages show more of
what can be cleared, and say plainly what must stay.


Step 5 done (2026-11-16, D132): the Apps view became the **Library**: apps grouped by kind with
chips to show one kind, launchers listing their games, an app's kind changeable by the person
(remembered), and **Your files** grouped by type and kind with the folders holding the most and
the biggest files. Next: removing the old Apps tab from the side panel; more launchers
(Battle.net, EA app, Ubisoft, GOG, Xbox) so their games are found.

Step 6 done (2026-11-16, D133): **Uninstall** on an app's Library page (only in builds with
removing): the app's own uninstaller, a guided order for launchers, games a launcher manages
removed from inside the launcher, and what was left behind listed right there. Fixed the same day:
a home folder with a `package.json` or `.git` counted as one huge code project.

Still open: removing the old Apps tab from the side panel; more launchers (Battle.net, EA app,
Ubisoft, GOG, Xbox / Microsoft Store) so their games are found and grouped.

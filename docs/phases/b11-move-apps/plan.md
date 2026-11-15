# B11: Moving an app to another drive (plan)

Owner's request (2026-11-15, new 1 TB SSD): pick an app and move it to another drive so it
keeps working and the old drive is freed, with no leftovers. Owner accepted the defaults:

- **Regular apps:** copy the program folder to `<drive>:\Apps\<App name>` (changeable), check
  the copy byte for byte, move the original into the holding folder and put a junction in its
  place (shortcuts, the uninstaller and Windows' app list keep working; the registry is never
  edited). The person starts the app and confirms "It works"; then the old copy is deleted.
  "Move back" restores everything. The app must be closed; DiskCleaner never closes programs.
- Any app that passes the checks may be moved, with a warning when it is not on the list of apps
  known to work. Only the program folder moves (AppData stays).
- **Never moved:** Windows, drivers and services (anti-cheat such as Riot Vanguard), Office,
  Visual Studio, Microsoft Store apps (Windows moves those itself).
- **Games in launchers:** the launcher's own way is used and shown step by step (it updates the
  launcher's own record). Checked on 2026-11-15:
  - Steam: built in (Properties > Installed Files > Move install folder).
  - Microsoft Store / Xbox: built in (Settings > Apps > Installed apps > ... > Move).
  - HoYoPlay (Genshin Impact, Star Rail): move the folder, then "Locate game" in HoYoPlay.
  - Riot Client (VALORANT, League): move the folder, then Install > "Already installed?" and
    pick the moved folder. Riot Vanguard stays where it is.
  - Battle.net: move the folder, then "Locate the game" under Install.
  - Epic Games: no move; a workaround (install to the new folder, cancel, put the files there,
    resume so it verifies them).
  - EA app: no move; old Origin had one. GOG Galaxy: a workaround like Epic's.
  For launchers without a move, DiskCleaner can do the folder move itself (same copy, check and
  hold as above, without a junction) and then show the launcher step.
- Built after removing is switched on (it reuses the holding folder and checks).

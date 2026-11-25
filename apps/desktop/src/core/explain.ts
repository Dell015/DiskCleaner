// "What is this?" for any folder or file: what it is for, what happens if it is deleted, and the
// right way to free its space. From, in order: a list of places Windows, games and developer tools
// are known to keep things; the app that owns it (and what kind of place it is for that app); the
// checks' verdict; and, for anything unknown, clues from the scan (what is inside, which programs,
// how recent). Read-only: it only describes. It never changes what is offered for removing.

import { placeRole } from "./apps";
import { KIND } from "./format";
import type { Tree } from "./tree";
import { count, date, human } from "./util";

export interface Explanation {
  /** What it is for. */
  what: string;
  /** What happens if it is deleted. */
  ifDeleted: string;
  /** The right way to free its space, when there is one. */
  how?: string;
  /** Where the explanation comes from. */
  from: "known" | "app" | "check" | "clues";
  /** For unknown things: what the scan shows about it. */
  clues?: string[];
}

interface Known {
  /** Matched against the lower-case path with backslashes (the whole path, `$` anchored). */
  re: RegExp;
  what: string;
  ifDeleted: string;
  how?: string;
}

const STORAGE = "Settings → System → Storage → Temporary files";
/** "Settings → System → Storage → Temporary files, tick "X", then Remove files". */
const tick = (what: string): string => `${STORAGE}: tick "${what}", then Remove files.`;

/** Places whose purpose is the same on every Windows PC. Most specific first. */
const KNOWN: Known[] = [
  // Windows itself
  { re: /^[a-z]:\\windows\\winsxs$/, what: "Windows' component store: the parts of Windows kept so updates can be installed and Windows can repair itself. Much of its size is shared with System32 (the same files under two names), so it looks bigger than it is.", ifDeleted: "Windows updates and repairs break, and Windows may not start.", how: `Let Windows clean it: ${tick("Windows Update Cleanup")}` },
  { re: /^[a-z]:\\windows\\installer$/, what: "Copies of the installers of your apps (hidden). Windows needs them to repair, update or uninstall those apps.", ifDeleted: "Apps can no longer be updated, repaired or uninstalled; some stop working.", how: "Uninstall apps you no longer use; their installer copies go with them." },
  { re: /^[a-z]:\\windows\\softwaredistribution\\download$/, what: "Windows Update's downloads, kept while updates install.", ifDeleted: "Nothing lasting: Windows downloads what it still needs again.", how: tick("Windows Update Cleanup") },
  { re: /^[a-z]:\\windows\\softwaredistribution$/, what: "Windows Update's working folder: downloads and its own records.", ifDeleted: "Update history is lost and Windows Update may need repairing.", how: `Only its Download folder can go: ${tick("Windows Update Cleanup")}` },
  { re: /^[a-z]:\\windows\\system32\\driverstore$/, what: "Every driver installed on this PC, kept so devices work and can be set up again.", ifDeleted: "Devices (graphics, sound, network) can stop working.", how: "Leave it. Old graphics drivers can be removed with the maker's cleanup tool." },
  { re: /^[a-z]:\\windows\\(system32|syswow64)$/, what: "Windows' own programs and libraries.", ifDeleted: "Windows stops working.", how: "Leave it." },
  { re: /^[a-z]:\\windows\\temp$/, what: "Temporary files of Windows and installers.", ifDeleted: "Usually nothing; a running installer could fail.", how: tick("Temporary files") },
  { re: /^[a-z]:\\windows\\prefetch$/, what: "Notes Windows keeps to start your apps faster.", ifDeleted: "Apps start a little slower for a while; Windows makes them again.", how: "Leave it: it is small and comes back." },
  { re: /^[a-z]:\\windows\\fonts$/, what: "The fonts installed on this PC.", ifDeleted: "Text in apps and Windows can look wrong or fail.", how: "Remove single fonts in Settings → Personalization → Fonts." },
  { re: /^[a-z]:\\windows$/, what: "Windows itself.", ifDeleted: "The PC stops starting.", how: `Let Windows clean itself: ${STORAGE}.` },
  { re: /^[a-z]:\\windows\.old$/, what: "Your previous Windows, kept after an upgrade so you can go back.", ifDeleted: "You can no longer go back to the previous Windows. Windows deletes it by itself after about 10 days.", how: tick("Previous Windows installation(s)") },
  { re: /^[a-z]:\\\$recycle\.bin$/, what: "The Recycle Bin: things you deleted in File Explorer.", ifDeleted: "They cannot be brought back any more.", how: "Right-click the Recycle Bin on the desktop → Empty Recycle Bin." },
  { re: /^[a-z]:\\system volume information$/, what: "Restore points and shadow copies (Windows' own backups of system files), kept by System Protection.", ifDeleted: "You cannot use System Restore to undo a bad update or driver.", how: 'Search "Create a restore point" → Configure → lower the space used, or Delete old points.' },
  { re: /^[a-z]:\\pagefile\.sys$/, what: "Windows' page file: extra memory on the disk when the RAM is full.", ifDeleted: "Windows recreates it; turning it off can make apps crash when memory runs out.", how: "Leave it. Windows manages its size." },
  { re: /^[a-z]:\\hiberfil\.sys$/, what: "Hibernation file: holds what was in memory for hibernate and Fast Startup.", ifDeleted: "Windows recreates it while hibernation is on.", how: 'If you never hibernate: run "powercfg /h off" in a Terminal (Admin). Fast Startup goes with it.' },
  { re: /^[a-z]:\\swapfile\.sys$/, what: "Swap file for Microsoft Store apps, managed with the page file.", ifDeleted: "Windows recreates it.", how: "Leave it." },
  { re: /^[a-z]:\\(recovery|\$winreagent|\$sysreset)$/, what: "Windows' recovery tools, used to repair or reset Windows.", ifDeleted: "Repairing or resetting Windows may not work.", how: "Leave it." },
  { re: /^[a-z]:\\\$windows\.~(bt|ws)$/, what: "Files left from a Windows upgrade or its download.", ifDeleted: "Usually nothing once the upgrade is done.", how: tick("Windows upgrade log files" + '" and "' + "Temporary Windows installation files") },
  { re: /^[a-z]:\\program files( \(x86\))?$/, what: "Where apps install their programs.", ifDeleted: "Your apps stop working.", how: "Uninstall apps you no longer use (Library → the app → Uninstall)." },
  { re: /^[a-z]:\\programdata\\package cache$/, what: "Copies of installers (Visual Studio, .NET, C++ runtimes, ...) that their apps need to update, repair or uninstall.", ifDeleted: "Those apps may fail to update, repair or uninstall.", how: "Uninstall the apps you no longer use; their copies go with them." },
  { re: /^[a-z]:\\programdata\\microsoft\\windows\\wer$/, what: "Windows error reports: notes about apps that crashed.", ifDeleted: "Nothing you use.", how: tick("Windows error reports") },
  { re: /^[a-z]:\\programdata$/, what: "Data apps share between all users of the PC (hidden): settings, licences, downloads.", ifDeleted: "Apps lose settings or licences, or stop working.", how: "Free space app by app in the Library." },
  { re: /^[a-z]:\\users$/, what: "Everyone's personal folders on this PC.", ifDeleted: "Everyone's files and settings are lost.", how: "Look inside your own folder." },
  // A person's own folders
  { re: /^[a-z]:\\users\\[^\\]+\\appdata\\local\\temp$/, what: "Temporary files apps make while they run.", ifDeleted: "Usually nothing; an app that is open right now could lose unsaved work.", how: `Close your apps, then ${tick("Temporary files")}` },
  { re: /^[a-z]:\\users\\[^\\]+\\appdata\\local\\packages$/, what: "Data of apps from the Microsoft Store.", ifDeleted: "Those apps lose their settings, logins and downloads.", how: "Settings → Apps → Installed apps → an app → Advanced options → Reset, or uninstall it." },
  { re: /^[a-z]:\\users\\[^\\]+\\appdata\\(local|roaming|locallow)$/, what: "Apps' settings, caches and data for you (Local stays on this PC, Roaming can follow your account, LocalLow is for apps with low rights, often games).", ifDeleted: "Your apps lose settings, logins and saved data; some game saves are here.", how: "Free space app by app in the Library, or with what Rhizome offers inside." },
  { re: /^[a-z]:\\users\\[^\\]+\\appdata$/, what: "Apps' settings and data for you (hidden).", ifDeleted: "Your apps forget everything; some stop working.", how: "Free space app by app in the Library." },
  { re: /^[a-z]:\\users\\[^\\]+\\onedrive[^\\]*$/, what: "Your OneDrive: files kept in sync with the cloud.", ifDeleted: "They are deleted from the cloud and your other devices too.", how: 'To keep them in the cloud only: right-click → "Free up space".' },
  { re: /^[a-z]:\\users\\[^\\]+\\downloads$/, what: "Your downloads.", ifDeleted: "Those files are gone.", how: "Library → Your files → Downloads shows installers and archives you may not need any more." },
  { re: /^[a-z]:\\users\\[^\\]+\\(documents|desktop|pictures|videos|music)$/, what: "One of your personal folders.", ifDeleted: "Your own files are gone.", how: "Library → Your files shows what takes the space; move big things to another drive." },
  { re: /^[a-z]:\\users\\[^\\]+$/, what: "A person's own folder: their files, settings and apps' data.", ifDeleted: "Everything of that person is gone.", how: "Look inside, or use Library → Your files." },
  // Developer tools
  { re: /\\node_modules$/, what: "Packages a JavaScript project downloaded (npm, pnpm or yarn).", ifDeleted: "The project will not run until its packages are installed again.", how: 'Delete it in projects you are not working on; "npm install" brings it back.' },
  { re: /\\\.git$/, what: "A project's whole history (Git).", ifDeleted: "The project's history, branches and unsent work are lost for good.", how: 'Leave it. "git gc" can make it smaller.' },
  { re: /\\(\.venv|venv)$/, what: "A Python project's own set of packages (virtual environment).", ifDeleted: "The project will not run until the environment is made again.", how: "Delete it for projects you are not using; make it again with pip from requirements.txt." },
  { re: /\\__pycache__$/, what: "Compiled Python files, made again automatically.", ifDeleted: "Nothing: Python makes them again.", how: "Safe to clear." },
  { re: /\\\.gradle\\caches$|\\\.gradle$/, what: "Gradle's downloads and build caches (Java and Android projects).", ifDeleted: "The next build downloads what it needs again, slowly.", how: "Clear it when you are not building." },
  { re: /\\\.nuget\\packages$/, what: "NuGet packages downloaded for .NET projects.", ifDeleted: "The next build downloads them again.", how: 'Run "dotnet nuget locals all --clear".' },
  { re: /\\appdata\\local\\npm-cache$/, what: "npm's download cache.", ifDeleted: "Packages are downloaded again when needed.", how: 'Run "npm cache clean --force".' },
  { re: /\\\.cargo\\registry$/, what: "Rust crates downloaded by Cargo.", ifDeleted: "The next build downloads them again.", how: "Clear it when you are not building." },
  { re: /\\\.android\\avd$/, what: "Android emulator phones (each holds a whole phone's disk).", ifDeleted: "Those emulators are gone.", how: "Android Studio → Device Manager → delete the ones you do not use." },
  { re: /\\\.vscode\\extensions$/, what: "Visual Studio Code's extensions.", ifDeleted: "VS Code loses its extensions until you install them again.", how: "Uninstall extensions you do not use in VS Code." },
  { re: /\\docker\\wsl$|\\docker desktop$/, what: "Docker's disk: images, containers and volumes.", ifDeleted: "Your images, containers and volumes (databases inside them too) are lost.", how: 'Docker Desktop → Troubleshoot → Clean / Purge data, or "docker system prune".' },
  // Games
  { re: /\\steamapps\\common$/, what: "Your Steam games.", ifDeleted: "Steam thinks they are broken and downloads them again.", how: "Uninstall games in Steam (Library → right-click → Manage → Uninstall), or use Library here." },
  { re: /\\steamapps\\shadercache$/, what: "Steam's shader cache: graphics data prepared for your games.", ifDeleted: "Games may stutter a little the first time; Steam rebuilds it.", how: "Safe to clear." },
  { re: /\\steamapps\\downloading$/, what: "Steam downloads that have not finished.", ifDeleted: "Those downloads start again.", how: "Cancel them in Steam → Downloads." },
  { re: /\\steamapps\\workshop$/, what: "Steam Workshop items (mods) you subscribed to.", ifDeleted: "Steam downloads them again while you are subscribed.", how: "Unsubscribe in the Workshop." },
  // More of Windows
  { re: /^[a-z]:\\windows\\serviceprofiles\\networkservice\\appdata\\local\\microsoft\\windows\\deliveryoptimization(\\cache)?$/, what: "Delivery Optimization: pieces of Windows and Store updates kept to share with other PCs.", ifDeleted: "Nothing lasting: updates still arrive.", how: tick("Delivery Optimization Files") },
  { re: /^[a-z]:\\programdata\\microsoft\\search\\data$/, what: "Windows Search's index: what lets the Start menu and File Explorer find files fast.", ifDeleted: "Search is slow or finds nothing until Windows rebuilds the index.", how: "Settings → Privacy & security → Searching Windows → Advanced indexing options → Advanced → Rebuild (it can get smaller)." },
  { re: /^[a-z]:\\windows\\(minidump|livekernelreports)$|^[a-z]:\\windows\\memory\.dmp$/, what: "Crash reports Windows saved when the PC or a driver crashed.", ifDeleted: "Nothing you use; they only help find why a crash happened.", how: tick("System error memory dump files") },
  { re: /\\appdata\\local\\crashdumps$/, what: "Crash reports apps saved when they crashed.", ifDeleted: "Nothing you use.", how: "Safe to clear." },
  { re: /\\appdata\\local\\d3dscache$/, what: "DirectX shader cache: graphics data games and apps prepared before.", ifDeleted: "Games may stutter a little the first time; it is made again.", how: tick("DirectX Shader Cache") },
  { re: /^[a-z]:\\windows\\logs$/, what: "Windows' own log files.", ifDeleted: "Usually nothing; they help find problems.", how: "Leave it, or let Windows clean itself." },
  { re: /^[a-z]:\\windows\\downloaded program files$/, what: "Old web add-ons (ActiveX) for Internet Explorer.", ifDeleted: "Nothing on a current PC.", how: tick("Downloaded Program Files") },
  { re: /^[a-z]:\\\$getcurrent$/, what: "Files left from a Windows feature update.", ifDeleted: "Usually nothing once the update is done.", how: tick("Temporary Windows installation files") },
  { re: /^[a-z]:\\program files\\windowsapps$/, what: "The programs of apps from the Microsoft Store (hidden and locked).", ifDeleted: "Store apps break.", how: "Settings → Apps → Installed apps → uninstall the ones you do not use." },
  { re: /^[a-z]:\\xboxgames$/, what: "Games installed with the Xbox app.", ifDeleted: "Those games stop working.", how: "Uninstall them in the Xbox app (or move them to another drive there)." },
  { re: /\\packages\\[^\\]+\\localstate\\ext4\.vhdx$/, what: "The disk of a Linux system in WSL: all its files and programs.", ifDeleted: "That Linux system and everything in it are gone.", how: "Remove what you do not need inside Linux; the file does not shrink by itself (see Microsoft's WSL docs on compacting)." },
  // Browsers: profiles hold logins; their caches come back
  { re: /\\(google\\chrome|microsoft\\edge|bravesoftware\\brave-browser|chromium|vivaldi)\\user data\\[^\\]+\\(cache|code cache|gpucache|service worker\\cachestorage)$/, what: "A browser's cache: copies of web pages and their code, to load sites faster.", ifDeleted: "Nothing lasting: sites load a little slower until it fills again. You stay logged in.", how: "In the browser: Settings → Privacy → Clear browsing data → Cached images and files." },
  { re: /\\(google\\chrome|microsoft\\edge|bravesoftware\\brave-browser|chromium|vivaldi)\\user data$/, what: "A browser's profiles: your logins, passwords, bookmarks, history and extensions, plus its caches.", ifDeleted: "You are logged out everywhere; passwords and bookmarks not synced to an account are lost.", how: "Clear only the cache from inside the browser: Settings → Privacy → Clear browsing data → Cached images and files." },
  { re: /\\mozilla\\firefox\\profiles\\[^\\]+\\cache2$/, what: "Firefox's cache: copies of web pages.", ifDeleted: "Nothing lasting: sites load a little slower for a while.", how: "Firefox → Settings → Privacy & Security → Cookies and Site Data → Clear Data → Cached Web Content." },
  { re: /\\appdata\\roaming\\mozilla\\firefox\\profiles$/, what: "Firefox's profiles: logins, passwords, bookmarks, history and extensions.", ifDeleted: "Everything not synced to a Firefox account is lost.", how: "Clear only the cache from inside Firefox." },
  { re: /\\opera software\\opera( gx)? stable$/, what: "Opera's profile (and its cache, in the Local folder): logins, bookmarks, history.", ifDeleted: "You are logged out; bookmarks and passwords not synced are lost.", how: "Opera → Settings → Privacy & security → Clear browsing data → Cached images and files." },
  // Microsoft apps
  { re: /\\appdata\\local\\packages\\msteams_8wekyb3d8bbwe\\localcache\\microsoft\\msteams$/, what: "The new Teams' cache and settings.", ifDeleted: "Teams starts slower once and forgets your personal settings; chats stay in the cloud.", how: "Settings → Apps → Installed apps → Microsoft Teams → Advanced options → Reset." },
  { re: /\\appdata\\roaming\\microsoft\\teams$/, what: "Classic Teams' cache.", ifDeleted: "Teams starts slower once; chats stay in the cloud.", how: "Quit Teams, then clear it (Microsoft's own steps)." },
  { re: /\\microsoft\\outlook\\[^\\]+\.ost$/, what: "Outlook's offline copy of your mailbox. The mail itself is on the server.", ifDeleted: "Outlook downloads the mail again (slow); anything not yet sent may be lost.", how: "Outlook → Account Settings → your account → Change → keep less mail offline (the slider)." },
  { re: /\.pst$/, what: "An Outlook data file: mail stored only on this PC (archives, old accounts).", ifDeleted: "That mail is gone for good.", how: "Keep it, or move it to another drive or a backup." },
  // Game launchers and games
  { re: /\\appdata\\local\\epicgameslauncher\\saved\\webcache[^\\]*$/, what: "The Epic Games Launcher's web cache.", ifDeleted: "Nothing lasting: the launcher makes it again.", how: "Quit the launcher, then clear it (Epic's own steps); in the Launcher Preview: Settings → About → Clear and restart." },
  { re: /\\epic games\\launcher\\vaultcache$/, what: "Downloads of Unreal Engine assets from the Epic store (the Vault).", ifDeleted: "They are downloaded again when you add them to a project.", how: "Clear it in the Epic Games Launcher (Unreal Engine → Library → Vault) or when you no longer use those assets." },
  { re: /^[a-z]:\\riot games$/, what: "Riot's games (League of Legends, VALORANT) and the Riot Client.", ifDeleted: "The games stop working; Riot Vanguard may be left broken.", how: "Uninstall from Riot Client (Settings → the game → Uninstall), Vanguard and the client last; or use Library here." },
  { re: /\\appdata\\roaming\\\.minecraft\\saves$/, what: "Your Minecraft worlds.", ifDeleted: "Those worlds are gone for good.", how: "Keep them; back them up before deleting anything here." },
  { re: /\\appdata\\roaming\\\.minecraft$/, what: "Minecraft (Java edition): the game, versions, mods and your worlds (in saves).", ifDeleted: "Minecraft and your worlds are gone.", how: "Delete old versions or mods inside it; keep saves." },
  // Graphics drivers and PC makers
  { re: /\\appdata\\local(low)?\\nvidia(\\perdriverversion)?\\(dxcache|glcache)$/, what: "NVIDIA's shader cache: graphics data prepared for games.", ifDeleted: "Games may stutter a little at first; it is made again.", how: "NVIDIA Control Panel → Manage 3D settings → Shader Cache Size sets how big it may grow." },
  { re: /\\appdata\\local\\amd\\(dxcache|dxccache|vkcache|glcache)$/, what: "AMD's shader cache: graphics data prepared for games.", ifDeleted: "Games may stutter a little at first; it is made again.", how: "AMD Software → Gaming → Graphics → Advanced → Reset Shader Cache." },
  { re: /^[a-z]:\\amd$/, what: "Files the AMD driver installer unpacked.", ifDeleted: "Nothing for the installed driver; you would download the installer again to reinstall.", how: "Delete it once the driver works." },
  { re: /^[a-z]:\\nvidia$/, what: "Files an NVIDIA driver installer unpacked.", ifDeleted: "Nothing for the installed driver.", how: "Delete it once the driver works." },
  { re: /^[a-z]:\\(swsetup|drivers)$/, what: "Driver and app installers your PC maker (HP, Dell, Lenovo, ...) put there.", ifDeleted: "Nothing for what is installed; reinstalling a driver means downloading it again.", how: "Keep it if you may need to reinstall drivers offline; otherwise it can go." },
  // Creative and media apps
  { re: /\\appdata\\roaming\\adobe\\common\\media cache( files)?$/, what: "Adobe's media cache (Premiere Pro, After Effects, Media Encoder): prepared audio and previews of your footage.", ifDeleted: "Projects open slower once while it is made again; your footage and projects are not touched.", how: "Premiere Pro → Settings → Media Cache → Delete Unused (with projects closed)." },
  { re: /\\appdata\\local\\adobe\\oobe$/, what: "Creative Cloud's own cache.", ifDeleted: "Creative Cloud rebuilds it; quit it fully first.", how: "Quit Creative Cloud (also from the tray), then clear it." },
  { re: /\\appdata\\local\\spotify\\(storage|data)$|\\packages\\spotifyab[^\\]+\\localcache\\spotify\\data$/, what: "Spotify's cache and downloaded songs.", ifDeleted: "Downloaded songs must be downloaded again; nothing else is lost.", how: "Spotify → Settings → Storage → Clear cache (and remove downloads you no longer need)." },
  { re: /\\appdata\\roaming\\discord\\(cache|code cache|gpucache)$/, what: "Discord's cache: images and media you have seen.", ifDeleted: "Nothing lasting: it fills again.", how: "Quit Discord, then clear it." },
  { re: /\\appdata\\roaming\\obs-studio\\logs$/, what: "OBS Studio's log files.", ifDeleted: "Nothing you use.", how: "Safe to clear. Your recordings are wherever OBS saves them (Settings → Output), usually Videos." },
  { re: /\\capcut\\user data\\projects$/, what: "Your CapCut projects (drafts).", ifDeleted: "Those projects are gone for good.", how: "Delete old projects inside CapCut; export what you want to keep first." },
  // More developer tools
  { re: /\\appdata\\local\\pip\\cache$/, what: "pip's download cache (Python packages).", ifDeleted: "Packages are downloaded again when needed.", how: 'Run "py -m pip cache purge".' },
  { re: /\\(anaconda3|miniconda3|miniforge3)\\pkgs$/, what: "Conda's downloaded packages.", ifDeleted: "Environments may need packages downloaded again.", how: 'Run "conda clean --all".' },
  { re: /\\appdata\\local\\yarn\\cache$/, what: "Yarn's package cache.", ifDeleted: "Packages are downloaded again.", how: 'Run "yarn cache clean".' },
  { re: /\\appdata\\local\\pnpm\\store$|\\\.pnpm-store$/, what: "pnpm's package store, shared by your projects.", ifDeleted: "Projects need their packages installed again.", how: 'Run "pnpm store prune".' },
  { re: /\\\.m2\\repository$/, what: "Maven's downloaded libraries (Java).", ifDeleted: "The next build downloads them again.", how: "Clear it when you are not building." },
  { re: /\\go\\pkg\\mod$/, what: "Go's module cache.", ifDeleted: "The next build downloads modules again.", how: 'Run "go clean -modcache".' },
  { re: /\\appdata\\local\\pub\\cache$/, what: "Dart and Flutter's package cache.", ifDeleted: "Packages are downloaded again.", how: 'Run "flutter pub cache clean".' },
  { re: /\\\.gradle\\wrapper\\dists$/, what: "Gradle versions downloaded for your projects.", ifDeleted: "Each project downloads its Gradle version again.", how: "Delete old versions you no longer build with." },
  { re: /\\appdata\\local\\jetbrains$/, what: "JetBrains IDEs' caches, indexes and logs, one folder per IDE version.", ifDeleted: "Projects are indexed again (slow once). Folders of old IDE versions are not used any more.", how: "Delete folders of IDE versions you no longer have; in the IDE: File → Invalidate Caches." },
  { re: /\\\.vs$/, what: "Visual Studio's per-solution data (IntelliSense database, window layout).", ifDeleted: "Visual Studio makes it again; open files and layout for that solution are forgotten.", how: "Safe to delete with Visual Studio closed." },
  { re: /\\android\\sdk\\system-images$/, what: "Android emulator system images.", ifDeleted: "Emulators that use them stop working.", how: "Android Studio → SDK Manager → untick the images you do not use." },
  { re: /\\deriveddatacache$/, what: "Unreal Engine's derived data cache: prepared assets.", ifDeleted: "Projects open slower once while it is made again.", how: "Safe to clear with Unreal closed." },
  // Apps people often ask about
  { re: /\\muse hub$/, what: "Muse Hub: MuseScore's manager for its sounds, with the sound libraries it downloaded (Muse Sounds).", ifDeleted: "MuseScore falls back to basic sounds, and Muse Hub may download them again.", how: "Open Muse Hub → remove the sounds you do not use; uninstall Muse Hub (and MuseScore) if you do not use them." },
  { re: /\\(nvidia corporation\\installer2|nvidia\\installer2)$/, what: "Copies of old NVIDIA driver installers.", ifDeleted: "Usually nothing; repairing an old driver install may fail.", how: "Use NVIDIA's own cleanup or Disk Cleanup." },
];

const GONE = "It is gone for good once the Removed tab is emptied, unless you have a copy elsewhere.";
const KEEP = "Move it to another drive or to cloud storage if you want to keep it but need the space.";

/** Files no app owns, by the scan's file type. */
const FILE_TYPE: Record<string, Omit<Explanation, "from">> = {
  video: { what: "A video: yours, or one you downloaded.", ifDeleted: GONE, how: KEEP },
  image: { what: "A picture: yours, or one you downloaded.", ifDeleted: GONE, how: KEEP },
  audio: { what: "A sound or music file.", ifDeleted: GONE, how: KEEP },
  document: { what: "A document (text, PDF, spreadsheet or slides).", ifDeleted: GONE, how: KEEP },
  archive: { what: "A compressed archive (zip, rar, 7z) or a disk image (iso).", ifDeleted: "What is packed inside is gone, unless you unpacked it.", how: "If you already unpacked it or installed what it held, the archive is no longer needed." },
  installer: { what: "An installer or setup program you downloaded.", ifDeleted: "You would need to download it again to install that app.", how: "If the app is installed already, the installer is no longer needed." },
  code: { what: "A code file.", ifDeleted: "That code is gone, unless it is in Git or backed up.", how: "Keep code in a Git repository so nothing is lost." },
};

/** The lower-case path with backslashes, as the known list reads it. */
function key(path: string): string {
  return path.toLowerCase().replace(/\//g, "\\").replace(/\\+$/, "");
}

/** The known explanation for a path, if any. Exported for tests. */
export function known(path: string): Known | undefined {
  const k = key(path);
  return KNOWN.find((e) => e.re.test(k));
}

const CAT_WORDS: Record<string, string> = {
  video: "videos", image: "pictures", audio: "sound files", archive: "archives", code: "code",
  document: "documents", game_asset: "game files", installer: "installers", executable: "programs",
  temporary: "temporary files", other: "other files",
};

/** What the scan shows about something nothing explains. Bounded: at most 200,000 entries are read. */
export function clues(tree: Tree, i: number): string[] {
  const out: string[] = [];
  if (tree.kind[i] !== KIND.dir) {
    const dot = tree.names[i].lastIndexOf(".");
    out.push(dot > 0 ? `A .${tree.names[i].slice(dot + 1).toLowerCase()} file of ${human(tree.agg[i])}, changed ${date(tree.mtime[i])}.` : `A file of ${human(tree.agg[i])} without a type, changed ${date(tree.mtime[i])}.`);
    return out;
  }
  const byCat = new Map<string, number>();
  const exes: [string, number][] = [];
  const stack = [i];
  let budget = 200_000;
  while (stack.length && budget-- > 0) {
    const n = stack.pop() as number;
    if (tree.kind[n] === KIND.dir) {
      for (let c = tree.firstChild[n]; c >= 0; c = tree.nextSibling[c]) stack.push(c);
      continue;
    }
    const cat = tree.cat[n] >= 0 ? tree.categories[tree.cat[n]] ?? "other" : "other";
    byCat.set(cat, (byCat.get(cat) ?? 0) + tree.agg[n]);
    if (/\.exe$/i.test(tree.names[n])) exes.push([tree.names[n], tree.agg[n]]);
  }
  const total = Math.max(1, tree.agg[i]);
  const mix = [...byCat.entries()].sort((a, b) => b[1] - a[1]).filter(([, b]) => b / total >= 0.05).slice(0, 3);
  if (mix.length) out.push(`Mostly ${mix.map(([c, b]) => `${CAT_WORDS[c] ?? c} (${Math.round((b / total) * 100)}%)`).join(", ")}.`);
  if (exes.length) {
    const top = exes.sort((a, b) => b[1] - a[1]).slice(0, 3).map(([n]) => n);
    out.push(`Programs inside: ${top.join(", ")}${exes.length > 3 ? ` and ${count(exes.length - 3)} more` : ""}. Their names often say which app this is.`);
  }
  out.push(`${count(tree.files[i])} files; last changed ${date(tree.newest[i])}.`);
  if (budget <= 0) out.push("(Only part of it was looked at: it is very big.)");
  return out;
}

/** Explain one scan entry. */
export function explain(tree: Tree, i: number): Explanation {
  const path = tree.path(i);
  const k = known(path);
  if (k) return { what: k.what, ifDeleted: k.ifDeleted, how: k.how, from: "known" };

  const o = tree.owner[i];
  const owner = o >= 0 ? tree.owners[o] : undefined;
  if (owner?.kind === "windows") {
    return { what: "Part of Windows.", ifDeleted: "Windows or its features can stop working.", how: `Let Windows clean itself: ${STORAGE}.`, from: "app" };
  }
  if (owner?.kind === "app") {
    const name = owner.name;
    switch (placeRole(tree, i, owner.folder)) {
      case "program":
        return { what: `Part of ${name}: its program files.`, ifDeleted: `${name} stops working, and may not update or uninstall properly.`, how: `Uninstall ${name} if you do not need it (Library → ${name} → Uninstall).`, from: "app" };
      case "saves":
        return { what: `${name}'s saves and settings.`, ifDeleted: "Your progress and settings for it are gone, unless it keeps them in the cloud.", how: "Keep it, or delete it yourself only if you are done with the game for good.", from: "app" };
      case "temp":
        return { what: `Temporary files ${name} made.`, ifDeleted: "Usually nothing, unless it is open right now.", how: `Close ${name}, then clear what Rhizome offers here.`, from: "app" };
      default:
        return { what: `${name}'s settings and data (caches, logins, downloads).`, ifDeleted: `${name} may forget your settings or logins, or set itself up again; caches are made again.`, how: `Remove what Rhizome offers inside ${name}'s page in the Library, or uninstall ${name}.`, from: "app" };
    }
  }
  if (owner?.kind === "vendor") {
    return { what: `Files of apps made by ${owner.name}.`, ifDeleted: "Those apps can stop working.", how: `Uninstall the ${owner.name} apps you do not use.`, from: "app" };
  }

  const m = tree.mark[i] >= 0 ? tree.marks[tree.mark[i]] : undefined;
  if (m) {
    const ifDeleted = m.risk === "safe" ? "Nothing lasting: it is made again when needed." : m.risk === "probably_safe" ? "Usually nothing, but look first: something in it may still be wanted." : "Don't: something would break or be lost.";
    return { what: m.title, ifDeleted, how: m.offered ? "Tick it and Review: it goes to the Removed tab first." : undefined, from: "check" };
  }

  // A file nothing claims: say what kind of file it is.
  if (tree.kind[i] !== KIND.dir) {
    const cat = tree.cat[i] >= 0 ? tree.categories[tree.cat[i]] ?? "other" : "other";
    const byType = FILE_TYPE[cat];
    if (byType) return { ...byType, from: "known" };
  }

  return {
    what: "Rhizome does not know what this is: no app claims it and no check knows it.",
    ifDeleted: "Unknown. Rhizome leaves unknown things alone.",
    how: "The clues below often tell. You can also ask your AI.",
    from: "clues",
    clues: clues(tree, i),
  };
}

/** A question for an AI about something unknown, with the clues and the home folder hidden. */
export function askText(tree: Tree, i: number): string {
  const path = tree.path(i).replace(/^([a-z]:\\users\\)[^\\]+/i, "%USERPROFILE%");
  const lines = clues(tree, i);
  return `On my Windows PC, what is "${path}" (${human(tree.agg[i])}) for, and is it safe to delete? What is the right way to free its space?\n${lines.join("\n")}`;
}

/** "Can I delete it?" in plain words, from the checks' verdict (unknown means no). */
export function canDelete(risk: "safe" | "probably_safe" | "dont_touch" | null, offered: boolean): { tone: string; text: string } {
  if (risk === "safe" && offered) return { tone: "safe", text: "Yes. It is made again by itself or never needed again. Remove it with Rhizome so you can put it back." };
  if (risk === "probably_safe" && offered) return { tone: "probably_safe", text: "Probably. Look at it first; remove it with Rhizome so you can put it back." };
  if (risk === "dont_touch") return { tone: "dont_touch", text: "No. Deleting it by hand could break an app or Windows." };
  return { tone: "none", text: "Leave it: nothing says it is safe to delete. Unknown means hands off." };
}

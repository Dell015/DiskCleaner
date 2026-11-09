// A made-up disk in the `dc-view` format, for the demo and for tests. No real files are read.
// Structure and sizes are shaped like a real Windows PC so the view can be judged on it.

import type { Mark, Owner, ViewFile } from "./format";
import { rng } from "./util";

const CAT = { video: 0, image: 1, audio: 2, archive: 3, code: 4, document: 5, game: 6, installer: 7, exe: 8, temp: 9, other: 10 };
const CATEGORIES = ["video", "image", "audio", "archive", "code", "document", "game_asset", "installer", "executable", "temporary", "other"];
const EXT: Record<number, string[]> = {
  0: ["mp4", "mkv", "mov"], 1: ["jpg", "png", "webp"], 2: ["mp3", "flac", "wav"], 3: ["zip", "7z", "rar"],
  4: ["js", "ts", "json", "rs", "py", "map"], 5: ["pdf", "docx", "xlsx", "txt"], 6: ["pak", "bundle", "assets"],
  7: ["msi", "exe"], 8: ["dll", "exe", "sys"], 9: ["tmp", "log"], 10: ["dat", "bin", "db", "cache"],
};
const DAY = 86400;

class Builder {
  name: string[] = [];
  parent: number[] = [];
  kind: number[] = [];
  size: number[] = [];
  mtime: number[] = [];
  flags: number[] = [];
  cat: number[] = [];
  owners: Owner[] = [];
  assignments: [number, number, number][] = [];
  marks: Omit<Mark, "bytes">[] = [];
  private ownerIds = new Map<string, number>();
  constructor(readonly r: () => number, readonly now: number) {}

  push(parent: number, name: string, kind: number, size: number, ageDays: number, cat = -1, flags = 0): number {
    this.name.push(name);
    this.parent.push(parent);
    this.kind.push(kind);
    this.size.push(kind === 0 ? Math.max(0, Math.round(size)) : 0);
    this.mtime.push(Math.round(this.now - ageDays * DAY - this.r() * DAY));
    this.flags.push(flags);
    this.cat.push(cat);
    return this.name.length - 1;
  }
  dir(parent: number, name: string, age = 30, flags = 0): number {
    return this.push(parent, name, 1, 0, age, -1, flags);
  }
  /** A log-normal size around `median` bytes. */
  sz(median: number, spread = 1.2): number {
    const u = Math.max(1e-9, this.r());
    const z = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * this.r());
    return Math.round(median * Math.exp(spread * z));
  }
  files(parent: number, count: number, cat: number, median: number, age = 60, spread = 1.2, prefix = "f"): void {
    const exts = EXT[cat];
    for (let i = 0; i < count; i++) {
      const ext = exts[Math.floor(this.r() * exts.length)];
      this.push(parent, `${prefix}${i.toString(36)}.${ext}`, 0, this.sz(median, spread), age * (0.2 + this.r() * 1.6), cat);
    }
  }
  owner(node: number, name: string, kind: Owner["kind"] = "app", publisher?: string): void {
    let id = this.ownerIds.get(name);
    if (id === undefined) {
      id = this.owners.length;
      this.owners.push(publisher ? { name, kind, publisher } : { name, kind });
      this.ownerIds.set(name, id);
    }
    this.assignments.push([node, id, 2]);
  }
  mark(node: number, m: Omit<Mark, "bytes" | "node">): void {
    this.marks.push({ ...m, node });
  }
}

/** Build a demo disk with roughly `target` entries (at least about 2,000). */
export function makeDemo(target = 150_000, seed = 42, now = 1_762_000_000): ViewFile {
  const r = rng(seed);
  const b = new Builder(r, now);
  const s = Math.max(0.05, target / 48_000);
  const n = (x: number): number => Math.max(1, Math.round(x * s));

  const pc = b.dir(-1, "This PC", 0);
  const c = b.dir(pc, "C:\\", 0);

  // Windows: many small system files, one big update cache.
  const win = b.dir(c, "Windows", 5, 8);
  b.owner(win, "Windows", "windows");
  const sys32 = b.dir(win, "System32", 5, 8);
  b.files(sys32, n(2500), CAT.exe, 400_000, 200);
  const drivers = b.dir(sys32, "drivers", 40, 8);
  b.files(drivers, n(300), CAT.exe, 120_000, 300);
  const winsxs = b.dir(win, "WinSxS", 10, 8);
  for (let i = 0; i < n(1500); i++) {
    const d = b.dir(winsxs, `amd64_microsoft-windows-${i.toString(36)}_31bf3856ad364e35`, 300);
    b.files(d, 2 + Math.floor(r() * 7), CAT.exe, 250_000, 300);
  }
  const sd = b.dir(win, "SoftwareDistribution", 3);
  const sdd = b.dir(sd, "Download", 3);
  for (let i = 0; i < n(6); i++) b.files(b.dir(sdd, `${(r() * 1e16).toString(16)}`, 20), 15, CAT.other, 30_000_000, 20, 1.5);
  b.mark(sdd, { risk: "probably_safe", source: "rule", title: "Windows Update downloads", reason: "update_cache", detail: "rule windows_update_download", evidence: [], offered: true });
  const wtemp = b.dir(win, "Temp", 2);
  b.files(wtemp, n(400), CAT.temp, 80_000, 10, 2);
  b.mark(wtemp, { risk: "safe", source: "rule", title: "Windows temporary files", reason: "temp", detail: "rule windows_temp", evidence: [], offered: true });
  const inst = b.dir(win, "Installer", 100, 16);
  b.files(inst, n(120), CAT.installer, 25_000_000, 400, 1.3);
  b.files(b.dir(win, "Fonts", 400), n(300), CAT.other, 300_000, 900);
  b.push(c, "pagefile.sys", 0, 17_179_869_184, 0, CAT.other, 8 | 16);
  b.push(c, "hiberfil.sys", 0, 13_421_772_800, 1, CAT.other, 8 | 16);

  // Program Files: apps with programs, resources and language files.
  const apps: [string, string, number, string][] = [
    ["Google\\Chrome", "Google Chrome", 1.0, "Google LLC"], ["Microsoft Visual Studio", "Visual Studio", 3.0, "Microsoft"],
    ["Docker", "Docker Desktop", 2.0, "Docker Inc."], ["nodejs", "Node.js", 0.6, "OpenJS"], ["Mozilla Firefox", "Firefox", 0.8, "Mozilla"],
    ["NVIDIA Corporation", "NVIDIA Graphics Driver", 1.5, "NVIDIA"], ["LGHUB", "Logitech G HUB", 0.7, "Logitech"],
    ["Blender Foundation", "Blender", 1.3, "Blender Foundation"], ["OBS Studio", "OBS Studio", 0.6, "OBS Project"],
    ["Git", "Git", 0.5, "The Git Development Community"], ["7-Zip", "7-Zip", 0.1, "Igor Pavlov"], ["VideoLAN", "VLC media player", 0.4, "VideoLAN"],
  ];
  const pf = b.dir(c, "Program Files", 20);
  const pf86 = b.dir(c, "Program Files (x86)", 20);
  const app = (under: number, path: string, label: string, weight: number, publisher: string, age = 40): number => {
    let at = under;
    for (const part of path.split("\\")) at = b.dir(at, part, age);
    b.owner(at, label, "app", publisher);
    b.files(at, n(8 * weight), CAT.exe, 2_000_000 * weight, age);
    const bin = b.dir(at, "bin", age);
    b.files(bin, n(60 * weight), CAT.exe, 900_000, age);
    const res = b.dir(at, "resources", age);
    b.files(res, n(40 * weight), CAT.game, 3_000_000 * weight, age, 1.6);
    const loc = b.dir(at, "locales", age);
    b.files(loc, n(25), CAT.other, 400_000, age);
    if (r() < 0.6) {
      const pl = b.dir(at, "plugins", age);
      for (let i = 0; i < n(6 * weight); i++) b.files(b.dir(pl, `plugin-${i}`, age), 3 + Math.floor(r() * 10), CAT.exe, 300_000, age);
    }
    return at;
  };
  for (const [path, label, w, pub] of apps) app(pf, path, label, w, pub);
  const lgOld = b.dir(pf, "LGHUB.old", 500);
  b.files(lgOld, n(80), CAT.exe, 2_500_000, 500);
  b.mark(lgOld, { risk: "probably_safe", source: "leftover", title: "LGHUB: old backup copy", reason: "leftover_backup_copy", detail: "Named like a backup of an app that is still installed", confidence: "high", evidence: [{ points: 50, text: "named like a backup of LGHUB, which is still here" }, { points: 30, text: "contains programs, but no installed app claims this folder" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: true });
  const java = b.dir(pf, "Java", 450);
  const jdk = b.dir(java, "jdk-17", 450);
  b.files(b.dir(jdk, "bin", 450), n(50), CAT.exe, 200_000, 450);
  b.files(b.dir(jdk, "lib", 450), n(200), CAT.other, 900_000, 450, 1.5);
  b.mark(java, { risk: "dont_touch", source: "leftover", title: "Java: programs nobody registered", reason: "low_confidence", detail: "Shown only: one weak kind of evidence (a portable or hand-installed tool looks the same)", confidence: "low", evidence: [{ points: 30, text: "contains programs, but no installed app claims this folder" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: false });
  const wsl = b.dir(pf, "WSL", 30);
  b.push(wsl, "system.vhd", 0, 2_400_000_000, 30, CAT.other);
  b.mark(wsl, { risk: "dont_touch", source: "refused", title: "Refused (leftover)", reason: "leftover_unregistered_program", detail: "it contains something protected: C:\\Program Files\\WSL\\system.vhd", evidence: [], offered: false });

  // Games: few, very large files.
  const steam = app(pf86, "Steam", "Steam", 1.0, "Valve");
  const common = b.dir(b.dir(steam, "steamapps", 10), "common", 10);
  for (const g of ["Cyberpunk 2077", "Baldur's Gate 3", "Counter-Strike 2", "Elden Ring"]) {
    const gd = b.dir(common, g, 15);
    b.files(gd, n(40), CAT.game, 350_000_000, 60, 1.0);
    b.files(b.dir(gd, "bin", 15), n(30), CAT.exe, 5_000_000, 60);
    const data = b.dir(gd, "data", 15);
    for (let i = 0; i < n(5); i++) b.files(b.dir(data, `chunk${i}`, 30), 20, CAT.game, 40_000_000, 60, 1.1);
  }
  b.mark(steam, { risk: "dont_touch", source: "leftover", title: "Steam: app that can hold installed games", reason: "may_hold_installed_games", detail: "Shown only: launchers keep installed games here", confidence: "medium", evidence: [{ points: 60, text: "named like Steam" }], offered: false });
  app(pf86, "Common Files\\Adobe", "Adobe Shared", 0.4, "Adobe");

  // ProgramData: installer caches and app leftovers.
  const pd = b.dir(c, "ProgramData", 30, 16);
  const pc2 = b.dir(pd, "Package Cache", 60);
  for (let i = 0; i < n(80); i++) b.files(b.dir(pc2, `{${(r() * 1e16).toString(16)}}`, 200), 1 + Math.floor(r() * 3), CAT.installer, 8_000_000, 300);
  const any = b.dir(pd, "AnyDesk", 700);
  b.files(any, 12, CAT.other, 20_000, 700);
  b.mark(any, { risk: "probably_safe", source: "leftover", title: "AnyDesk: known app, not installed", reason: "leftover_known_app_removed", detail: "Folder of a well-known app that is no longer installed", confidence: "high", evidence: [{ points: 60, text: "named like AnyDesk, which is not installed any more" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: true });
  const nortonPd = b.dir(pd, "Norton", 600);
  b.files(b.dir(nortonPd, "Antivirus", 600), n(60), CAT.other, 1_500_000, 600);
  b.mark(nortonPd, { risk: "dont_touch", source: "refused", title: "Refused (leftover)", reason: "leftover_known_app_removed", detail: "it contains a file with more than one name (a hard link)", evidence: [], offered: false });
  const nv = b.dir(pd, "NVIDIA Corporation", 50);
  b.files(b.dir(nv, "Downloader", 50), n(10), CAT.installer, 600_000_000, 80);

  // The user's profile.
  const users = b.dir(c, "Users", 1);
  const me = b.dir(users, "demo", 1);
  const ad = b.dir(me, "AppData", 1, 16);
  const local = b.dir(ad, "Local", 1);
  const roaming = b.dir(ad, "Roaming", 1);
  const low = b.dir(ad, "LocalLow", 30);
  b.push(me, "Application Data", 2, 0, 900, -1, 1 | 16);

  const temp = b.dir(local, "Temp", 0);
  b.files(temp, n(2000), CAT.temp, 60_000, 30, 2);
  for (let i = 0; i < n(40); i++) b.files(b.dir(temp, `tmp${i.toString(36)}`, 40), 3 + Math.floor(r() * 20), CAT.temp, 200_000, 40, 2);
  b.mark(temp, { risk: "safe", source: "rule", title: "Your temporary files", reason: "temp", detail: "rule user_temp", evidence: [], offered: true });
  const chrome = b.dir(b.dir(local, "Google", 1), "Chrome", 1);
  b.owner(chrome, "Google Chrome", "app", "Google LLC");
  const ud = b.dir(b.dir(chrome, "User Data", 1), "Default", 1);
  const cache = b.dir(b.dir(ud, "Cache", 0), "Cache_Data", 0);
  b.files(cache, n(3000), CAT.other, 40_000, 15, 1.8);
  b.mark(cache, { risk: "safe", source: "rule", title: "Chrome cache", reason: "browser_cache", detail: "rule chrome_cache", evidence: [], offered: true });
  b.files(b.dir(ud, "IndexedDB", 2), n(200), CAT.other, 300_000, 50);
  const npm = b.dir(local, "npm-cache", 3);
  const cacache = b.dir(npm, "_cacache", 3);
  for (let i = 0; i < n(256); i++) b.files(b.dir(cacache, i.toString(16).padStart(2, "0"), 30), 2 + Math.floor(r() * 12), CAT.other, 30_000, 60, 1.6);
  b.mark(npm, { risk: "safe", source: "rule", title: "npm download cache", reason: "package_cache", detail: "rule npm_cache", evidence: [], offered: true });
  for (const [path, label, w] of [["Discord", "Discord", 0.8], ["Spotify", "Spotify", 1.2], ["Microsoft\\Teams", "Microsoft Teams", 0.7]] as [string, string, number][]) {
    let at = local;
    for (const part of path.split("\\")) at = b.dir(at, part, 3);
    b.owner(at, label);
    b.files(at, n(30 * w), CAT.exe, 1_500_000, 10);
    const cc = b.dir(at, "Cache", 1);
    b.files(cc, n(400 * w), CAT.other, 50_000, 10, 1.6);
    b.mark(cc, { risk: "safe", source: "rule", title: `${label} cache`, reason: "app_cache", detail: "rule app_cache", evidence: [], offered: true });
  }
  const avast = b.dir(local, "Avast Software", 800);
  b.files(avast, 6, CAT.other, 10_000, 800);
  b.mark(avast, { risk: "probably_safe", source: "leftover", title: "Avast: known app, not installed", reason: "leftover_known_app_removed", detail: "Folder of a well-known app that is no longer installed", confidence: "high", evidence: [{ points: 60, text: "named like Avast, which is not installed any more" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: true });
  const crash = b.dir(local, "CrashDumps", 100);
  b.files(crash, n(12), CAT.other, 400_000_000, 100);
  b.mark(crash, { risk: "probably_safe", source: "rule", title: "Crash dumps", reason: "crash_dumps", detail: "rule crash_dumps", evidence: [], offered: true });
  const android = b.dir(local, "Android", 200);
  const sdk = b.dir(android, "Sdk", 200);
  for (const part of ["platforms", "system-images", "build-tools", "emulator"]) {
    const d = b.dir(sdk, part, 200);
    for (let i = 0; i < n(4); i++) b.files(b.dir(d, `${part}-${30 + i}`, 220), 40, CAT.other, 8_000_000, 220, 1.4);
  }
  b.mark(android, { risk: "dont_touch", source: "leftover", title: "Android: programs nobody registered", reason: "may_hold_personal_data", detail: "Shown only: may hold personal data (virtual disks)", confidence: "low", evidence: [{ points: 30, text: "contains programs, but no installed app claims this folder" }], offered: false });

  const norton = b.dir(roaming, "Norton", 700);
  b.files(b.dir(norton, "logs", 700), n(30), CAT.other, 200_000, 700);
  b.mark(norton, { risk: "probably_safe", source: "leftover", title: "Norton: known app, not installed", reason: "leftover_known_app_removed", detail: "Folder of a well-known app that is no longer installed", confidence: "high", evidence: [{ points: 60, text: "named like Norton, which is not installed any more" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: true });
  const nortonLow = b.dir(low, "Norton", 700);
  b.files(nortonLow, n(10), CAT.other, 250_000, 700);
  b.mark(nortonLow, { risk: "probably_safe", source: "leftover", title: "Norton: known app, not installed", reason: "leftover_known_app_removed", detail: "Folder of a well-known app that is no longer installed", confidence: "high", evidence: [{ points: 60, text: "named like Norton, which is not installed any more" }, { points: 30, text: "nothing inside was changed for a year or more" }], offered: true });
  const code = b.dir(roaming, "Code", 2);
  b.owner(code, "Visual Studio Code", "app", "Microsoft");
  b.files(b.dir(code, "CachedData", 5), n(300), CAT.other, 200_000, 20);
  b.files(b.dir(code, "logs", 1), n(200), CAT.temp, 30_000, 10);

  // Personal folders: documents, projects with node_modules, pictures, videos, downloads.
  const docs = b.dir(me, "Documents", 2);
  b.files(docs, n(300), CAT.document, 400_000, 300, 1.5);
  const proj = b.dir(docs, "Projects", 2);
  for (const p of ["portfolio", "iot-dashboard", "karela", "game-jam"]) {
    const pd2 = b.dir(proj, p, 5);
    b.files(b.dir(pd2, "src", 5), n(80), CAT.code, 8_000, 20);
    const nm = b.dir(pd2, "node_modules", 30);
    for (let i = 0; i < n(500); i++) {
      const pkg = b.dir(nm, `pkg-${i.toString(36)}`, 60);
      b.files(pkg, 2 + Math.floor(r() * 7), CAT.code, 6_000, 60, 1.5);
      if (r() < 0.3) b.files(b.dir(pkg, "dist", 60), 2 + Math.floor(r() * 10), CAT.code, 20_000, 60, 1.5);
    }
  }
  const pics = b.dir(me, "Pictures", 3);
  for (const y of ["2021", "2022", "2023", "2024", "2025"]) b.files(b.dir(pics, y, 100), n(600), CAT.image, 3_500_000, 400, 0.6, "IMG_");
  const vids = b.dir(me, "Videos", 10);
  b.files(vids, n(60), CAT.video, 1_200_000_000, 200, 1.0, "VID_");
  b.files(b.dir(vids, "Captures", 5), n(80), CAT.video, 300_000_000, 60, 1.0, "clip_");
  const dl = b.dir(me, "Downloads", 0);
  b.files(dl, n(120), CAT.installer, 90_000_000, 120, 1.4, "setup-");
  b.files(dl, n(60), CAT.archive, 300_000_000, 200, 1.4);
  // Installers of apps that are installed, an archive next to what was unpacked from it, a
  // portable tool and a loose program: what the Overview should point out.
  for (const [name, mb, age] of [["ChromeSetup.exe", 1.4, 400], ["Blender-4.2.1-windows-x64.msi", 330, 200], ["Git-2.46.0-64-bit.exe", 65, 150], ["OBS-Studio-30.2-Full-Installer-x64.exe", 140, 90], ["DiscordSetup.exe", 110, 30]] as const) {
    b.push(dl, name, 0, mb * 1_000_000, age, CAT.installer);
  }
  b.push(dl, "game-mods.zip", 0, 2_400_000_000, 500, CAT.archive);
  b.files(b.dir(dl, "game-mods", 500), n(40), CAT.game, 50_000_000, 500);
  const hw = b.dir(dl, "HWiNFO64", 250);
  b.push(hw, "HWiNFO64.exe", 0, 12_000_000, 250, CAT.exe);
  b.push(dl, "speedtest.exe", 0, 4_000_000, 40, CAT.exe);
  b.files(dl, n(200), CAT.document, 1_000_000, 300, 1.5);
  b.push(dl, "Win11_23H2_x64.iso", 0, 6_800_000_000, 700, CAT.archive);
  // Tools unpacked from the internet, outside Program Files: nobody installed them.
  const tools = b.dir(c, "Tools", 300);
  for (const [dir, exe] of [["Rufus", "rufus-4.5.exe"], ["CPU-Z", "cpuz_x64.exe"], ["platform-tools", "adb.exe"]]) {
    const t = b.dir(tools, dir, 300);
    b.push(t, exe, 0, 9_000_000, 300, CAT.exe);
    b.files(t, 3, CAT.other, 400_000, 300);
  }
  const music = b.dir(me, "Music", 300);
  b.files(music, n(500), CAT.audio, 8_000_000, 600, 0.5);
  const od = b.dir(me, "OneDrive", 1);
  for (let i = 0; i < n(300); i++) b.push(od, `cloud-file-${i}.docx`, 0, 0, 100, CAT.document, 4);

  // What Windows knows about each app: version, install date, last start, uninstaller. A few
  // apps own nothing in the scanned folders; one has no record of being started.
  const ownNothing: [string, string][] = [["Microsoft Edge WebView2 Runtime", "Microsoft Corporation"], ["Zoom Workplace", "Zoom Communications"], ["WinRAR 7.01 (64-bit)", "win.rar GmbH"]];
  for (const [name, publisher] of ownNothing) b.owners.push({ name, kind: "app", publisher });
  b.owners.forEach((o, i) => {
    if (o.kind !== "app") return;
    o.version = `${1 + (i % 9)}.${i % 7}.${(i * 13) % 50}`;
    const installedDays = 30 + Math.floor(r() * 1200);
    o.installed = new Date((now - installedDays * DAY) * 1000).toISOString().slice(0, 10);
    o.uninstaller = i % 11 !== 3;
    o.stated_bytes = Math.round(r() * 2e9);
    if (i % 5 !== 2) {
      o.last_used = Math.round(now - Math.min(installedDays, Math.floor(r() ** 2 * 900)) * DAY);
      o.runs = 1 + Math.floor(r() * 400);
    }
  });

  // Keep the disk a realistic size whatever the number of entries: scale file sizes so the
  // whole demo holds about 620 GB (pagefile and other big files keep their share).
  const raw = b.size.reduce((a, x) => a + x, 0);
  const k = 620e9 / Math.max(1, raw);
  for (let i = 0; i < b.size.length; i++) b.size[i] = Math.round(b.size[i] * k);
  // Sizes of marks: the total of their folder.
  const agg = b.size.slice();
  for (let i = b.name.length - 1; i > 0; i--) agg[b.parent[i]] += agg[i];
  const marks: Mark[] = b.marks.map((m) => ({ ...m, bytes: m.source === "refused" ? 0 : agg[m.node] }));

  return {
    schema: "dc-view",
    schema_version: 1,
    tool_version: "demo",
    created_unix: now,
    roots: ["C:\\"],
    categories: CATEGORIES,
    nodes: { name: b.name, parent: b.parent, kind: b.kind, size: b.size, mtime: b.mtime, flags: b.flags, cat: b.cat },
    owners: b.owners,
    assignments: b.assignments,
    marks,
  };
}

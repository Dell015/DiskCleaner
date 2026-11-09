// The Overview: everything on the disk at a glance, worked out from the scanned tree.
// Pure functions over the tree; read-only (nothing here can change a file).

import { KIND, type Owner } from "./format";
import type { Tree } from "./tree";

const DAY = 86_400;
const YEAR = 365 * DAY;

// ---------- file types ----------

export type GroupId = "installers" | "archives" | "images" | "videos" | "pictures" | "music" | "documents" | "programs" | "code" | "other";

export const GROUPS: { id: GroupId; label: string }[] = [
  { id: "videos", label: "Videos" },
  { id: "installers", label: "Installers" },
  { id: "archives", label: "Archives (zip, rar, 7z)" },
  { id: "images", label: "Disk images (iso, vhd)" },
  { id: "pictures", label: "Pictures" },
  { id: "music", label: "Music and audio" },
  { id: "documents", label: "Documents" },
  { id: "programs", label: "Programs" },
  { id: "code", label: "Code and projects" },
  { id: "other", label: "Everything else" },
];

const EXT: Record<string, GroupId> = {};
const add = (g: GroupId, list: string): void => list.split(" ").forEach((e) => (EXT[e] = g));
add("videos", "mp4 mkv avi mov wmv webm m4v flv mpg mpeg ts m2ts 3gp vob");
add("installers", "msi msix msixbundle appx appxbundle");
add("archives", "zip rar 7z tar gz tgz bz2 xz zst cab lz lzh");
add("images", "iso img vhd vhdx vmdk dmg wim esd bin cue");
add("pictures", "jpg jpeg png gif bmp webp heic heif tif tiff raw cr2 nef arw dng psd svg ico");
add("music", "mp3 flac wav aac ogg m4a wma opus aiff mid midi");
add("documents", "pdf doc docx xls xlsx ppt pptx odt ods odp txt rtf md csv epub mobi");
add("programs", "exe com bat cmd ps1 jar appimage");
add("code", "js ts py rs c cpp h cs java go rb php html css json xml yml yaml toml lock sln csproj");

/** File extension, lower case, without the dot ("" if none). */
export function ext(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1).toLowerCase() : "";
}

const INSTALLER_WORDS = /(^|[^a-z])(setup|install|installer|installation|instalar|update|updater)([^a-z]|$)|setup|installer/i;

/** Does a program file look like an installer ("ChromeSetup.exe", "vlc-3.0-win64.exe" is not)? */
export function looksLikeInstaller(name: string): boolean {
  const e = ext(name);
  if (EXT[e] === "installers") return true;
  return e === "exe" && INSTALLER_WORDS.test(name.replace(/\.exe$/i, ""));
}

/** Which group a file belongs to. With `apps`, a program named like an installed app counts as its installer. */
export function groupOf(name: string, apps?: AppMatcher): GroupId {
  if (looksLikeInstaller(name)) return "installers";
  const g = EXT[ext(name)] ?? "other";
  if (g === "programs" && apps && ext(name) === "exe" && apps.match(name) >= 0) return "installers";
  return g;
}

// ---------- matching an installer to an installed app ----------

const GENERIC = new Set(["setup", "install", "installer", "update", "updater", "win", "windows", "x64", "x86", "amd64", "arm64", "win64", "win32", "64bit", "32bit", "bit", "full", "online", "offline", "web", "client", "app", "desktop", "free", "pro", "microsoft", "software", "tools", "tool", "driver", "drivers", "runtime", "edition", "version", "latest", "stable", "release", "inc", "corporation", "the", "and", "for"]);

/** Words of a name: split on separators and camelCase, lower case, numbers and generic words dropped. */
export function words(name: string): string[] {
  return name
    .replace(/\.[a-z0-9]{1,5}$/i, "")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((w) => w.length >= 3 && !/^\d+$/.test(w) && !/^v?\d+(\.\d+)*$/.test(w) && !GENERIC.has(w));
}

/** Matches file names to installed apps (the apps' words are worked out once). */
export class AppMatcher {
  private apps: { owner: number; words: string[] }[] = [];
  constructor(owners: Owner[]) {
    owners.forEach((o, i) => {
      if (o.kind === "app") this.apps.push({ owner: i, words: words(o.name) });
    });
  }

  /**
   * The installed app a file is probably for, or -1 (a hint, never a fact). The app with the most
   * matching letters wins; a match needs a word of 4+ letters, two words, or all of the app's words.
   */
  match(name: string): number {
    const own = new Set(words(name));
    const glued = name.toLowerCase().replace(/\.[a-z0-9]{1,5}$/, "").replace(/[^a-z0-9]/g, "");
    let best = -1;
    let bestScore = 0;
    for (const a of this.apps) {
      let score = 0;
      let hits = 0;
      let long = false;
      for (const w of a.words) {
        if (own.has(w) || (w.length >= 4 && glued.startsWith(w))) {
          score += w.length;
          hits++;
          if (w.length >= 4) long = true;
        }
      }
      if ((long || hits >= 2 || (hits > 0 && hits === a.words.length)) && score > bestScore) {
        best = a.owner;
        bestScore = score;
      }
    }
    return best;
  }
}

/** The installed app an installer file is probably for, or -1 (a hint, never a fact). */
export function installerFor(name: string, owners: Owner[]): number {
  return new AppMatcher(owners).match(name);
}

// ---------- where the space goes ----------

export type Bucket = "windows" | "apps" | "yours" | "other";
export const BUCKETS: { id: Bucket; label: string }[] = [
  { id: "apps", label: "Apps and games" },
  { id: "yours", label: "Your files" },
  { id: "windows", label: "Windows" },
  { id: "other", label: "Everything else" },
];
const USER_FOLDERS = new Set(["downloads", "documents", "desktop", "pictures", "videos", "music", "onedrive", "saved games", "contacts", "favorites", "links"]);

/** The `Users\<name>\<folder>` folders (Downloads, Documents, ...), by index. */
export function userFolders(tree: Tree): number[] {
  const out: number[] = [];
  for (let i = 0; i < tree.n; i++) {
    if (tree.kind[i] !== KIND.dir) continue;
    const p = tree.parent[i];
    const pp = p >= 0 ? tree.parent[p] : -1;
    if (pp < 0 || tree.names[pp].toLowerCase() !== "users") continue;
    const n = tree.names[i].toLowerCase();
    if (USER_FOLDERS.has(n) || n.startsWith("onedrive")) out.push(i);
  }
  return out;
}

/** Bytes in each bucket. Every byte is counted once (sizes of entries themselves, not folders). */
export function spaceBuckets(tree: Tree): Record<Bucket, number> {
  const out: Record<Bucket, number> = { windows: 0, apps: 0, yours: 0, other: 0 };
  const mine = new Uint8Array(tree.n);
  for (const f of userFolders(tree)) mine[f] = 1;
  for (let i = 1; i < tree.n; i++) {
    const p = tree.parent[i];
    if (p >= 0 && mine[p]) mine[i] = 1;
    const bytes = tree.own[i];
    if (!bytes) continue;
    const o = tree.owner[i];
    const kind = o >= 0 ? tree.owners[o]?.kind : undefined;
    if (kind === "windows") out.windows += bytes;
    else if (kind === "app" || kind === "vendor") out.apps += bytes;
    else if (mine[i]) out.yours += bytes;
    else out.other += bytes;
  }
  return out;
}

// ---------- apps ----------

export interface AppRow {
  owner: number;
  /** Bytes in the scanned folders (0 if it owns nothing there). */
  bytes: number;
  /** Top folders it owns, biggest first. */
  places: number[];
}

/** Every installed app, biggest first (Windows and publishers' shared folders are not apps). */
export function appRows(tree: Tree): AppRow[] {
  const per = new Map<number, AppRow>();
  tree.owners.forEach((o, i) => {
    if (o.kind === "app") per.set(i, { owner: i, bytes: 0, places: [] });
  });
  for (let i = 0; i < tree.n; i++) {
    const o = tree.ownerHere[i];
    if (o < 0) continue;
    const above = tree.parent[i] >= 0 ? tree.owner[tree.parent[i]] : -1;
    if (above === o) continue;
    const row = per.get(o);
    if (!row) continue;
    row.bytes += tree.agg[i];
    row.places.push(i);
  }
  const rows = [...per.values()];
  for (const r of rows) r.places.sort((a, b) => tree.agg[b] - tree.agg[a]);
  return rows.sort((a, b) => b.bytes - a.bytes || (tree.owners[b.owner].stated_bytes ?? 0) - (tree.owners[a.owner].stated_bytes ?? 0));
}

// ---------- programs nobody installed ----------

const SYSTEM_TOP = new Set(["windows", "$recycle.bin", "system volume information", "recovery", "boot", "efi", "$windows.~bt", "$windows.~ws", "msocache", "programdata"]);

function underSystem(tree: Tree, i: number): boolean {
  for (let a = i; a > 0; a = tree.parent[a]) {
    const p = tree.parent[a];
    // A drive's top folder (its parent is the drive itself, whose parent is the shared top).
    if (p > 0 && tree.parent[p] === 0 && SYSTEM_TOP.has(tree.names[a].toLowerCase())) return true;
  }
  return false;
}

export interface Portable { node: number; programs: number }

/**
 * Folders with programs in them that no installed app claims (portable tools, games unpacked
 * from the internet), outermost only, plus loose programs in such places. Installers are left out.
 */
export function portablePrograms(tree: Tree): Portable[] {
  const apps = new AppMatcher(tree.owners);
  const program = (i: number): boolean => tree.kind[i] === KIND.file && ext(tree.names[i]) === "exe" && groupOf(tree.names[i], apps) === "programs";
  const counts = new Map<number, number>();
  for (let i = 1; i < tree.n; i++) {
    if (!program(i)) continue;
    if (tree.owner[i] >= 0) continue;
    const p = tree.parent[i];
    counts.set(p, (counts.get(p) ?? 0) + 1);
  }
  const isUserFolder = (f: number): boolean => {
    const p = tree.parent[f];
    const pp = p >= 0 ? tree.parent[p] : -1;
    return pp >= 0 && tree.names[pp].toLowerCase() === "users" && USER_FOLDERS.has(tree.names[f].toLowerCase());
  };
  const inTemp = (f: number): boolean => {
    for (let a = f; a > 0; a = tree.parent[a]) if (tree.names[a].toLowerCase() === "temp") return true;
    return false;
  };
  const folders = [...counts.keys()].filter((f) => !underSystem(tree, f) && !inTemp(f));
  // A user folder (Downloads, Desktop) is not itself a program: its loose programs are listed one
  // by one, and folders inside it still count on their own.
  const set = new Set(folders.filter((f) => !isUserFolder(f)));
  const out: Portable[] = [];
  for (const f of folders) {
    if (isUserFolder(f)) {
      for (let c = tree.firstChild[f]; c >= 0; c = tree.nextSibling[c]) {
        if (program(c) && tree.owner[c] < 0) out.push({ node: c, programs: 1 });
      }
      continue;
    }
    let outer = true;
    for (let a = tree.parent[f]; a > 0; a = tree.parent[a]) if (set.has(a)) { outer = false; break; }
    if (!outer) continue;
    let total = 0;
    for (const g of set) if (g === f || tree.isAncestor(f, g)) total += counts.get(g) ?? 0;
    out.push({ node: f, programs: total });
  }
  return out.sort((a, b) => tree.agg[b.node] - tree.agg[a.node]);
}

// ---------- sorting a messy folder ----------

export interface Group {
  id: GroupId;
  label: string;
  bytes: number;
  count: number;
  /** Not changed for a year or more. */
  oldBytes: number;
  oldCount: number;
  /** The biggest files of the group (up to 200). */
  top: number[];
}

export interface Hint { kind: "installer_of_installed" | "unpacked" | "untouched"; node: number; other: number }

export interface Sorted {
  root: number;
  groups: Group[];
  /** Direct entries with something to say about them. */
  hints: Hint[];
}

/** Insert `i` into `top` (sorted biggest first) keeping at most `max`. */
function keepTop(tree: Tree, top: number[], i: number, max: number): void {
  const s = tree.agg[i];
  if (top.length >= max && s <= tree.agg[top[top.length - 1]]) return;
  let lo = 0;
  let hi = top.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (tree.agg[top[mid]] >= s) lo = mid + 1;
    else hi = mid;
  }
  top.splice(lo, 0, i);
  if (top.length > max) top.pop();
}

/** Every file below `root`, by type, with ages; and hints about the folder's own entries. */
export function sortFolder(tree: Tree, root: number, now: number): Sorted {
  const apps = new AppMatcher(tree.owners);
  const groups = new Map<GroupId, Group>(GROUPS.map((g) => [g.id, { ...g, bytes: 0, count: 0, oldBytes: 0, oldCount: 0, top: [] }]));
  const stack = [root];
  while (stack.length) {
    const d = stack.pop()!;
    for (let c = tree.firstChild[d]; c >= 0; c = tree.nextSibling[c]) {
      if (tree.kind[c] === KIND.dir) {
        stack.push(c);
        continue;
      }
      if (tree.kind[c] !== KIND.file) continue;
      const g = groups.get(groupOf(tree.names[c], apps))!;
      g.bytes += tree.agg[c];
      g.count++;
      if (tree.mtime[c] > 0 && now - tree.mtime[c] >= YEAR) {
        g.oldBytes += tree.agg[c];
        g.oldCount++;
      }
      keepTop(tree, g.top, c, 200);
    }
  }
  const hints: Hint[] = [];
  const children: number[] = [];
  for (let c = tree.firstChild[root]; c >= 0; c = tree.nextSibling[c]) children.push(c);
  const folderByName = new Map<string, number>();
  for (const c of children) if (tree.kind[c] === KIND.dir) folderByName.set(tree.names[c].toLowerCase(), c);
  for (const c of children) {
    const name = tree.names[c];
    if (tree.kind[c] === KIND.file) {
      if (groupOf(name, apps) === "installers") {
        const app = apps.match(name);
        if (app >= 0) hints.push({ kind: "installer_of_installed", node: c, other: app });
      }
      if (groupOf(name, apps) === "archives") {
        const base = name.replace(/(\.tar)?\.[a-z0-9]+$/i, "").toLowerCase();
        const unpacked = folderByName.get(base);
        if (unpacked !== undefined) hints.push({ kind: "unpacked", node: c, other: unpacked });
      }
    }
    const changed = tree.kind[c] === KIND.dir ? tree.newest[c] : tree.mtime[c];
    if (changed > 0 && now - changed >= YEAR) hints.push({ kind: "untouched", node: c, other: -1 });
  }
  return { root, groups: [...groups.values()].filter((g) => g.count > 0).sort((a, b) => b.bytes - a.bytes), hints };
}

// ---------- big and old files ----------

/**
 * The biggest files of the whole scan, and, separately, those not changed for a year (outside
 * Windows' own folders, where old files are normal and not yours to judge).
 */
export function bigFiles(tree: Tree, now: number, max = 100): { big: number[]; old: number[]; oldBytes: number; oldCount: number } {
  const system = new Uint8Array(tree.n);
  for (let i = 1; i < tree.n; i++) {
    const p = tree.parent[i];
    if (system[p]) system[i] = 1;
    else if (tree.kind[i] === KIND.dir && p > 0 && tree.parent[p] === 0 && SYSTEM_TOP.has(tree.names[i].toLowerCase())) system[i] = 1;
  }
  const big: number[] = [];
  const old: number[] = [];
  let oldBytes = 0;
  let oldCount = 0;
  for (let i = 1; i < tree.n; i++) {
    if (tree.kind[i] !== KIND.file) continue;
    keepTop(tree, big, i, max);
    if (!system[i] && tree.mtime[i] > 0 && now - tree.mtime[i] >= YEAR) {
      oldBytes += tree.agg[i];
      oldCount++;
      keepTop(tree, old, i, max);
    }
  }
  return { big, old, oldBytes, oldCount };
}

/** "3 days ago", "8 months ago", "2 years ago". */
export function ago(secs: number | undefined, now: number): string {
  if (!secs) return "not known";
  const d = Math.max(0, now - secs) / DAY;
  if (d < 1) return "today";
  if (d < 2) return "yesterday";
  if (d < 30) return `${Math.floor(d)} days ago`;
  if (d < 365) {
    const m = Math.floor(d / 30);
    return m <= 1 ? "a month ago" : `${m} months ago`;
  }
  const y = Math.floor(d / 365);
  return y <= 1 ? "a year ago" : `${y} years ago`;
}

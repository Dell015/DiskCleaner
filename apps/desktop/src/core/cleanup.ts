// Removing, on the page side: which group a picked entry is in (shown on the review screen), the
// picks themselves, and taking removed entries out of the picture. The desktop app decides the
// group again in Rust (dc_api::cleanup) and checks the disk before anything moves; this copy only
// tells the person in advance.

import { FLAG } from "./format";
import type { Tree } from "./tree";

export type Group = "safe" | "probably_safe" | "your_call" | "never";

export interface Eligibility {
  group: Group;
  /** Why it cannot be removed (never), or the finding's title. */
  why: string;
  /** The finding's reason id (suggested items). */
  reason?: string;
}

const NEVER_REASONS = new Set(["browser_profile_data", "wallet_or_vault"]);
// Same lists as dc_protected::not_your_own.
const SYSTEM_TOP = new Set(["windows", "windows.old", "program files", "program files (x86)", "programdata", "system volume information", "recovery", "boot", "efi", "perflogs", "config.msi", "msocache", "documents and settings", "intel", "amd", "nvidia", "drivers"]);
const NOT_A_PERSON = new Set(["default", "default user", "all users", "defaultapppool"]);
/** Folders of a profile that the protected list keeps (the removal engine refuses everything in them). */
const PROTECTED_USER = new Map([["documents", "your documents"], ["desktop", "your desktop"], ["pictures", "your pictures"], ["music", "your music"], ["videos", "your videos"]]);

/** The parts of a drive path below the drive, lower case (null if it is not one). */
function driveParts(path: string): string[] | null {
  const p = path.replace(/\//g, "\\").replace(/^\\\\\?\\/, "");
  if (!/^[a-z]:(\\|$)/i.test(p)) return null;
  return p.slice(2).split("\\").filter(Boolean).map((s) => s.toLowerCase().replace(/[. ]+$/, ""));
}

/** Why `path` is not one of the person's own places (same rule as dc_protected::not_your_own), or null. */
export function notYourOwn(path: string): string | null {
  const parts = driveParts(path);
  if (!parts) return "its path cannot be understood";
  if (parts.length === 0) return "it is a whole drive";
  const first = parts[0];
  if (SYSTEM_TOP.has(first) || first.startsWith("$")) return "it is in a Windows or program folder";
  if (first === "users") {
    if (parts.length === 1) return "it holds every user profile";
    if (parts.length === 2) return "it is a whole user profile";
    if (NOT_A_PERSON.has(parts[1])) return "it is in a profile Windows keeps for itself";
    const folder = parts[2];
    if (folder.startsWith(".") || folder === "appdata" || folder.startsWith("ntuser")) return "it holds settings of apps and Windows for this user";
    if (folder.startsWith("onedrive")) return "it is in OneDrive: removing it here would also delete it from OneDrive";
  }
  if (parts.includes("appdata")) return "it holds settings of apps";
  if (parts.some((p) => p.startsWith("onedrive"))) return "it is in OneDrive: removing it here would also delete it from OneDrive";
  return null;
}

/** The group of an entry (same rules as dc_api::cleanup::eligibility). */
export function eligibility(tree: Tree, i: number): Eligibility {
  const never = (why: string): Eligibility => ({ group: "never", why });
  if (i <= 0 || i >= tree.n) return never("it is not in the scan");
  if (tree.flags[i] & (FLAG.link | FLAG.cloud)) return never("it is a link or a cloud-only file");
  // Its chain from the scanned root down.
  const chain: number[] = [];
  for (let a = i; a > 0; a = tree.parent[a]) chain.push(a);
  chain.reverse();
  if (chain.length <= 1) return never("it is the whole scanned drive or folder");
  const parts = driveParts(tree.path(i)) ?? [];
  if (parts[0] === "users" && parts.length >= 3 && PROTECTED_USER.has(parts[2])) return never(`DiskCleaner protects it: ${PROTECTED_USER.get(parts[2])}`);
  const mk = tree.mark[i];
  if (mk >= 0) {
    const m = tree.marks[mk];
    if (m.source === "refused") return never(`DiskCleaner refuses it: ${m.detail}`);
    if (NEVER_REASONS.has(m.reason)) return never("it holds browser logins or keys; losing it can lose them for good");
    if (m.offered) {
      if (m.risk === "safe") return { group: "safe", why: m.title, reason: m.reason };
      if (m.risk === "probably_safe") return { group: "probably_safe", why: m.title, reason: m.reason };
      return never("its verdict is Don't touch");
    }
  }
  const o = tree.owner[i];
  if (o >= 0) {
    const owner = tree.owners[o];
    if (owner.kind === "windows") return never("it belongs to Windows");
    const p = tree.path(i).toLowerCase();
    const f = owner.folder?.toLowerCase().replace(/\\+$/, "");
    if ((f && (p === f || p.startsWith(`${f}\\`))) || /^[a-z]:\\program files( \(x86\))?\\/.test(p)) return never(`it is part of the installed app "${owner.name}": remove the app with its own uninstaller`);
    return never(`it belongs to "${owner.name}" (app data); removing it can break or reset that app`);
  }
  // The person's own places, judged on the full path (scanning C:\\Program Files itself changes nothing).
  const why = notYourOwn(tree.path(i));
  return why ? never(why) : { group: "your_call", why: "" };
}

/** A pick: the entry, and the person's "I checked this" for a Probably safe one. */
export interface Pick { node: number; checked: boolean }

/** Drop picks that are inside other picks (the outer one moves them anyway). */
export function outermost(tree: Tree, nodes: number[]): number[] {
  const set = new Set(nodes);
  return nodes.filter((n) => {
    for (let a = tree.parent[n]; a > 0; a = tree.parent[a]) if (set.has(a)) return false;
    return true;
  });
}

/** Take an entry out of the picture (it was removed): sizes above it shrink, and it is unlinked. */
export function detach(tree: Tree, i: number): void {
  if (i <= 0 || i >= tree.n) return;
  const p = tree.parent[i];
  if (p < 0) return;
  // Already detached?
  let found = false;
  for (let c = tree.firstChild[p]; c >= 0; c = tree.nextSibling[c]) if (c === i) { found = true; break; }
  if (!found) return;
  const bytes = tree.agg[i];
  const files = tree.files[i];
  for (let a = p; a >= 0; a = tree.parent[a]) {
    tree.agg[a] -= bytes;
    tree.files[a] -= files;
  }
  if (tree.firstChild[p] === i) tree.firstChild[p] = tree.nextSibling[i];
  else {
    for (let c = tree.firstChild[p]; c >= 0; c = tree.nextSibling[c]) {
      if (tree.nextSibling[c] === i) {
        tree.nextSibling[c] = tree.nextSibling[i];
        if (tree.lastChild[p] === i) tree.lastChild[p] = c;
        break;
      }
    }
  }
  if (tree.lastChild[p] === i) tree.lastChild[p] = -1;
  tree.nextSibling[i] = -1;
  tree.childCount[p]--;
  tree.version++;
}

/** Days until a cleanup made at `created` is emptied. */
export function daysLeft(created: number, retentionDays: number, now = Date.now() / 1000): number {
  return Math.max(0, Math.ceil((created + retentionDays * 86_400 - now) / 86_400));
}

/** Put a detached entry back (Undo): it joins its folder again and the sizes above it grow. */
export function attach(tree: Tree, i: number): void {
  if (i <= 0 || i >= tree.n) return;
  const p = tree.parent[i];
  if (p < 0) return;
  for (let c = tree.firstChild[p]; c >= 0; c = tree.nextSibling[c]) if (c === i) return;
  tree.nextSibling[i] = tree.firstChild[p];
  tree.firstChild[p] = i;
  if (tree.lastChild[p] < 0) tree.lastChild[p] = i;
  tree.childCount[p]++;
  for (let a = p; a >= 0; a = tree.parent[a]) {
    tree.agg[a] += tree.agg[i];
    tree.files[a] += tree.files[i];
  }
  tree.version++;
}

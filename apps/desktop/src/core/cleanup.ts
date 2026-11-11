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
const SYSTEM_TOP = new Set(["windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information", "recovery", "boot", "efi", "$windows.~bt", "$windows.~ws"]);

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
  const root = tree.names[chain[0]];
  const rootIsDrive = /^[a-z]:[\\/]?$/i.test(root);
  if (rootIsDrive && SYSTEM_TOP.has(tree.names[chain[1]].toLowerCase())) return never("it is in a Windows or program folder");
  const full = tree.path(i).toLowerCase().replace(/\//g, "\\");
  const at = full.indexOf("\\users\\");
  if (at >= 0) {
    const rest = full.slice(at + 7).split("\\");
    if (rest.length <= 1) return never("it is a whole user profile");
    const folder = rest[1];
    if (folder.startsWith(".") || folder === "appdata" || folder.startsWith("ntuser")) return never("it holds settings of apps and Windows for this user");
    if (folder.startsWith("onedrive")) return never("it is in OneDrive: removing it here would also delete it from OneDrive");
  }
  if (full.includes("\\appdata\\") || full.includes("\\onedrive")) return never("it holds settings or synced files of apps");
  return { group: "your_call", why: "" };
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

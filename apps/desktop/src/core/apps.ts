// The Apps view's data: every app with its size and places, and an app's parts sorted by what
// deleting them does. Read-only: this only describes the scan.

import { KIND } from "./format";
import type { Tree } from "./tree";

/** Windows parts and shared runtimes: real installs, but nothing a person "runs". */
const SYSTEM_PART = /\b(visual c\+\+|redistributable|\.net (framework|runtime|desktop runtime)|windows (sdk|driver|software development)|webview2|directx|vulkan|physx|runtime|driver|chipset|sdk|update for|hotfix|security update|microsoft edge update|vcredist|ms visual|universal crt)\b/i;

export interface AppCard {
  owner: number;
  /** Bytes in the scan. */
  bytes: number;
  /** Outermost folders it owns, biggest first. */
  places: number[];
  /** A Windows part or shared runtime (folded away by default). */
  system: boolean;
}

/** Is this app a Windows part or shared runtime? */
export function isSystemPart(name: string, publisher?: string): boolean {
  return SYSTEM_PART.test(name) || (/^microsoft/i.test(publisher ?? "") && /\b(update|component|support|helper|tools for)\b/i.test(name));
}

/** Every installed app: those with something in the scan (biggest first), then the rest. */
export function appCards(tree: Tree): AppCard[] {
  const per = new Map<number, AppCard>();
  tree.owners.forEach((o, i) => {
    if (o.kind === "app") per.set(i, { owner: i, bytes: 0, places: [], system: isSystemPart(o.name, o.publisher) });
  });
  for (let i = 0; i < tree.n; i++) {
    const o = tree.ownerHere[i];
    if (o < 0) continue;
    const above = tree.parent[i] >= 0 ? tree.owner[tree.parent[i]] : -1;
    if (above === o) continue;
    const card = per.get(o);
    if (!card) continue;
    card.bytes += tree.agg[i];
    card.places.push(i);
  }
  const list = [...per.values()];
  for (const c of list) c.places.sort((a, b) => tree.agg[b] - tree.agg[a]);
  return list.sort((a, b) => b.bytes - a.bytes || tree.owners[a.owner].name.localeCompare(tree.owners[b.owner].name));
}

export type PlaceRole = "program" | "data" | "saves" | "temp";

export const ROLE_TEXT: Record<PlaceRole, { title: string; why: string }> = {
  program: { title: "Needed to run", why: "The app's own program files. Remove the app with its uninstaller instead." },
  data: { title: "Settings and data", why: "Its settings, logins and data. Removing them can reset or break the app." },
  saves: { title: "Your saves", why: "Your game saves and settings. Kept unless you remove them yourself." },
  temp: { title: "Temporary files", why: "Temporary files it left; the parts DiskCleaner checked are offered below." },
};

/** What kind of place an app folder is. */
export function placeRole(tree: Tree, place: number, installFolder?: string): PlaceRole {
  const p = tree.path(place).toLowerCase().replace(/\//g, "\\");
  const f = installFolder?.toLowerCase().replace(/\\+$/, "");
  if (f && (p === f || p.startsWith(`${f}\\`))) return "program";
  if (/\\appdata\\local\\temp(\\|$)/.test(p)) return "temp";
  if (/\\(saved games|documents\\my games)\\/.test(p)) return "saves";
  if (/\\appdata\\|^[a-z]:\\programdata\\|\\documents\\/.test(p)) return "data";
  return "program";
}

export interface Clearable {
  node: number;
  bytes: number;
  /** The finding's title and verdict. */
  title: string;
  risk: "safe" | "probably_safe";
}

/**
 * What DiskCleaner's checks offer inside an app's places: the outermost entries marked Safe or
 * Probably safe (and not overruled by a more careful mark above them), biggest first.
 */
export function clearables(tree: Tree, places: number[]): Clearable[] {
  const out: Clearable[] = [];
  const stack = [...places];
  while (stack.length) {
    const i = stack.pop() as number;
    const mh = tree.markHere[i];
    if (mh >= 0 && tree.mark[i] === mh) {
      const m = tree.marks[mh];
      if (m.offered && (m.risk === "safe" || m.risk === "probably_safe")) {
        out.push({ node: i, bytes: tree.agg[i], title: m.title, risk: m.risk });
        continue; // its inside goes with it
      }
    }
    if (tree.kind[i] === KIND.dir) for (let c = tree.firstChild[i]; c >= 0; c = tree.nextSibling[c]) stack.push(c);
  }
  return out.sort((a, b) => b.bytes - a.bytes);
}

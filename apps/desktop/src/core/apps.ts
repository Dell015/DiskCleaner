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

/** One circle of an app's network drawing. */
export interface NetNode {
  /** The scan entry (-1 for the app itself, -2 for "more places"). */
  node: number;
  x: number;
  y: number;
  r: number;
  bytes: number;
  kind: "app" | PlaceRole | "more" | "safe" | "probably_safe";
  /** The circle it hangs from (index into the list; -1 for the app). */
  from: number;
  label: string;
}

/** The drawing's size; the page scales it. */
export const NET_W = 900;
export const NET_H = 440;
/** At most this many places get their own circle; the rest are one "more" circle. */
const NET_PLACES = 12;
/** At most this many clearable parts per place are drawn. */
const NET_CLEAR = 4;

/**
 * Lay out an app's network: the app in the middle, its places around it (bigger circle, more
 * space), and what can be cleared inside each place hanging further out. Circle areas follow
 * the bytes. Pure: no drawing here.
 */
export function layoutNetwork(tree: Tree, places: number[], roles: Map<number, PlaceRole>, clear: Clearable[], appName: string): NetNode[] {
  const cx = NET_W / 2;
  const cy = NET_H / 2;
  const total = places.reduce((a, p) => a + tree.agg[p], 0);
  const out: NetNode[] = [{ node: -1, x: cx, y: cy, r: 30, bytes: total, kind: "app", from: -1, label: appName }];
  const shown = places.slice(0, NET_PLACES);
  const rest = places.slice(NET_PLACES);
  const max = Math.max(1, ...shown.map((p) => tree.agg[p]), rest.reduce((a, p) => a + tree.agg[p], 0));
  const radius = (bytes: number, big: number): number => Math.max(5, Math.sqrt(bytes / max) * big);
  const ring: { node: number; bytes: number; kind: NetNode["kind"]; label: string }[] = shown.map((p) => ({ node: p, bytes: tree.agg[p], kind: roles.get(p) ?? "program", label: tree.names[p] }));
  if (rest.length) ring.push({ node: -2, bytes: rest.reduce((a, p) => a + tree.agg[p], 0), kind: "more", label: `${rest.length} more places` });
  const n = ring.length;
  // An ellipse fits the wide drawing; start at the top and go round.
  const rx = NET_W * 0.3;
  const ry = NET_H * 0.3;
  ring.forEach((p, k) => {
    // One or two places sit left and right of the app; more go round from the top.
    const a = n <= 2 ? Math.PI * (k === 0 ? 1 : 0) : -Math.PI / 2 + (2 * Math.PI * k) / n;
    const at = out.length;
    out.push({ node: p.node, x: cx + Math.cos(a) * rx, y: cy + Math.sin(a) * ry, r: radius(p.bytes, 34), bytes: p.bytes, kind: p.kind, from: 0, label: p.label });
    // What can be cleared inside this place, fanned out beyond it.
    const inside = p.node >= 0 ? clear.filter((c) => c.node === p.node || tree.isAncestor(p.node, c.node)).slice(0, NET_CLEAR) : [];
    inside.forEach((c, j) => {
      const spread = inside.length === 1 ? 0 : (j / (inside.length - 1) - 0.5) * 1.0;
      const b = a + spread;
      out.push({ node: c.node, x: cx + Math.cos(b) * rx * 1.42, y: cy + Math.sin(b) * ry * 1.42, r: radius(c.bytes, 22), bytes: c.bytes, kind: c.risk, from: at, label: c.title });
    });
  });
  // Keep every circle and its two label lines inside the drawing (outer circles in the top half
  // carry their labels above, the rest below).
  for (const d of out) {
    const labelAbove = d.from > 0 && d.y < cy;
    d.x = Math.min(NET_W - d.r - 2, Math.max(d.r + 2, d.x));
    d.y = Math.min(NET_H - d.r - (labelAbove ? 2 : 30), Math.max(d.r + (labelAbove ? 30 : 2), d.y));
  }
  return out;
}

// A live scan in the page: entries arrive in batches (a folder always before what is inside it)
// and go straight into the tree; at the end the checks arrive by path and are attached.
// The same code serves the desktop app (events from the Rust scanner) and a simulated scan of the
// demo disk in the browser (used by the browser checks).

import { FLAG, KIND, type Mark, type Owner, type RiskId, type ViewData } from "./core/format";
import { Tree, markRank } from "./core/tree";

export interface LiveEntry { name: string; kind: number; size: number; mtime: number; flags: number; cat: number }
export interface LiveDir { dir: string; entries: LiveEntry[] }
export interface PathMark extends Omit<Mark, "node"> { path: string; node?: number }
export interface LiveSummary {
  status: "complete" | "partial" | "cancelled";
  errors: number;
  files: number;
  dirs: number;
  bytes_on_disk: number;
  marks: PathMark[];
  owners: Owner[];
  assignments: { path: string; owner: number; confidence: number }[];
  uncounted: string[];
  apps_known: boolean;
}
export type LiveEvent =
  | { type: "started"; schema_version: number; root: string; drive_root?: boolean; volume_total?: number | null; volume_used?: number | null }
  | { type: "batch"; dirs: LiveDir[] }
  | { type: "progress"; files: number; dirs: number; bytes: number; current: string }
  | { type: "analysing" }
  | { type: "finished"; status: LiveSummary["status"] } & LiveSummary
  | { type: "failed"; message: string };

export const LIVE_SCHEMA_VERSION = 1;
const CATEGORIES = ["video", "image", "audio", "archive", "code", "document", "game_asset", "installer", "executable", "temporary", "other"];

/** The tree of a live scan, and where each folder is. */
export class LiveTree {
  readonly tree: Tree;
  private dirs = new Map<string, number>();
  private sep: string;
  readonly root: string;

  constructor(root: string) {
    this.root = root.replace(/[\\/]+$/, "") || root;
    this.sep = root.includes("\\") ? "\\" : "/";
    this.tree = new Tree(1 << 16, [], [], CATEGORIES);
    this.tree.append(-1, "This PC", KIND.dir, 0, 0, 0, -1);
    const r = this.tree.append(0, root, KIND.dir, 0, 0, 0, -1);
    this.dirs.set(this.key(this.root), r);
  }

  private key(path: string): string {
    return path.replace(/[\\/]+$/, "").toLowerCase();
  }

  /** Add a batch. Folders that are not known yet are skipped (cannot happen with a correct scanner). */
  add(dirs: LiveDir[]): number {
    let added = 0;
    for (const d of dirs) {
      const parent = this.dirs.get(this.key(d.dir));
      if (parent === undefined) continue;
      // "C:\" + "Windows" must give "C:\Windows", not "C:\\Windows".
      const base = d.dir.replace(/[\\/]+$/, "");
      for (const e of d.entries) {
        const i = this.tree.append(parent, e.name, e.kind, e.kind === KIND.file ? e.size : 0, e.mtime, e.flags, e.cat);
        added++;
        if (e.kind === KIND.dir) this.dirs.set(this.key(`${base}${this.sep}${e.name}`), i);
      }
    }
    return added;
  }

  /** Find an entry by full path (folders directly, files through their folder). */
  find(path: string): number {
    const k = this.key(path);
    const dir = this.dirs.get(k);
    if (dir !== undefined) return dir;
    const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
    if (cut <= 0) return -1;
    const parent = this.dirs.get(this.key(path.slice(0, cut)));
    return parent === undefined ? -1 : this.tree.childNamed(parent, path.slice(cut + 1));
  }

  /** Attach what the checks found. Returns how many marks found their entry. */
  finish(s: LiveSummary): number {
    for (const p of s.uncounted) {
      const i = this.find(p);
      if (i >= 0) this.tree.uncount(i, FLAG.notCounted);
    }
    const marks: Mark[] = [];
    const markOf = new Map<number, number>();
    for (const m of s.marks) {
      const i = this.find(m.path);
      if (i < 0) continue;
      // Fail closed: "don't touch" or refused can never be offered, whatever arrives.
      const offered = m.offered && m.risk !== "dont_touch" && m.source !== "refused";
      const mark: Mark = { node: i, risk: m.risk as RiskId, source: m.source, title: m.title, reason: m.reason, detail: m.detail, evidence: m.evidence ?? [], offered, bytes: m.bytes };
      if (m.confidence) mark.confidence = m.confidence;
      const old = markOf.get(i);
      if (old === undefined || markRank(mark) > markRank(marks[old])) markOf.set(i, marks.length);
      marks.push(mark);
    }
    const ownerOf = new Map<number, number>();
    for (const a of s.assignments) {
      if (a.confidence < 1 || a.owner < 0 || a.owner >= s.owners.length) continue;
      const i = this.find(a.path);
      if (i >= 0) ownerOf.set(i, a.owner);
    }
    this.tree.setAnnotations(s.owners, marks, ownerOf, markOf);
    return markOf.size;
  }
}

/**
 * A pretend live scan of a loaded view (the demo): folders are sent outward from the root in
 * batches, the way the scanner sends them, then a summary with the marks by path.
 */
export function simulateLive(data: ViewData, onEvent: (e: LiveEvent) => void, entriesPerSecond = 60_000): { cancel(): void } {
  const n = data.names.length;
  const kids: number[][] = Array.from({ length: n }, () => []);
  for (let i = 1; i < n; i++) kids[data.parent[i]].push(i);
  const root = 1;
  const pathOf: string[] = new Array(n);
  // Like the scanner: the root keeps its trailing separator ("C:\\"), folders below do not.
  pathOf[root] = data.names[root];
  const order: number[] = [root];
  for (let k = 0; k < order.length; k++) {
    const d = order[k];
    for (const c of kids[d]) {
      pathOf[c] = `${pathOf[d].replace(/[\\/]+$/, "")}\\${data.names[c]}`;
      if (data.kind[c] === KIND.dir) order.push(c);
    }
  }
  let cancelled = false;
  let k = 0;
  onEvent({ type: "started", schema_version: LIVE_SCHEMA_VERSION, root: data.names[root], drive_root: true, volume_total: null, volume_used: data.size.reduce((a, b) => a + b, 0) });
  let last = performance.now();
  const tick = (): void => {
    if (cancelled) return finish("cancelled");
    const now = performance.now();
    let budget = Math.max(1, Math.round(((now - last) / 1000) * entriesPerSecond));
    last = now;
    const dirs: LiveDir[] = [];
    while (k < order.length && budget > 0) {
      const d = order[k++];
      const entries = kids[d].map((c) => ({ name: data.names[c], kind: data.kind[c], size: data.size[c], mtime: data.mtime[c], flags: data.flags[c], cat: data.cat[c] }));
      budget -= entries.length + 1;
      dirs.push({ dir: pathOf[d], entries });
    }
    if (dirs.length) onEvent({ type: "batch", dirs });
    onEvent({ type: "progress", files: 0, dirs: k, bytes: 0, current: pathOf[order[Math.min(k, order.length - 1)]] });
    if (k >= order.length) {
      onEvent({ type: "analysing" });
      setTimeout(() => finish("complete"), 300);
    } else setTimeout(tick, 60);
  };
  const finish = (status: LiveSummary["status"]): void => {
    const done = status === "complete";
    onEvent({
      type: "finished", status, errors: 0, files: 0, dirs: order.length, bytes_on_disk: 0, apps_known: done,
      marks: done ? data.marks.map((m) => ({ ...m, path: pathOf[m.node] ?? "" })) : [],
      owners: done ? data.owners : [],
      assignments: [],
      uncounted: [],
    });
  };
  setTimeout(tick, 60);
  return { cancel: () => (cancelled = true) };
}

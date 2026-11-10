// Duplicate files: the events the desktop app sends, the state the Overview shows, and a
// simulation for the browser (demo and checks), which treats files of the same size and type as
// copies. The real search (Rust) compares the files' contents.

import { KIND } from "./format";
import type { Tree } from "./tree";
import { ext } from "./inventory";

export type DupEvent =
  | { type: "started"; files: number; bytes: number }
  | { type: "progress"; done: number; current: string }
  | { type: "group"; size: number; paths: string[] }
  | { type: "finished"; status: "complete" | "cancelled"; groups: number; wasted: number; unreadable: number };

export interface DupGroup { size: number; paths: string[]; nodes: number[] }

export interface DupState {
  status: "idle" | "running" | "done" | "cancelled" | "failed";
  /** Folder searched (-1: everything). */
  within: number;
  files: number;
  total: number;
  done: number;
  current: string;
  groups: DupGroup[];
  wasted: number;
  unreadable: number;
  message: string;
}

export function emptyDups(): DupState {
  return { status: "idle", within: -1, files: 0, total: 0, done: 0, current: "", groups: [], wasted: 0, unreadable: 0, message: "" };
}

/** Space the extra copies take. */
export function extra(g: DupGroup): number {
  return g.size * Math.max(0, g.paths.length - 1);
}

/** Apply one event. `find` turns a path into an entry of the tree (-1 if not there). */
export function applyDupEvent(s: DupState, e: DupEvent, find: (path: string) => number): void {
  switch (e.type) {
    case "started":
      s.files = e.files;
      s.total = e.bytes;
      break;
    case "progress":
      s.done = e.done;
      s.current = e.current;
      break;
    case "group":
      s.groups.push({ size: e.size, paths: e.paths, nodes: e.paths.map(find) });
      s.groups.sort((a, b) => extra(b) - extra(a));
      break;
    case "finished":
      s.status = e.status === "complete" ? "done" : "cancelled";
      s.wasted = e.wasted;
      s.unreadable = e.unreadable;
      s.done = s.total;
      break;
  }
}

/** A lookup from full path (any case) to entry, for the files that can be duplicates. */
export function pathFinder(tree: Tree, minSize = 1024 * 1024): (path: string) => number {
  let map: Map<string, number> | null = null;
  return (path: string): number => {
    if (!map) {
      map = new Map();
      for (let i = 1; i < tree.n; i++) if (tree.kind[i] === KIND.file && tree.agg[i] >= minSize) map.set(tree.path(i).toLowerCase(), i);
    }
    return map.get(path.toLowerCase()) ?? -1;
  };
}

/** Browser stand-in for the real search: same size and type count as copies. */
export function simulateDups(tree: Tree, within: number, onEvent: (e: DupEvent) => void, minSize = 1024 * 1024): { cancel(): void } {
  const bySize = new Map<string, number[]>();
  const inside = (i: number): boolean => within < 0 || tree.isAncestor(within, i);
  for (let i = 1; i < tree.n; i++) {
    if (tree.kind[i] !== KIND.file || tree.agg[i] < minSize || !inside(i)) continue;
    const k = `${tree.agg[i]}:${ext(tree.names[i])}`;
    const list = bySize.get(k);
    if (list) list.push(i);
    else bySize.set(k, [i]);
  }
  const sets = [...bySize.values()].filter((l) => l.length > 1).sort((a, b) => tree.agg[b[0]] - tree.agg[a[0]]);
  const files = sets.reduce((a, l) => a + l.length, 0);
  const bytes = sets.reduce((a, l) => a + l.length * tree.agg[l[0]], 0);
  let cancelled = false;
  let k = 0;
  let done = 0;
  let wasted = 0;
  onEvent({ type: "started", files, bytes });
  const tick = (): void => {
    if (cancelled) {
      onEvent({ type: "finished", status: "cancelled", groups: k, wasted, unreadable: 0 });
      return;
    }
    if (k >= sets.length) {
      onEvent({ type: "progress", done: bytes, current: "" });
      onEvent({ type: "finished", status: "complete", groups: sets.length, wasted, unreadable: 0 });
      return;
    }
    const set = sets[k++];
    done += set.length * tree.agg[set[0]];
    wasted += (set.length - 1) * tree.agg[set[0]];
    onEvent({ type: "progress", done, current: tree.path(set[0]) });
    onEvent({ type: "group", size: tree.agg[set[0]], paths: set.map((i) => tree.path(i)).sort() });
    setTimeout(tick, 120);
  };
  setTimeout(tick, 120);
  return { cancel: () => (cancelled = true) };
}

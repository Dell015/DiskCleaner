// Level of detail: which entries get a node in the graph. A real disk has more than a million
// entries; the graph shows a few thousand. The biggest folders open first, folders the user or the
// zoom asked for open before anything else, and small children are summed into one "more" node so
// the sizes on screen always add up.

import { KIND } from "./format";
import type { Tree } from "./tree";

export interface LodOptions {
  /** Automatic opening stops at this many nodes. */
  budget: number;
  /** Nothing opens past this many nodes, not even on request. */
  hardCap: number;
  /** Children shown for a folder opened automatically. */
  maxChildren: number;
  /** Children shown for a folder the user or the zoom opened. */
  maxChildrenOpened: number;
  /** Children shown for a folder whose "more" node was opened. */
  maxChildrenWide: number;
  /** Automatically shown children are at least this share of their folder. */
  minShare: number;
  /**
   * ...and at least (this / budget) of the whole picture, so the overview shows structure, not
   * fans of small files. Folders the user or the zoom opened ignore both limits.
   */
  globalShare: number;
}

export const DEFAULT_LOD: LodOptions = {
  budget: 1500,
  hardCap: 6000,
  maxChildren: 24,
  maxChildrenOpened: 40,
  maxChildrenWide: 250,
  minShare: 0.01,
  globalShare: 0.15,
};

export interface LodRequest {
  /** Folders opened by the user or by zooming in. */
  opened: ReadonlySet<number>;
  /** Folders the user closed. */
  closed: ReadonlySet<number>;
  /** Folders whose "more" node was opened: show many more children. */
  wide: ReadonlySet<number>;
  /** Entries that must be visible (search result, selection). */
  mustShow: ReadonlySet<number>;
  /** Where the picture starts (default 0; the single scan root when there is only one). */
  root?: number;
}

export interface GroupInfo { parent: number; count: number; bytes: number }

export interface VisItem {
  key: string;
  /** Tree index, or -1 for a "more items" group. */
  node: number;
  parentKey: string | null;
  open: boolean;
  group?: GroupInfo;
}

export interface Visible {
  items: VisItem[];
  byKey: Map<string, VisItem>;
  /** Folders whose children are on screen. */
  open: Set<number>;
  /** True when something asked for could not open because of the limits. */
  truncated: boolean;
}

export const nodeKey = (i: number): string => `n${i}`;
export const groupKey = (parent: number): string => `g${parent}`;

/** Max-heap of [priority, tiebreak, node]. */
class Heap {
  private a: [number, number, number][] = [];
  get size(): number {
    return this.a.length;
  }
  private less(x: [number, number, number], y: [number, number, number]): boolean {
    return x[0] !== y[0] ? x[0] < y[0] : x[1] !== y[1] ? x[1] < y[1] : x[2] > y[2];
  }
  push(v: [number, number, number]): void {
    const a = this.a;
    a.push(v);
    let i = a.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (!this.less(a[p], a[i])) break;
      [a[p], a[i]] = [a[i], a[p]];
      i = p;
    }
  }
  pop(): [number, number, number] | undefined {
    const a = this.a;
    if (a.length === 0) return undefined;
    const top = a[0];
    const last = a.pop()!;
    if (a.length > 0) {
      a[0] = last;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1;
        const r = l + 1;
        let m = i;
        if (l < a.length && this.less(a[m], a[l])) m = l;
        if (r < a.length && this.less(a[m], a[r])) m = r;
        if (m === i) break;
        [a[m], a[i]] = [a[i], a[m]];
        i = m;
      }
    }
    return top;
  }
}

/** Pick the visible entries. Pure: same tree, options and request give the same answer. */
export function selectVisible(tree: Tree, opt: LodOptions, req: LodRequest): Visible {
  const items: VisItem[] = [];
  const byKey = new Map<string, VisItem>();
  const open = new Set<number>();
  let truncated = false;
  if (tree.n === 0) return { items, byKey, open, truncated };

  const root = req.root !== undefined && req.root >= 0 && req.root < tree.n ? req.root : 0;
  // Everything above a must-show entry has to open (only inside the picture's root).
  const need = new Set<number>();
  for (const m of req.mustShow) {
    if (m < 0 || m >= tree.n || (m !== root && !tree.isAncestor(root, m))) continue;
    for (let p = tree.parent[m]; p >= 0 && p !== tree.parent[root]; p = tree.parent[p]) need.add(p);
  }
  const forced = (i: number): boolean => i === root || need.has(i) || req.opened.has(i);

  const add = (item: VisItem): void => {
    items.push(item);
    byKey.set(item.key, item);
  };
  add({ key: nodeKey(root), node: root, parentKey: null, open: false });

  const heap = new Heap();
  const push = (i: number): void => {
    if (tree.kind[i] !== KIND.dir || tree.childCount[i] === 0) return;
    heap.push([forced(i) ? 1 : 0, tree.agg[i], i]);
  };
  push(root);

  for (let next = heap.pop(); next; next = heap.pop()) {
    const p = next[2];
    const isForced = forced(p);
    if (req.closed.has(p) && !need.has(p) && p !== root) continue;
    const k = req.wide.has(p) ? opt.maxChildrenWide : isForced ? opt.maxChildrenOpened : opt.maxChildren;
    const { top } = tree.topChildren(p, k);
    const minBytes = Math.max(opt.minShare * tree.agg[p], (opt.globalShare / Math.max(1, opt.budget)) * tree.agg[root]);
    let shown = isForced || req.wide.has(p) ? top : top.filter((c) => tree.agg[c] > 0 && tree.agg[c] >= minBytes);
    // Must-show children (and the way to them) are added even when small.
    const extra: number[] = [];
    if (need.size > 0 || req.mustShow.size > 0) {
      const inShown = new Set(shown);
      for (const c of tree.children(p)) {
        if ((need.has(c) || req.mustShow.has(c)) && !inShown.has(c)) extra.push(c);
      }
    }
    if (shown.length === 0 && extra.length === 0) continue;
    const limit = isForced ? opt.hardCap : opt.budget;
    const restOf = (list: number[]): { count: number; bytes: number } => {
      let bytes = tree.agg[p] - tree.own[p];
      for (const c of list) bytes -= tree.agg[c];
      return { count: tree.childCount[p] - list.length, bytes: Math.max(0, bytes) };
    };
    let all = shown.concat(extra);
    let rest = restOf(all);
    let cost = all.length + (rest.count > 0 ? 1 : 0);
    if (items.length + cost > limit) {
      // Running out of the automatic budget is normal; only a request that cannot be met is reported.
      if (!isForced) continue;
      truncated = true;
      // Fit what we can, keeping the must-show children.
      const room = opt.hardCap - items.length - extra.length - 1;
      if (room < 0 || (room === 0 && extra.length === 0)) continue;
      shown = shown.slice(0, room);
      all = shown.concat(extra);
      rest = restOf(all);
      cost = all.length + (rest.count > 0 ? 1 : 0);
      if (items.length + cost > opt.hardCap) continue;
    }
    const pk = nodeKey(p);
    const parentItem = byKey.get(pk);
    if (!parentItem) continue;
    parentItem.open = true;
    open.add(p);
    for (const c of all) {
      add({ key: nodeKey(c), node: c, parentKey: pk, open: false });
      push(c);
    }
    if (rest.count > 0) {
      add({ key: groupKey(p), node: -1, parentKey: pk, open: false, group: { parent: p, count: rest.count, bytes: rest.bytes } });
    }
  }
  return { items, byKey, open, truncated };
}

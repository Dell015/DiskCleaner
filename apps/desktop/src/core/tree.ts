// The scanned tree in typed arrays. It can grow one entry at a time (a live scan, or a replay),
// and keeps folder totals up to date as entries arrive.

import { KIND, type Mark, type Owner, type RiskId } from "./format";

export const CATS = 11;

/** Higher is riskier. */
export function markRank(m: Mark): number {
  return m.risk === "dont_touch" ? 3 : m.risk === "probably_safe" ? 2 : 1;
} // number of file categories tracked per folder (dc-core Category::ALL)

function grow<T extends Int32Array | Uint8Array | Float64Array | Uint32Array | Int8Array>(a: T, cap: number): T {
  const b = new (a.constructor as new (n: number) => T)(cap);
  b.set(a);
  return b;
}

export class Tree {
  n = 0;
  private cap: number;
  names: string[] = [];
  parent: Int32Array;
  kind: Uint8Array;
  /** Own size on disk (files). */
  own: Float64Array;
  /** Size of the entry and everything inside it. */
  agg: Float64Array;
  /** Number of files inside (a file counts itself). */
  files: Float64Array;
  /** Newest change inside (seconds). */
  newest: Float64Array;
  mtime: Float64Array;
  flags: Uint32Array;
  cat: Int8Array;
  firstChild: Int32Array;
  lastChild: Int32Array;
  nextSibling: Int32Array;
  childCount: Int32Array;
  depth: Int32Array;
  /** Owner index set directly on this entry, or -1. */
  ownerHere: Int32Array;
  /** Nearest owner of this entry or an ancestor, or -1. */
  owner: Int32Array;
  /** Mark index set directly on this entry, or -1. */
  markHere: Int32Array;
  /** The riskiest mark on this entry or an ancestor (nearest on ties), or -1. */
  mark: Int32Array;
  /** Bytes per file category, per folder (folders only; slot from dirSlot). */
  private dirSlot: Int32Array;
  private catBytes: Float64Array;
  private dirs = 0;
  /** Bumped on every change, so views know when to refresh. */
  version = 0;

  constructor(
    capacity: number,
    public owners: Owner[] = [],
    public marks: Mark[] = [],
    readonly categories: string[] = [],
    private ownerOf: Map<number, number> = new Map(),
    private markOf: Map<number, number> = new Map(),
  ) {
    const c = Math.max(16, capacity);
    this.cap = c;
    this.parent = new Int32Array(c);
    this.kind = new Uint8Array(c);
    this.own = new Float64Array(c);
    this.agg = new Float64Array(c);
    this.files = new Float64Array(c);
    this.newest = new Float64Array(c);
    this.mtime = new Float64Array(c);
    this.flags = new Uint32Array(c);
    this.cat = new Int8Array(c);
    this.firstChild = new Int32Array(c);
    this.lastChild = new Int32Array(c);
    this.nextSibling = new Int32Array(c);
    this.childCount = new Int32Array(c);
    this.depth = new Int32Array(c);
    this.ownerHere = new Int32Array(c);
    this.owner = new Int32Array(c);
    this.markHere = new Int32Array(c);
    this.mark = new Int32Array(c);
    this.dirSlot = new Int32Array(c);
    this.catBytes = new Float64Array(Math.max(16, c >> 3) * CATS);
  }

  private ensure(): void {
    if (this.n < this.cap) return;
    const c = this.cap * 2;
    this.cap = c;
    this.parent = grow(this.parent, c);
    this.kind = grow(this.kind, c);
    this.own = grow(this.own, c);
    this.agg = grow(this.agg, c);
    this.files = grow(this.files, c);
    this.newest = grow(this.newest, c);
    this.mtime = grow(this.mtime, c);
    this.flags = grow(this.flags, c);
    this.cat = grow(this.cat, c);
    this.firstChild = grow(this.firstChild, c);
    this.lastChild = grow(this.lastChild, c);
    this.nextSibling = grow(this.nextSibling, c);
    this.childCount = grow(this.childCount, c);
    this.depth = grow(this.depth, c);
    this.ownerHere = grow(this.ownerHere, c);
    this.owner = grow(this.owner, c);
    this.markHere = grow(this.markHere, c);
    this.mark = grow(this.mark, c);
    this.dirSlot = grow(this.dirSlot, c);
  }

  /**
   * Add one entry. Entries arrive parent first (the parent index is always smaller).
   * Returns the new index.
   */
  append(parent: number, name: string, kind: number, size: number, mtime: number, flags: number, cat: number): number {
    this.ensure();
    const i = this.n++;
    if (i === 0) {
      if (parent !== -1) throw new Error("the first entry must be the root");
    } else if (parent < 0 || parent >= i || this.kind[parent] !== KIND.dir) {
      throw new Error(`entry ${i} has a bad parent ${parent}`);
    }
    this.names[i] = name;
    this.parent[i] = parent;
    this.kind[i] = kind;
    this.own[i] = size;
    this.agg[i] = size;
    this.files[i] = kind === KIND.file ? 1 : 0;
    this.mtime[i] = mtime;
    this.newest[i] = mtime;
    this.flags[i] = flags;
    this.cat[i] = cat;
    this.firstChild[i] = -1;
    this.lastChild[i] = -1;
    this.nextSibling[i] = -1;
    this.childCount[i] = 0;
    const oh = this.ownerOf.get(i) ?? -1;
    const mh = this.markOf.get(i) ?? -1;
    this.ownerHere[i] = oh;
    this.markHere[i] = mh;
    if (kind === KIND.dir) {
      if ((this.dirs + 1) * CATS > this.catBytes.length) this.catBytes = grow(this.catBytes, this.catBytes.length * 2);
      this.dirSlot[i] = this.dirs++;
    } else {
      this.dirSlot[i] = -1;
    }
    if (parent >= 0) {
      this.depth[i] = this.depth[parent] + 1;
      this.owner[i] = oh >= 0 ? oh : this.owner[parent];
      // Risk only goes up: inside something riskier, the riskier mark still applies.
      const up = this.mark[parent];
      this.mark[i] = mh >= 0 && (up < 0 || markRank(this.marks[mh]) >= markRank(this.marks[up])) ? mh : up;
      if (this.lastChild[parent] < 0) this.firstChild[parent] = i;
      else this.nextSibling[this.lastChild[parent]] = i;
      this.lastChild[parent] = i;
      this.childCount[parent]++;
      const isFile = kind === KIND.file ? 1 : 0;
      const c = cat >= 0 && cat < CATS && isFile ? cat : -1;
      for (let p = parent; p >= 0; p = this.parent[p]) {
        this.agg[p] += size;
        this.files[p] += isFile;
        if (mtime > this.newest[p]) this.newest[p] = mtime;
        if (c >= 0 && size > 0) this.catBytes[this.dirSlot[p] * CATS + c] += size;
      }
    } else {
      this.depth[i] = 0;
      this.owner[i] = oh;
      this.mark[i] = mh;
    }
    this.version++;
    return i;
  }

  /** Children of a folder, in arrival order. */
  *children(i: number): Generator<number> {
    for (let c = this.firstChild[i]; c >= 0; c = this.nextSibling[c]) yield c;
  }

  /** The `k` biggest children (biggest first) and what the rest add up to. */
  topChildren(i: number, k: number): { top: number[]; restCount: number; restBytes: number } {
    const top: number[] = [];
    // A small sorted list is fastest for the small `k` used here.
    for (let c = this.firstChild[i]; c >= 0; c = this.nextSibling[c]) {
      if (top.length < k) {
        let j = top.length;
        top.push(c);
        while (j > 0 && this.bigger(c, top[j - 1])) {
          top[j] = top[j - 1];
          j--;
        }
        top[j] = c;
      } else if (k > 0 && this.bigger(c, top[k - 1])) {
        let j = k - 1;
        while (j > 0 && this.bigger(c, top[j - 1])) {
          top[j] = top[j - 1];
          j--;
        }
        top[j] = c;
      }
    }
    let shown = 0;
    for (const t of top) shown += this.agg[t];
    return {
      top,
      restCount: this.childCount[i] - top.length,
      restBytes: Math.max(0, this.agg[i] - this.own[i] - shown),
    };
  }

  /** Order: bigger first, then folders before files, then earlier first (stable and total). */
  private bigger(a: number, b: number): boolean {
    if (this.agg[a] !== this.agg[b]) return this.agg[a] > this.agg[b];
    if (this.kind[a] !== this.kind[b]) return this.kind[a] === KIND.dir;
    return a < b;
  }

  /**
   * Names and parents of every entry that will arrive (set while a file is replayed), so paths of
   * entries not yet added can be shown.
   */
  source: { names: string[]; parent: Int32Array } | null = null;

  /** Full path (Windows style). The virtual root is not part of it. */
  path(i: number): string {
    const names = this.source?.names ?? this.names;
    const parent = this.source?.parent ?? this.parent;
    if (i < 0 || i >= names.length) return "";
    const parts: string[] = [];
    for (let p = i; p > 0; p = parent[p]) {
      parts.push(names[p]);
      if (parent[p] === 0) break;
    }
    if (i === 0) return names[0];
    parts.reverse();
    const head = parts[0].replace(/[\\/]+$/, "");
    return [head, ...parts.slice(1)].join("\\");
  }

  /** Ancestors from the root down to the parent. */
  ancestors(i: number): number[] {
    const out: number[] = [];
    for (let p = this.parent[i]; p >= 0; p = this.parent[p]) out.push(p);
    return out.reverse();
  }

  isAncestor(a: number, b: number): boolean {
    for (let p = this.parent[b]; p >= 0; p = this.parent[p]) if (p === a) return true;
    return false;
  }

  /** The category holding the most bytes inside a folder (or a file's own), or -1. */
  dominantCategory(i: number): number {
    if (this.kind[i] !== KIND.dir) return this.cat[i];
    const s = this.dirSlot[i] * CATS;
    let best = -1;
    let bestBytes = 0;
    for (let c = 0; c < CATS; c++) {
      const v = this.catBytes[s + c];
      if (v > bestBytes) {
        bestBytes = v;
        best = c;
      }
    }
    return best;
  }

  /** Bytes per category inside a folder. */
  categoryBytes(i: number): number[] {
    if (this.kind[i] !== KIND.dir) return [];
    const s = this.dirSlot[i] * CATS;
    return Array.from(this.catBytes.subarray(s, s + CATS));
  }

  /**
   * Set owners and marks after the entries are in (a live scan learns them at the end).
   * Inheritance (owner of the nearest folder, riskiest mark above) is worked out again.
   */
  setAnnotations(owners: Owner[], marks: Mark[], ownerOf: Map<number, number>, markOf: Map<number, number>): void {
    this.owners = owners;
    this.marks = marks;
    this.ownerOf = ownerOf;
    this.markOf = markOf;
    for (let i = 0; i < this.n; i++) {
      const oh = ownerOf.get(i) ?? -1;
      const mh = markOf.get(i) ?? -1;
      this.ownerHere[i] = oh;
      this.markHere[i] = mh;
      const p = this.parent[i];
      if (p < 0) {
        this.owner[i] = oh;
        this.mark[i] = mh;
        continue;
      }
      this.owner[i] = oh >= 0 ? oh : this.owner[p];
      const up = this.mark[p];
      this.mark[i] = mh >= 0 && (up < 0 || markRank(marks[mh]) >= markRank(marks[up])) ? mh : up;
    }
    this.version++;
  }

  /** Stop counting a file's size (a second name of a hard-linked file). */
  uncount(i: number, flag: number): void {
    const size = this.own[i];
    if (size <= 0) return;
    this.own[i] = 0;
    this.flags[i] |= flag;
    for (let p = i; p >= 0; p = this.parent[p]) this.agg[p] -= size;
    this.version++;
  }

  /** A child of a folder by name (case ignored, as on Windows), or -1. */
  childNamed(parent: number, name: string): number {
    const want = name.toLowerCase();
    for (let c = this.firstChild[parent]; c >= 0; c = this.nextSibling[c]) {
      if (this.names[c].toLowerCase() === want) return c;
    }
    return -1;
  }

  riskOf(i: number): RiskId | null {
    const m = this.mark[i];
    return m >= 0 ? this.marks[m].risk : null;
  }
}

/** Lookups from node index to the owner and mark set directly on it. */
export function indexAnnotations(assignments: Int32Array, marks: Mark[]): { ownerOf: Map<number, number>; markOf: Map<number, number> } {
  const ownerOf = new Map<number, number>();
  for (let i = 0; i + 2 < assignments.length; i += 3) {
    const node = assignments[i];
    const conf = assignments[i + 2];
    // Keep the most confident claim per entry; low-confidence claims are not shown as ownership.
    if (conf >= 1) ownerOf.set(node, assignments[i + 1]);
  }
  const rank = markRank;
  const markOf = new Map<number, number>();
  marks.forEach((m, idx) => {
    const old = markOf.get(m.node);
    // Risk only goes up: if two marks land on one entry, the riskier one wins.
    if (old === undefined || rank(m) > rank(marks[old])) markOf.set(m.node, idx);
  });
  return { ownerOf, markOf };
}

import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { makeDemo } from "../../src/core/demo";
import { checkView, parseView, ViewError, type ViewFile } from "../../src/core/format";
import { DEFAULT_LOD, selectVisible, type LodOptions, type LodRequest } from "../../src/core/lod";
import { search } from "../../src/core/search";
import { Tree } from "../../src/core/tree";

const demo = makeDemo(20_000);
const data = checkView(demo);
const tree = buildTree(data);
const none: LodRequest = { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set() };

function clone(v: ViewFile): ViewFile {
  return JSON.parse(JSON.stringify(v)) as ViewFile;
}

function bad(change: (v: ViewFile) => void): string {
  const v = clone(makeDemo(2_000));
  change(v);
  try {
    checkView(v);
  } catch (e) {
    expect(e).toBeInstanceOf(ViewError);
    return (e as Error).message;
  }
  throw new Error("accepted a damaged file");
}

describe("format", () => {
  it("accepts the demo and keeps every entry", () => {
    expect(data.names.length).toBe(demo.nodes.name.length);
    expect(data.parent[0]).toBe(-1);
  });
  it("rejects damaged or contradictory files with a plain message", () => {
    expect(bad((v) => (v.schema = "x"))).toMatch(/not a DiskCleaner view file/);
    expect(bad((v) => (v.schema_version = 2))).toMatch(/version 2/);
    expect(bad((v) => (v.nodes.parent[5] = 5))).toMatch(/parent of entry 5/);
    expect(bad((v) => (v.nodes.parent[5] = v.nodes.kind.findIndex((k) => k === 0)))).toMatch(/not a folder|parent of entry/);
    expect(bad((v) => v.nodes.size.pop())).toMatch(/different length/);
    expect(bad((v) => (v.nodes.size[3] = -1))).toMatch(/size of entry 3/);
    expect(bad((v) => (v.nodes.size[0] = 10))).toMatch(/own size/);
    expect(bad((v) => (v.marks.find((m) => m.risk === "dont_touch")!.offered = true))).toMatch(/don't touch/);
    expect(bad((v) => (v.marks[0].node = 1e9))).toMatch(/entry/);
    expect(bad((v) => (v.assignments[0][1] = 999))).toMatch(/owner/);
    expect(bad((v) => (v.nodes.cat[4] = 40))).toMatch(/category/);
    expect(() => parseView("{\"schema\":")).toThrow(/not valid JSON/);
  });
});

describe("tree", () => {
  it("adds sizes up the tree", () => {
    const total = data.size.reduce((a, b) => a + b, 0);
    expect(tree.agg[0]).toBe(total);
    for (let i = 0; i < tree.n; i++) {
      let sum = tree.own[i];
      for (const c of tree.children(i)) sum += tree.agg[c];
      expect(sum).toBe(tree.agg[i]);
    }
  });
  it("picks the biggest children, and the rest adds up", () => {
    for (const p of [0, 1, 2, 5, 40]) {
      const all = [...tree.children(p)].sort((a, b) => tree.agg[b] - tree.agg[a]);
      const { top, restCount, restBytes } = tree.topChildren(p, 7);
      expect(top.map((t) => tree.agg[t])).toEqual(all.slice(0, 7).map((t) => tree.agg[t]));
      expect(restCount).toBe(Math.max(0, all.length - 7));
      expect(restBytes).toBe(all.slice(7).reduce((a, c) => a + tree.agg[c], 0));
    }
  });
  it("builds paths without the virtual root", () => {
    const i = data.names.indexOf("System32");
    expect(tree.path(i)).toBe("C:\\Windows\\System32");
    expect(tree.path(1)).toBe("C:");
  });
  it("lets risk only go up, and inherits owners", () => {
    const t = new Tree(8, [{ name: "A", kind: "app" }], [
      { node: 1, risk: "dont_touch", source: "refused", title: "", reason: "", detail: "", evidence: [], offered: false, bytes: 0 },
      { node: 2, risk: "safe", source: "rule", title: "", reason: "", detail: "", evidence: [], offered: true, bytes: 0 },
      { node: 4, risk: "probably_safe", source: "rule", title: "", reason: "", detail: "", evidence: [], offered: true, bytes: 0 },
      { node: 5, risk: "safe", source: "rule", title: "", reason: "", detail: "", evidence: [], offered: true, bytes: 0 },
    ], [], new Map([[1, 0]]), new Map([[1, 0], [2, 1], [4, 2], [5, 3]]));
    t.append(-1, "root", 1, 0, 0, 0, -1);
    t.append(0, "risky", 1, 0, 0, 0, -1); // 1
    t.append(1, "cache", 1, 0, 0, 0, -1); // 2 safe inside dont_touch
    t.append(0, "plain", 1, 0, 0, 0, -1); // 3
    t.append(3, "maybe", 1, 0, 0, 0, -1); // 4
    t.append(4, "cache2", 1, 0, 0, 0, -1); // 5 safe inside probably_safe
    expect(t.riskOf(2)).toBe("dont_touch");
    expect(t.riskOf(5)).toBe("probably_safe");
    expect(t.riskOf(3)).toBeNull();
    expect(t.owner[2]).toBe(0);
    expect(t.owner[3]).toBe(-1);
  });
  it("refuses entries that arrive before their parent or under a file", () => {
    const t = new Tree(4);
    t.append(-1, "root", 1, 0, 0, 0, -1);
    t.append(0, "f", 0, 5, 0, 0, -1);
    expect(() => t.append(1, "x", 0, 1, 0, 0, -1)).toThrow();
    expect(() => t.append(7, "x", 0, 1, 0, 0, -1)).toThrow();
  });
});

function checkVisible(opt: LodOptions, req: LodRequest): ReturnType<typeof selectVisible> {
  const v = selectVisible(tree, opt, req);
  expect(v.items.length).toBeLessThanOrEqual(opt.hardCap);
  for (const item of v.items) {
    if (item.parentKey === null) continue;
    const parent = v.byKey.get(item.parentKey);
    expect(parent, item.key).toBeDefined();
    expect(parent!.open).toBe(true);
  }
  // Sizes on screen add up for every open folder.
  for (const p of v.open) {
    let sum = tree.own[p];
    for (const item of v.items) {
      if (item.parentKey !== `n${p}`) continue;
      sum += item.group ? item.group.bytes : tree.agg[item.node];
    }
    expect(sum).toBe(tree.agg[p]);
  }
  return v;
}

describe("level of detail", () => {
  it("respects the budget and keeps every node attached", () => {
    for (const budget of [50, 300, 1500]) {
      const v = checkVisible({ ...DEFAULT_LOD, budget }, none);
      expect(v.items.length).toBeLessThanOrEqual(budget);
      expect(v.items.length).toBeGreaterThan(Math.min(budget, 50) / 2);
    }
  });
  it("is deterministic", () => {
    const a = selectVisible(tree, DEFAULT_LOD, none).items.map((i) => i.key);
    const b = selectVisible(tree, DEFAULT_LOD, none).items.map((i) => i.key);
    expect(a).toEqual(b);
  });
  it("shows a must-show entry and the whole way to it, even when tiny", () => {
    let tiny = -1;
    for (let i = tree.n - 1; i > 0; i--) if (tree.depth[i] >= 6 && tree.agg[i] < 50_000) { tiny = i; break; }
    expect(tiny).toBeGreaterThan(0);
    const v = checkVisible({ ...DEFAULT_LOD, budget: 60 }, { ...none, mustShow: new Set([tiny]) });
    expect(v.byKey.has(`n${tiny}`)).toBe(true);
    for (const a of tree.ancestors(tiny)) expect(v.open.has(a)).toBe(true);
  });
  it("opens requested folders before bigger ones, and keeps closed ones closed", () => {
    const small = data.names.indexOf("Music");
    const v = checkVisible({ ...DEFAULT_LOD, budget: 40 }, { ...none, mustShow: new Set([small]), opened: new Set([small]) });
    expect(v.open.has(small)).toBe(true);
    const win = data.names.indexOf("Windows");
    const w = checkVisible(DEFAULT_LOD, { ...none, closed: new Set([win]) });
    expect(w.open.has(win)).toBe(false);
  });
  it("shows many more children when a 'more' node is opened", () => {
    const sys32 = data.names.indexOf("System32");
    const base = checkVisible(DEFAULT_LOD, { ...none, opened: new Set([sys32]), mustShow: new Set([sys32]) });
    const wide = checkVisible(DEFAULT_LOD, { ...none, opened: new Set([sys32]), mustShow: new Set([sys32]), wide: new Set([sys32]) });
    const kids = (v: typeof base): number => v.items.filter((i) => i.parentKey === `n${sys32}`).length;
    expect(kids(wide)).toBeGreaterThan(kids(base));
  });
  it("never passes the hard cap, even when everything is requested", () => {
    const all = new Set<number>();
    for (let i = 0; i < tree.n; i++) if (tree.kind[i] === 1) all.add(i);
    const opt = { ...DEFAULT_LOD, hardCap: 800 };
    const v = checkVisible(opt, { ...none, opened: all });
    expect(v.items.length).toBeLessThanOrEqual(800);
    expect(v.truncated).toBe(true);
  });
});

describe("search", () => {
  it("finds by name, ignoring case, biggest first", () => {
    const hits = search(tree, "system32");
    expect(hits[0].path).toBe("C:\\Windows\\System32");
    const pics = search(tree, "img_");
    for (let i = 1; i < pics.length; i++) expect(tree.agg[pics[i - 1].node]).toBeGreaterThanOrEqual(tree.agg[pics[i].node]);
  });
  it("finds by the end of a path", () => {
    const hits = search(tree, "Roaming\\Norton");
    expect(hits.map((h) => h.path)).toContain("C:\\Users\\demo\\AppData\\Roaming\\Norton");
    expect(hits.map((h) => h.path)).not.toContain("C:\\ProgramData\\Norton");
  });
  it("treats special characters as plain text", () => {
    expect(() => search(tree, "(*[")).not.toThrow();
  });
});

describe("demo", () => {
  it("is the same every time and close to the size asked for", () => {
    expect(JSON.stringify(makeDemo(5_000))).toBe(JSON.stringify(makeDemo(5_000)));
    const n = makeDemo(100_000).nodes.name.length;
    expect(n).toBeGreaterThan(70_000);
    expect(n).toBeLessThan(140_000);
  });
});

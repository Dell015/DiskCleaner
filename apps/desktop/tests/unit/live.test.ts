import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { makeDemo } from "../../src/core/demo";
import { checkView, FLAG, KIND } from "../../src/core/format";
import { LiveTree, type LiveDir, type LiveSummary } from "../../src/live";

const data = checkView(makeDemo(8_000));
const full = buildTree(data);

/** Send the demo the way the scanner does: folders outward from the root, each with its entries. */
function batches(): { root: string; dirs: LiveDir[]; pathOf: string[] } {
  const n = data.names.length;
  const kids: number[][] = Array.from({ length: n }, () => []);
  for (let i = 1; i < n; i++) kids[data.parent[i]].push(i);
  const pathOf: string[] = new Array(n);
  pathOf[1] = data.names[1].replace(/[\\/]+$/, "");
  const dirs: LiveDir[] = [];
  const order = [1];
  for (let k = 0; k < order.length; k++) {
    const d = order[k];
    for (const c of kids[d]) {
      pathOf[c] = `${pathOf[d]}\\${data.names[c]}`;
      if (data.kind[c] === KIND.dir) order.push(c);
    }
    dirs.push({ dir: pathOf[d], entries: kids[d].map((c) => ({ name: data.names[c], kind: data.kind[c], size: data.size[c], mtime: data.mtime[c], flags: data.flags[c], cat: data.cat[c] })) });
  }
  return { root: data.names[1], dirs, pathOf };
}

describe("live tree", () => {
  it("rebuilds the same sizes as loading the whole file", () => {
    const { root, dirs } = batches();
    const lt = new LiveTree(root);
    let added = 0;
    for (let i = 0; i < dirs.length; i += 37) added += lt.add(dirs.slice(i, i + 37));
    expect(added).toBe(data.names.length - 2);
    expect(lt.tree.agg[0]).toBe(full.agg[0]);
    expect(lt.tree.files[0]).toBe(full.files[0]);
  });

  it("finds entries by path, attaches marks, and never offers what must not be touched", () => {
    const { root, dirs, pathOf } = batches();
    const lt = new LiveTree(root);
    lt.add(dirs);
    const file = data.kind.findIndex((k, i) => k === KIND.file && i > 1);
    expect(lt.find(pathOf[file])).toBeGreaterThan(1);
    expect(lt.find(pathOf[file].toUpperCase())).toBe(lt.find(pathOf[file]));
    expect(lt.find("Z:\\nowhere")).toBe(-1);
    const summary: LiveSummary = {
      status: "complete", errors: 0, files: 0, dirs: 0, bytes_on_disk: 0, apps_known: true,
      owners: data.owners, assignments: [], uncounted: [pathOf[file]],
      marks: data.marks.map((m) => ({ ...m, path: pathOf[m.node], offered: true })),
    };
    const before = lt.tree.agg[0];
    const matched = lt.finish(summary);
    expect(matched).toBeGreaterThan(5);
    // A file that must not be counted twice no longer adds to the totals.
    expect(lt.tree.agg[0]).toBe(before - data.size[file]);
    expect(lt.tree.flags[lt.find(pathOf[file])] & FLAG.notCounted).toBeTruthy();
    // Fail closed: a "don't touch" mark that claims to be offered is not offered.
    for (const m of lt.tree.marks) if (m.risk === "dont_touch" || m.source === "refused") expect(m.offered).toBe(false);
    // Risk only goes up through inheritance, as in a loaded file.
    const steam = lt.find(pathOf[data.names.indexOf("Steam")]);
    const inside = lt.tree.firstChild[steam];
    expect(lt.tree.riskOf(inside)).toBe("dont_touch");
  });

  it("skips entries whose folder was never announced", () => {
    const lt = new LiveTree("C:\\");
    expect(lt.add([{ dir: "C:\\missing", entries: [{ name: "x", kind: 0, size: 1, mtime: 0, flags: 0, cat: -1 }] }])).toBe(0);
  });
});

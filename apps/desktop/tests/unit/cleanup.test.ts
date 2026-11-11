import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";
import { attach, daysLeft, detach, eligibility, outermost } from "../../src/core/cleanup";

const tree = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));
const find = (p: string): number => {
  for (let i = 1; i < tree.n; i++) if (tree.path(i) === p) return i;
  throw new Error(`not found: ${p}`);
};
const group = (p: string): string => eligibility(tree, find(p)).group;

describe("which group a pick is in (same rules as the Rust side)", () => {
  it("follows the design", () => {
    expect(group("C:\\Users\\demo\\AppData\\Local\\Temp")).toBe("safe");
    expect(group("C:\\Users\\demo\\AppData\\Local\\CrashDumps")).toBe("probably_safe");
    expect(group("C:\\Users\\demo\\Downloads\\game-mods.zip")).toBe("your_call");
    expect(group("C:\\Tools\\Rufus")).toBe("your_call");
    expect(group("C:\\Windows")).toBe("never");
    expect(group("C:\\Program Files (x86)\\Steam")).toBe("never");
    expect(eligibility(tree, find("C:\\Program Files (x86)\\Steam")).why).toContain("uninstaller");
    expect(group("C:\\Users\\demo")).toBe("never");
    expect(group("C:\\Users\\demo\\OneDrive")).toBe("never");
    expect(eligibility(tree, 1).group).toBe("never");
    expect(eligibility(tree, 0).group).toBe("never");
  });
});

describe("taking removed entries out of the picture and back", () => {
  it("detach and attach keep every total right", () => {
    const dl = find("C:\\Users\\demo\\Downloads");
    const zip = find("C:\\Users\\demo\\Downloads\\game-mods.zip");
    const before = { root: tree.agg[0], dl: tree.agg[dl], files: tree.files[0], kids: tree.childCount[dl] };
    detach(tree, zip);
    expect(tree.agg[dl]).toBe(before.dl - tree.agg[zip]);
    expect(tree.childCount[dl]).toBe(before.kids - 1);
    let seen = false;
    for (let c = tree.firstChild[dl]; c >= 0; c = tree.nextSibling[c]) if (c === zip) seen = true;
    expect(seen).toBe(false);
    detach(tree, zip); // twice changes nothing
    expect(tree.agg[dl]).toBe(before.dl - tree.agg[zip]);
    attach(tree, zip);
    attach(tree, zip);
    expect({ root: tree.agg[0], dl: tree.agg[dl], files: tree.files[0], kids: tree.childCount[dl] }).toEqual(before);
  });

  it("nested picks count once, and days left count down", () => {
    const dl = find("C:\\Users\\demo\\Downloads");
    const zip = find("C:\\Users\\demo\\Downloads\\game-mods.zip");
    expect(outermost(tree, [zip, dl])).toEqual([dl]);
    expect(daysLeft(0, 30, 29.5 * 86_400)).toBe(1);
    expect(daysLeft(0, 30, 31 * 86_400)).toBe(0);
  });
});

import { describe, expect, it } from "vitest";
import { appCards, clearables } from "../../src/core/apps";
import { buildTree } from "../../src/core/build";
import { canDelete } from "../../src/core/explain";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";
import { homeFacts, shortPlace, UNUSED_SECS } from "../../src/panels/home";

describe("Home and Free up space (D140)", () => {
  const now = 1_762_000_000;
  const tree = buildTree(checkView(makeDemo(20_000, 42, now)));
  const f = homeFacts(tree, now, {});

  it("counts what can be freed exactly as the checks offer it", () => {
    const roots: number[] = [];
    for (let c = tree.firstChild[0]; c >= 0; c = tree.nextSibling[c]) roots.push(c);
    const list = clearables(tree, roots);
    expect(f.freeableCount).toBe(list.length);
    expect(f.freeable).toBe(list.reduce((a, c) => a + c.bytes, 0));
    expect(f.freeable).toBeLessThanOrEqual(f.total);
  });

  it("only calls apps unused when Windows recorded a start over six months ago", () => {
    for (const c of f.unused) {
      const last = tree.owners[c.owner].last_used ?? 0;
      expect(last).toBeGreaterThan(0);
      expect(now - last).toBeGreaterThan(UNUSED_SECS);
    }
    expect(f.unusedBytes).toBe(f.unused.reduce((a, c) => a + c.bytes, 0));
    // An app with no recorded start is never called unused.
    const unknown = appCards(tree).filter((c) => !tree.owners[c.owner].last_used);
    for (const c of unknown) expect(f.unused.some((u) => u.owner === c.owner)).toBe(false);
  });

  it("splits the space into buckets that add up", () => {
    const sum = Object.values(f.buckets).reduce((a, b) => a + b, 0);
    expect(sum).toBe(f.total);
  });

  it("says where in a few words", () => {
    expect(shortPlace("C:\\Games\\Genshin Impact game")).toBe("C:\\Games");
    expect(shortPlace("C:\\Users\\me\\AppData\\Local\\Discord")).toBe("AppData");
    expect(shortPlace("D:\\")).toBe("D:");
  });

  it("answers Can I delete it? cautiously", () => {
    expect(canDelete("safe", true).tone).toBe("safe");
    expect(canDelete("safe", false).tone).toBe("none"); // shown only, never offered: hands off
    expect(canDelete("probably_safe", true).tone).toBe("probably_safe");
    expect(canDelete("dont_touch", true).tone).toBe("dont_touch");
    expect(canDelete(null, false).text).toMatch(/hands off/);
  });
});

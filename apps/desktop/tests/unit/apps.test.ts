import { describe, expect, it } from "vitest";
import { appCards, clearables, isSystemPart, placeRole } from "../../src/core/apps";
import { buildTree } from "../../src/core/build";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";

describe("apps view data", () => {
  const tree = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));

  it("lists every installed app, biggest first, with its places", () => {
    const cards = appCards(tree);
    expect(cards.length).toBe(tree.owners.filter((o) => o.kind === "app").length);
    const withBytes = cards.filter((c) => c.bytes > 0);
    for (let k = 1; k < withBytes.length; k++) expect(withBytes[k - 1].bytes).toBeGreaterThanOrEqual(withBytes[k].bytes);
    for (const c of withBytes) expect(c.places.reduce((a, p) => a + tree.agg[p], 0)).toBe(c.bytes);
  });

  it("folds Windows parts and runtimes away", () => {
    expect(isSystemPart("Microsoft Visual C++ 2015-2022 Redistributable (x64)")).toBe(true);
    expect(isSystemPart("Microsoft Windows Desktop Runtime - 8.0.4")).toBe(true);
    expect(isSystemPart("NVIDIA Graphics Driver 551.86")).toBe(true);
    expect(isSystemPart("Microsoft Edge WebView2 Runtime")).toBe(true);
    for (const app of ["Steam", "Genshin Impact", "VLC media player", "Visual Studio Code", "Brave", "VALORANT", "Microsoft Teams"]) expect(isSystemPart(app), app).toBe(false);
  });

  it("only offers what the checks marked Safe or Probably safe, outermost first", () => {
    const chrome = appCards(tree).find((c) => tree.owners[c.owner].name === "Google Chrome");
    expect(chrome).toBeTruthy();
    const list = clearables(tree, chrome!.places);
    expect(list.length).toBeGreaterThan(0);
    for (const c of list) {
      const m = tree.marks[tree.markHere[c.node]];
      expect(m.offered).toBe(true);
      expect(["safe", "probably_safe"]).toContain(m.risk);
      for (const d of list) if (d !== c) expect(tree.isAncestor(d.node, c.node)).toBe(false);
    }
  });

  it("tells program files from data, saves and temporary files", () => {
    const find = (p: string) => { for (let i = 1; i < tree.n; i++) if (tree.path(i) === p) return i; return -1; };
    const steam = find("C:\\Program Files (x86)\\Steam");
    expect(placeRole(tree, steam, "C:\\Program Files (x86)\\Steam")).toBe("program");
    const temp = find("C:\\Users\\demo\\AppData\\Local\\Temp");
    if (temp > 0) expect(placeRole(tree, temp)).toBe("temp");
  });
});

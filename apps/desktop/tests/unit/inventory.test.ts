import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { checkView, KIND } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";
import { ago, appRows, bigFiles, groupOf, installerFor, looksLikeInstaller, portablePrograms, sortFolder, spaceBuckets, userFolders, words } from "../../src/core/inventory";

const NOW = 1_762_000_000;
const tree = buildTree(checkView(makeDemo(20_000, 42, NOW)));
const find = (path: string): number => {
  for (let i = 0; i < tree.n; i++) if (tree.path(i) === path) return i;
  return -1;
};

describe("file types", () => {
  it("knows installers from programs", () => {
    expect(looksLikeInstaller("ChromeSetup.exe")).toBe(true);
    expect(looksLikeInstaller("OBS-Studio-30.2-Full-Installer-x64.exe")).toBe(true);
    expect(looksLikeInstaller("Blender-4.2.1-windows-x64.msi")).toBe(true);
    expect(looksLikeInstaller("cpuz_x64.exe")).toBe(false);
    expect(looksLikeInstaller("setup.txt")).toBe(false);
    expect(groupOf("movie.MKV")).toBe("videos");
    expect(groupOf("Win11_23H2_x64.iso")).toBe("images");
    expect(groupOf("noext")).toBe("other");
  });

  it("matches installers to installed apps by name, as a hint", () => {
    expect(words("OBS-Studio-30.2-Full-Installer-x64.exe")).toEqual(["obs", "studio"]);
    const owners = tree.owners;
    const name = (f: string): string => owners[installerFor(f, owners)]?.name ?? "";
    expect(name("ChromeSetup.exe")).toBe("Google Chrome");
    expect(name("Blender-4.2.1-windows-x64.msi")).toBe("Blender");
    expect(name("OBS-Studio-30.2-Full-Installer-x64.exe")).toBe("OBS Studio");
    expect(name("DiscordSetup.exe")).toBe("Discord");
    expect(name("Git-2.46.0-64-bit.exe")).toBe("Git");
    expect(installerFor("setup.exe", owners)).toBe(-1);
  });
});

describe("the overview of the demo disk", () => {
  it("counts every byte once in the space buckets", () => {
    const b = spaceBuckets(tree);
    const sum = b.windows + b.apps + b.yours + b.other;
    expect(Math.abs(sum - tree.agg[0])).toBeLessThan(1);
    expect(b.windows).toBeGreaterThan(0);
    expect(b.apps).toBeGreaterThan(0);
    expect(b.yours).toBeGreaterThan(0);
    expect(userFolders(tree).map((i) => tree.names[i])).toContain("Downloads");
  });

  it("lists every installed app, also those that own nothing here", () => {
    const rows = appRows(tree);
    const zoom = rows.find((r) => tree.owners[r.owner].name === "Zoom Workplace");
    expect(zoom?.bytes).toBe(0);
    expect(rows.filter((r) => r.bytes > 0).every((r, i, a) => i === 0 || a[i - 1].bytes >= r.bytes)).toBe(true);
    expect(rows.some((r) => tree.owners[r.owner].last_used === undefined)).toBe(true);
    expect(rows.every((r) => tree.owners[r.owner].kind === "app")).toBe(true);
  });

  it("finds programs nobody installed, outermost folder only, without installers", () => {
    const found = portablePrograms(tree).map((p) => tree.path(p.node));
    expect(found).toEqual(expect.arrayContaining(["C:\\Tools\\Rufus", "C:\\Tools\\CPU-Z", "C:\\Tools\\platform-tools"]));
    expect(found).toContain("C:\\Users\\demo\\Downloads\\HWiNFO64");
    expect(found).toContain("C:\\Users\\demo\\Downloads\\speedtest.exe");
    expect(found.some((p) => p.startsWith("C:\\ProgramData"))).toBe(false);
    expect(found).not.toContain("C:\\Users\\demo\\Downloads\\Git-2.46.0-64-bit.exe");
    expect(found.some((p) => /setup|installer/i.test(p))).toBe(false);
    expect(found.some((p) => p.startsWith("C:\\Windows"))).toBe(false);
  });

  it("sorts Downloads by type, with the right hints", () => {
    const dl = find("C:\\Users\\demo\\Downloads");
    const s = sortFolder(tree, dl, NOW);
    const total = s.groups.reduce((a, g) => a + g.bytes, 0);
    let files = 0;
    const walk = (d: number): void => { for (let c = tree.firstChild[d]; c >= 0; c = tree.nextSibling[c]) { if (tree.kind[c] === KIND.dir) walk(c); else files += tree.agg[c]; } };
    walk(dl);
    expect(Math.abs(total - files)).toBeLessThan(1);
    expect(s.groups.map((g) => g.id)).toEqual(expect.arrayContaining(["installers", "archives", "images", "documents"]));
    for (const g of s.groups) for (let k = 1; k < g.top.length; k++) expect(tree.agg[g.top[k - 1]]).toBeGreaterThanOrEqual(tree.agg[g.top[k]]);
    const kinds = (k: string): string[] => s.hints.filter((h) => h.kind === k).map((h) => tree.names[h.node]);
    expect(kinds("installer_of_installed")).toEqual(expect.arrayContaining(["ChromeSetup.exe", "Blender-4.2.1-windows-x64.msi", "Git-2.46.0-64-bit.exe"]));
    expect(kinds("unpacked")).toEqual(["game-mods.zip"]);
    expect(kinds("untouched")).toContain("Win11_23H2_x64.iso");
  });

  it("big and old files are biggest first, and old ones are a year old or more", () => {
    const f = bigFiles(tree, NOW, 50);
    expect(f.big.length).toBe(50);
    expect(tree.names[f.big[0]]).toBe("pagefile.sys");
    for (const i of f.old) expect(NOW - tree.mtime[i]).toBeGreaterThanOrEqual(365 * 86_400);
    for (const i of f.old) expect(tree.path(i).startsWith("C:\\Windows\\")).toBe(false);
    expect(f.oldCount).toBeGreaterThanOrEqual(f.old.length);
  });

  it("says how long ago, and never guesses", () => {
    expect(ago(undefined, NOW)).toBe("not known");
    expect(ago(NOW - 3 * 86_400, NOW)).toBe("3 days ago");
    expect(ago(NOW - 400 * 86_400, NOW)).toBe("a year ago");
    expect(ago(NOW - 800 * 86_400, NOW)).toBe("2 years ago");
  });
});

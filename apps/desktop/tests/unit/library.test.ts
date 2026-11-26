import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";
import { appKind, guessKind, webAppOf as webApp } from "../../src/core/kinds";
import { sortFile, yourFiles } from "../../src/core/yours";

describe("kinds of apps", () => {
  it("sorts well-known apps the way people think of them", () => {
    const cases: [string, string | undefined, string][] = [
      ["Steam", "Valve Corporation", "launcher"],
      ["HoYoPlay", "COGNOSPHERE PTE. LTD.", "launcher"],
      ["Riot Client", "Riot Games, Inc", "launcher"],
      ["Epic Games Launcher", "Epic Games, Inc.", "launcher"],
      ["TLauncher", undefined, "launcher"],
      ["VALORANT", "Riot Games, Inc", "game"],
      ["Roblox Player for Randel", "Roblox Corporation", "game"],
      ["Roblox Studio for Randel", "Roblox Corporation", "dev"],
      ["Visual Studio Build Tools 2022", "Microsoft Corporation", "dev"],
      ["Microsoft Visual Studio Code", "Microsoft Corporation", "dev"],
      ["Docker Desktop", "Docker Inc.", "dev"],
      ["Python 3.14.4 (64-bit)", "Python Software Foundation", "dev"],
      ["Eclipse Temurin JDK with Hotspot 21", "Eclipse Adoptium", "dev"],
      ["Kiro (User)", "Amazon", "dev"],
      ["CapCut", "Bytedance Pte. Ltd.", "creative"],
      ["Figma", "Figma, Inc.", "creative"],
      ["OBS Studio", "OBS Project", "creative"],
      ["Brave", "Brave Software Inc", "browser"],
      ["Opera GX Stable 123.0", "Opera Software", "browser"],
      ["Discord", "Discord Inc.", "chat"],
      ["Zoom Workplace (64-bit)", "Zoom Communications, Inc.", "chat"],
      ["Microsoft Office Home 2024 - en-us", "Microsoft Corporation", "office"],
      ["Microsoft OneDrive", "Microsoft Corporation", "office"],
      ["Copilot", "Microsoft Corporation", "ai"],
      ["AMD Chat", "Advanced Micro Devices, Inc.", "ai"],
      ["Spotify", "Spotify AB", "media"],
      ["Logitech G HUB", "Logitech", "hardware"],
      ["AMD Install Manager", "Advanced Micro Devices, Inc.", "hardware"],
      ["AI Suite 3", "ASUSTeK Computer Inc.", "hardware"],
      ["Wallpaper Engine", undefined, "utility"],
      ["Tenorshare 4uKey for Android", "Tenorshare, Inc.", "utility"],
      ["Microsoft Visual C++ 2015-2022 Redistributable (x64)", "Microsoft Corporation", "system"],
      ["Something Nobody Knows", "Tiny Studio", "other"],
    ];
    for (const [name, pub, kind] of cases) expect(guessKind(name, pub), name).toBe(kind);
  });

  it("calls anything a launcher installed a game, and anything in a games folder too", () => {
    expect(guessKind("Genshin Impact", "COGNOSPHERE PTE. LTD.", undefined, "HoYoPlay")).toBe("game");
    expect(guessKind("Some Indie Thing", "Nobody", "D:\\SteamLibrary\\steamapps\\common\\Some Indie Thing")).toBe("game");
    expect(guessKind("Some Indie Thing", "Nobody", "C:\\Tools\\Thing")).toBe("other");
  });

  it("a kind the person chose wins", () => {
    const t = buildTree(checkView(makeDemo(5_000, 42, 1_762_000_000)));
    const chrome = t.owners.findIndex((o) => o.name === "Google Chrome");
    expect(appKind(t, chrome)).toBe("browser");
    expect(appKind(t, chrome, { "Google Chrome": "dev" })).toBe("dev");
    expect(appKind(t, chrome, { "Google Chrome": "nonsense" as never })).toBe("browser");
  });
});

describe("your files", () => {
  it("sorts files by what they are", () => {
    expect(sortFile("Screenshot 2026-01-01.png", "C:\\Users\\me\\Pictures\\")).toEqual(["pictures", "Screenshots"]);
    expect(sortFile("x.png", "C:\\Users\\me\\Pictures\\Screenshots\\")).toEqual(["pictures", "Screenshots"]);
    expect(sortFile("IMG_1234.JPG", "C:\\Users\\me\\Pictures\\")).toEqual(["pictures", "Camera photos"]);
    expect(sortFile("2026-03-01 18-22-10.mkv", "C:\\Users\\me\\Videos\\")).toEqual(["videos", "Recordings & clips"]);
    expect(sortFile("report.PDF", "C:\\Users\\me\\Documents\\")).toEqual(["documents", "PDFs"]);
    expect(sortFile("song.mp3", "D:\\Music\\")).toEqual(["music", "Songs & sounds"]);
    expect(sortFile("win11.iso", "D:\\stuff\\")).toEqual(["archives", "Disk images (iso, vhd)"]);
    expect(sortFile("noext", "D:\\stuff\\")).toEqual(["other", "Other files"]);
    expect(sortFile("x.unity3d", "D:\\stuff\\", "game_asset")).toEqual(["other", "Game files"]);
  });

  // A small disk: your Downloads, a code project, pictures, a program folder and a game.
  const names = ["C:\\", "Users", "me", "Downloads", "setup.exe", "Documents", "proj", "package.json", "node_modules", "lib.js", "Pictures", "Screenshots", "a.png", "IMG_0001.jpg", "Program Files", "App", "app.exe", "Games", "Game", "data.pak", "Windows", "notepad.exe"];
  const parent = [-1, 0, 1, 2, 3, 2, 5, 6, 6, 8, 2, 10, 11, 10, 0, 14, 15, 0, 17, 18, 0, 20];
  const dirs = new Set(["C:\\", "Users", "me", "Downloads", "Documents", "proj", "node_modules", "Pictures", "Screenshots", "Program Files", "App", "Games", "Game", "Windows"]);
  const size: number[] = names.map((n) => (dirs.has(n) ? 0 : 1000));
  size[9] = 50_000; // node_modules\lib.js
  const view = {
    schema: "dc-view", schema_version: 1, roots: ["C:\\"],
    categories: ["video", "image", "audio", "archive", "code", "document", "game_asset", "installer", "executable", "temporary", "other"],
    nodes: { name: names, parent, kind: names.map((n) => (dirs.has(n) ? 1 : 0)), size, mtime: names.map(() => 1_700_000_000), flags: names.map(() => 0), cat: names.map(() => -1) },
    owners: [{ name: "App", kind: "app" as const }, { name: "Game", kind: "app" as const }],
    assignments: [[15, 0, 2], [18, 1, 2]] as [number, number, number][],
    marks: [],
  };
  const tree = buildTree(checkView(view));
  const groups = yourFiles(tree);
  const g = (id: string) => groups.find((x) => x.id === id);

  it("counts Downloads, projects and pictures once each, and nothing an app owns or Windows holds", () => {
    expect(g("downloads")?.files).toBe(1);
    expect(g("downloads")?.kinds[0].label).toBe("Installers");
    expect(g("projects")?.files).toBe(2);
    expect(g("projects")?.bytes).toBe(51_000);
    expect(g("projects")?.rebuildable).toBe(50_000);
    expect(g("projects")?.kinds[0].label).toBe("JavaScript & TypeScript projects");
    expect(g("pictures")?.kinds.map((k) => k.label).sort()).toEqual(["Camera photos", "Screenshots"]);
    // app.exe (owned), data.pak (owned) and notepad.exe (Windows) are not yours.
    expect(groups.reduce((a, x) => a + x.files, 0)).toBe(5);
    expect(g("archives")).toBeUndefined();
    expect(g("other")).toBeUndefined();
  });

  it("a home folder with a package.json or .git is not one big project", () => {
    // C:\Users\me has package.json and .git (npm init / git init in the home folder), and AppData.
    const n = ["C:\\", "Users", "me", "package.json", ".git", "AppData", "Local", "cache.bin", "Documents", "app", "Cargo.toml", "main.rs", "notes.pdf"];
    const p = [-1, 0, 1, 2, 2, 2, 5, 6, 2, 8, 9, 9, 8];
    const d = new Set(["C:\\", "Users", "me", ".git", "AppData", "Local", "Documents", "app"]);
    const v = {
      schema: "dc-view", schema_version: 1, roots: ["C:\\"], categories: view.categories,
      nodes: { name: n, parent: p, kind: n.map((x) => (d.has(x) ? 1 : 0)), size: n.map((x) => (d.has(x) ? 0 : 100)), mtime: n.map(() => 1_700_000_000), flags: n.map(() => 0), cat: n.map(() => -1) },
      owners: [], assignments: [] as [number, number, number][], marks: [],
    };
    const gs = yourFiles(buildTree(checkView(v)));
    const proj = gs.find((x) => x.id === "projects");
    // Only Documents\app is a project; AppData is never counted; the PDF is a document.
    expect(proj?.files).toBe(2);
    expect(proj?.kinds[0].label).toBe("Rust projects");
    expect(gs.find((x) => x.id === "documents")?.files).toBe(2); // notes.pdf and the home package.json
    expect(gs.reduce((a, x) => a + x.files, 0)).toBe(4);
  });

  it("on the demo disk every counted file is counted once", () => {
    const t = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));
    const all = yourFiles(t);
    expect(all.length).toBeGreaterThan(3);
    const sum = all.reduce((a, x) => a + x.bytes, 0);
    expect(sum).toBeLessThanOrEqual(t.agg[0]);
    for (const x of all) expect(x.kinds.reduce((a, k) => a + k.bytes, 0)).toBe(x.bytes);
  });
});

describe("uninstalling in order", () => {
  it("puts a launcher's games and anti-cheat first and the launcher last; finds paths again", async () => {
    const { uninstallSteps, findPath } = await import("../../src/core/uninstall");
    const t = buildTree(checkView(makeDemo(5_000, 42, 1_762_000_000)));
    const steam = t.owners.findIndex((o) => o.name === "Steam");
    // Make the demo look like Riot: a game the launcher installed and an anti-cheat driver.
    const game = t.owners.findIndex((o) => o.name === "Discord");
    t.owners[game].launcher = "Steam";
    t.owners.push({ name: "Riot Vanguard", kind: "app", publisher: "Valve Corporation" });
    t.owners[steam].publisher = "Valve";
    const steps = uninstallSteps(t, steam, "launcher");
    expect(steps.map((s) => t.owners[s.owner].name)).toEqual(["Discord", "Riot Vanguard", "Steam"]);
    expect(steps[1].restart).toBe(true);
    expect(steps[0].restart).toBe(false);
    // Not a launcher: only itself.
    expect(uninstallSteps(t, game, "game").map((s) => s.owner)).toEqual([game]);
    // Paths are found again whatever the case.
    const some = 5;
    expect(findPath(t, t.path(some).toUpperCase())).toBe(some);
    expect(findPath(t, "C:\\no such folder")).toBe(-1);
  });
});

describe("web apps (a website installed as an app)", () => {
  it("are told apart from the real app", () => {
    expect(webApp("BraveSoftware\\Brave-Browser", "C:\\Users\\me\\AppData\\Local\\BraveSoftware\\Brave-Browser\\User Data\\Default\\Web Applications\\_crx_pjib")).toBe("Brave");
    expect(webApp("Google\\Chrome", "C:\\Users\\me\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Web Applications\\_crx_abc")).toBe("Chrome");
    expect(webApp("Microsoft\\Edge", undefined)).toBe("Edge");
    expect(webApp("Spotify AB", "C:\\Users\\me\\AppData\\Roaming\\Spotify")).toBeNull();
    expect(webApp("Microsoft Corporation", "C:\\Program Files\\Microsoft OneDrive")).toBeNull();
  });
});

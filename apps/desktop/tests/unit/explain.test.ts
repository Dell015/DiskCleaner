import { describe, expect, it } from "vitest";
import { buildTree } from "../../src/core/build";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";
import { askText, explain, known } from "../../src/core/explain";

describe("what is this?", () => {
  it("knows the places every Windows PC has", () => {
    const cases: [string, RegExp][] = [
      ["C:\\Windows\\WinSxS", /component store/],
      ["c:\\windows\\installer", /installers of your apps/],
      ["C:\\hiberfil.sys", /Hibernation/],
      ["C:\\pagefile.sys", /page file/],
      ["C:\\Windows.old", /previous Windows/],
      ["C:\\$Recycle.Bin", /Recycle Bin/],
      ["C:\\Users\\Randel\\AppData\\Local\\Temp", /Temporary files apps make/],
      ["C:\\Users\\Randel\\AppData\\Local\\Muse Hub", /Muse Hub/],
      ["D:\\code\\site\\node_modules", /JavaScript project/],
      ["D:\\code\\site\\.git", /history/],
      ["D:\\SteamLibrary\\steamapps\\common", /Steam games/],
      ["C:\\Users\\Randel\\OneDrive - Work", /OneDrive/],
      ["C:\\Users\\Randel\\Downloads", /Your downloads/],
      ["C:\\Program Files (x86)", /install their programs/],
    ];
    for (const [p, re] of cases) expect(known(p)?.what, p).toMatch(re);
    // Researched additions (Microsoft, Epic, pip and Adobe's own pages).
    const more: [string, RegExp][] = [
      ["C:\\Windows\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache", /Delivery Optimization/],
      ["C:\\Users\\R\\AppData\\Local\\Packages\\MSTeams_8wekyb3d8bbwe\\LocalCache\\Microsoft\\MSTeams", /new Teams/],
      ["C:\\Users\\R\\AppData\\Local\\EpicGamesLauncher\\Saved\\webcache_4430", /Epic Games Launcher/],
      ["C:\\Users\\R\\AppData\\Local\\pip\\Cache", /pip/],
      ["C:\\Users\\R\\AppData\\Roaming\\Adobe\\Common\\Media Cache Files", /media cache/],
      ["C:\\Users\\R\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache", /browser's cache/],
      ["C:\\Users\\R\\AppData\\Local\\BraveSoftware\\Brave-Browser\\User Data", /profiles: your logins/],
      ["C:\\Users\\R\\AppData\\Roaming\\.minecraft\\saves", /Minecraft worlds/],
      ["D:\\mail\\old.pst", /only on this PC/],
      ["C:\\AMD", /AMD driver installer/],
    ];
    for (const [p, re] of more) expect(known(p)?.what, p).toMatch(re);
    // Anything that can hold logins, saves or projects never says it is rebuilt.
    for (const p of ["C:\\Users\\R\\AppData\\Local\\Microsoft\\Edge\\User Data", "C:\\Users\\R\\AppData\\Roaming\\.minecraft\\saves", "C:\\Users\\R\\AppData\\Local\\CapCut\\User Data\\Projects", "D:\\x.pst", "D:\\code\\.git"]) {
      expect(known(p)?.ifDeleted, p).toMatch(/lost|gone|logged out/);
    }
    // The most specific entry wins.
    expect(known("C:\\Windows\\SoftwareDistribution\\Download")?.ifDeleted).toMatch(/downloads what it still needs/);
    expect(known("C:\\Windows")?.what).toBe("Windows itself.");
    // Not a known place.
    expect(known("C:\\Users\\Randel\\AppData\\Local\\SomethingOdd")).toBeUndefined();
    expect(known("C:\\Windows\\WinSxS\\amd64_thing")).toBeUndefined();
  });

  it("explains every entry of a scan, from what is known, the owning app, the checks or clues", () => {
    const t = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));
    const seen = new Set<string>();
    for (let i = 1; i < t.n; i++) {
      const x = explain(t, i);
      expect(x.what.length, t.path(i)).toBeGreaterThan(5);
      expect(x.ifDeleted.length, t.path(i)).toBeGreaterThan(5);
      if (x.from === "clues") expect(x.clues?.length).toBeGreaterThan(0);
      seen.add(x.from);
    }
    expect([...seen].sort()).toEqual(["app", "check", "clues", "known"]);
  });

  it("an app's program folder says to uninstall it; the AI question hides the user name", () => {
    const t = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));
    const chrome = t.owners.findIndex((o) => o.name === "Google Chrome");
    let place = -1;
    for (let i = 0; i < t.n && place < 0; i++) if (t.ownerHere[i] === chrome && t.kind[i] === 1 && /program files/i.test(t.path(i))) place = i;
    if (place >= 0) expect(explain(t, place).how).toMatch(/Uninstall Google Chrome/);
    let user = -1;
    for (let i = 0; i < t.n && user < 0; i++) if (/^C:\\Users\\[^\\]+\\.+/i.test(t.path(i)) && explain(t, i).from === "clues") user = i;
    if (user >= 0) {
      const q = askText(t, user);
      expect(q).toContain("%USERPROFILE%");
      expect(q).not.toMatch(/C:\\Users\\[^%]/i);
    }
  });
});

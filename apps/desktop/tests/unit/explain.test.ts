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

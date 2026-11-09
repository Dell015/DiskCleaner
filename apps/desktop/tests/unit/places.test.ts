import { describe, expect, it } from "vitest";
import { placeKind } from "../../src/panels/panels";

describe("where an app keeps things", () => {
  it("names each kind of place", () => {
    expect(placeKind("C:\\Riot Games")).toBe("Top of the drive");
    expect(placeKind("E:\\SteamLibrary")).toBe("Top of the drive");
    expect(placeKind("C:\\Program Files (x86)\\Steam")).toBe("Program files");
    expect(placeKind("C:\\Program Files\\Norton")).toBe("Program files");
    expect(placeKind("C:\\Users\\bob\\AppData\\Roaming\\discord")).toBe("App data");
    expect(placeKind("C:\\Users\\bob\\AppData\\Local\\Temp\\x")).toBe("Temporary files");
    expect(placeKind("C:\\ProgramData\\McAfee")).toBe("Shared app data");
    expect(placeKind("C:\\Users\\bob\\Documents\\My Games\\THE FINALS")).toBe("Game saves");
    expect(placeKind("C:\\Users\\bob\\Saved Games\\Game")).toBe("Game saves");
    expect(placeKind("C:\\Users\\bob\\Documents\\Zoom")).toBe("Documents");
    expect(placeKind("D:\\Stuff\\Tools\\App")).toBe("Elsewhere");
  });
});

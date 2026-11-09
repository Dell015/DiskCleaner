import { afterEach, describe, expect, it } from "vitest";
import { human, setSizeUnits } from "../../src/core/util";

describe("size units", () => {
  afterEach(() => setSizeUnits("windows"));

  it("writes sizes like File Explorer by default (1024s, GB)", () => {
    expect(human(0)).toBe("0 B");
    expect(human(1023)).toBe("1023 B");
    expect(human(1536)).toBe("1.50 KB");
    expect(human(5 * 1024 ** 3)).toBe("5.00 GB");
    expect(human(931.32 * 1024 ** 3)).toBe("931 GB");
  });

  it("can use the precise names (GiB)", () => {
    setSizeUnits("binary");
    expect(human(5 * 1024 ** 3)).toBe("5.00 GiB");
    expect(human(1536)).toBe("1.50 KiB");
  });

  it("can count like drive boxes (1000s)", () => {
    setSizeUnits("decimal");
    expect(human(1e12)).toBe("1.00 TB");
    expect(human(1500)).toBe("1.50 KB");
    expect(human(999)).toBe("999 B");
  });

  it("an explicit unit argument wins and never prints 1000 or 1024 of a unit", () => {
    expect(human(1e12, "windows")).toBe("931 GB");
    expect(human(1024 ** 3 - 1, "windows")).toBe("1.00 GB");
    expect(human(999_999_999, "decimal")).toBe("1.00 GB");
  });
});

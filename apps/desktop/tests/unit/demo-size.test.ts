import { expect, it } from "vitest";
import { makeDemo } from "../../src/core/demo";

it("demo size follows the request", () => {
  for (const target of [10_000, 150_000, 600_000]) {
    const n = makeDemo(target).nodes.name.length;
    expect(n / target).toBeGreaterThan(0.8);
    expect(n / target).toBeLessThan(1.2);
  }
}, 120_000);

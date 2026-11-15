import { describe, expect, it } from "vitest";
import { clearables, layoutNetwork, NET_H, NET_W, placeRole, type PlaceRole } from "../../src/core/apps";
import { buildTree } from "../../src/core/build";
import { checkView } from "../../src/core/format";
import { makeDemo } from "../../src/core/demo";

describe("an app's network drawing", () => {
  const tree = buildTree(checkView(makeDemo(20_000, 42, 1_762_000_000)));
  // Every top folder of the demo drive as "places", to test a crowded drawing too.
  const tops: number[] = [];
  for (let c = tree.firstChild[1]; c >= 0; c = tree.nextSibling[c]) tops.push(c);
  tops.sort((a, b) => tree.agg[b] - tree.agg[a]);

  it("keeps every circle inside the drawing and hangs each from the app or a place", () => {
    for (const places of [tops.slice(0, 1), tops.slice(0, 2), tops]) {
      const roles = new Map(places.map((p) => [p, placeRole(tree, p)] as [number, PlaceRole]));
      const net = layoutNetwork(tree, places, roles, clearables(tree, places), "Test");
      expect(net[0].kind).toBe("app");
      for (const d of net.slice(1)) {
        expect(d.x - d.r).toBeGreaterThanOrEqual(0);
        expect(d.x + d.r).toBeLessThanOrEqual(NET_W);
        expect(d.y - d.r).toBeGreaterThanOrEqual(0);
        expect(d.y + d.r).toBeLessThanOrEqual(NET_H);
        expect(d.from).toBeGreaterThanOrEqual(0);
        expect(d.from).toBeLessThan(net.indexOf(d));
      }
      // At most 12 places plus one "more" circle.
      expect(net.filter((d) => d.from === 0).length).toBeLessThanOrEqual(13);
    }
  });

  it("bigger places get bigger circles", () => {
    const places = tops.slice(0, 5);
    const roles = new Map(places.map((p) => [p, placeRole(tree, p)] as [number, PlaceRole]));
    const ring = layoutNetwork(tree, places, roles, [], "Test").filter((d) => d.from === 0);
    for (let k = 1; k < ring.length; k++) expect(ring[k - 1].r).toBeGreaterThanOrEqual(ring[k].r);
  });
});

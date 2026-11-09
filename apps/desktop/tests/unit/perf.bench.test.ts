import { it } from "vitest";
import { buildTree } from "../../src/core/build";
import { makeDemo } from "../../src/core/demo";
import { checkView } from "../../src/core/format";
import { DEFAULT_LOD, selectVisible } from "../../src/core/lod";
import { search } from "../../src/core/search";

// Timing at the size of a real profile scan. Run with: DC_PERF=1 npx vitest run perf
it.skipIf(!(globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env.DC_PERF)("timing at 1.5 million entries", () => {
  const t = (label: string, f: () => unknown): unknown => {
    const s = performance.now();
    const v = f();
    console.log(`${label.padEnd(28)} ${(performance.now() - s).toFixed(0)} ms`);
    return v;
  };
  const demo = t("make demo", () => makeDemo(1_500_000)) as ReturnType<typeof makeDemo>;
  console.log("entries", demo.nodes.name.length);
  const json = t("stringify", () => JSON.stringify(demo)) as string;
  console.log("file size MB", (json.length / 1e6).toFixed(1));
  const raw = t("JSON.parse", () => JSON.parse(json));
  const data = t("check", () => checkView(raw)) as ReturnType<typeof checkView>;
  const tree = t("build tree", () => buildTree(data)) as ReturnType<typeof buildTree>;
  const sv = (o: typeof DEFAULT_LOD) => selectVisible(tree, o, { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set() });
  console.log("items default", sv(DEFAULT_LOD).items.length, "budget5000", sv({ ...DEFAULT_LOD, budget: 5000 }).items.length, "minShare0.002", sv({ ...DEFAULT_LOD, budget: 5000, minShare: 0.002 }).items.length);
  t("select visible (default)", () => selectVisible(tree, DEFAULT_LOD, { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set() }));
  t("select visible (budget 5000)", () => selectVisible(tree, { ...DEFAULT_LOD, budget: 5000 }, { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set() }));
  t("search 'cache'", () => search(tree, "cache"));
  t("search path", () => search(tree, "Roaming\\Norton"));
}, 300_000);

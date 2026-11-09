// The node design system in code: how an entry is drawn from what it is. The same rules are
// written out in docs/phases/f2-ui-polish/node-design-system.md and shown on the Key page.
//
//   size   = radius grows with size^0.42 (graph.ts), never smaller than a 2.2 px dot
//   fill   = what colour mode asks for (risk: folder/file tint by type; app: owner; type: category)
//   shape  = what it is and what the checks say:
//              file            plain dot
//              folder          dot with a lighter rim
//              open folder     hollow ring (its contents are around it)
//              safe            solid dot in the safe colour
//              probably safe   ring in the probably-safe colour around the normal dot
//              don't touch     bullseye in the don't-touch colour
//              link            thin hollow ring in the link colour (never followed)
//              cloud           ring with a pale centre
//              "N more"        faint hollow ring

import { FLAG, KIND, type RiskId } from "../core/format";
import type { Tree } from "../core/tree";
import { mix, type Palette } from "./style";

export type ColorMode = "risk" | "app" | "type";
export type Shape = "root" | "file" | "folder" | "open" | "safe" | "probably" | "dont" | "link" | "cloud" | "group";

export interface Look {
  shape: Shape;
  color: string;
  ringColor: string;
  ringSize: number;
  gapColor: string;
  gapSize: number;
  labelWeight: number;
  labelColor: string;
}

/** Fill colour for an entry in a colour mode (before the shape is applied). */
export function baseColor(tree: Tree, i: number, mode: ColorMode, p: Palette): string {
  const isDir = tree.kind[i] === KIND.dir;
  if (mode === "app") {
    const o = tree.owner[i];
    if (o < 0) return isDir ? p.noOwner : mix(p.noOwner, p.text2, 0.2);
    const c = tree.owners[o].kind === "windows" ? p.windows : p.apps[o % p.apps.length];
    return isDir ? c : mix(c, p.bg, 0.15);
  }
  if (mode === "type") {
    const cat = tree.dominantCategory(i);
    if (cat < 0) return isDir ? p.noOwner : mix(p.noOwner, p.text2, 0.2);
    return isDir ? mix(p.cats[cat % p.cats.length], p.folder, 0.3) : p.cats[cat % p.cats.length];
  }
  if (isDir) return p.folder;
  const cat = tree.cat[i];
  // Files carry a hint of their type even in risk mode, so the picture is never flat grey.
  return cat >= 0 ? mix(p.cats[cat % p.cats.length], p.file, 0.55) : p.file;
}

function shapeOf(risk: RiskId | null, mode: ColorMode, isDir: boolean, open: boolean): Shape {
  if (open) return "open";
  if (mode === "risk" && risk) return risk === "safe" ? "safe" : risk === "probably_safe" ? "probably" : "dont";
  return isDir ? "folder" : "file";
}

/** How one entry looks. */
export function lookOf(tree: Tree, i: number, open: boolean, isRoot: boolean, mode: ColorMode, p: Palette): Look {
  const isDir = tree.kind[i] === KIND.dir;
  const risk = tree.riskOf(i);
  const isLink = tree.kind[i] === KIND.link || (tree.flags[i] & FLAG.link) !== 0;
  const isCloud = (tree.flags[i] & FLAG.cloud) !== 0;
  const base = baseColor(tree, i, mode, p);
  const label = { labelWeight: isRoot ? 700 : isDir ? 600 : 400, labelColor: p.text };
  if (isRoot) {
    return { shape: "root", color: p.text, ringColor: mix(p.text, p.bg, 0.55), ringSize: 0.18, gapColor: p.bg, gapSize: 0.1, ...label };
  }
  if (isLink) return { shape: "link", color: p.bg, ringColor: p.link, ringSize: 0.2, gapColor: p.bg, gapSize: 0, ...label, labelColor: p.text2 };
  if (isCloud && !isDir) return { shape: "cloud", color: mix(p.cloud, p.bg, 0.6), ringColor: p.cloud, ringSize: 0.28, gapColor: p.bg, gapSize: 0, ...label };
  const shape = shapeOf(risk, mode, isDir, open);
  const riskColor = risk ? p.risk[risk] : base;
  switch (shape) {
    case "open": {
      const c = mode === "risk" && risk ? riskColor : base;
      return { shape, color: p.bg, ringColor: c, ringSize: 0.22, gapColor: p.bg, gapSize: 0, ...label };
    }
    case "safe":
      return { shape, color: p.risk.safe, ringColor: isDir ? mix(p.risk.safe, p.text, 0.35) : p.risk.safe, ringSize: isDir ? 0.16 : 0, gapColor: p.bg, gapSize: 0, ...label };
    case "probably":
      return { shape, color: base, ringColor: p.risk.probably_safe, ringSize: 0.3, gapColor: p.bg, gapSize: 0.14, ...label };
    case "dont":
      return { shape, color: p.risk.dont_touch, ringColor: p.risk.dont_touch, ringSize: 0.3, gapColor: p.bg, gapSize: 0.2, ...label };
    case "folder":
      return { shape, color: base, ringColor: mix(base, p.text, 0.35), ringSize: 0.16, gapColor: p.bg, gapSize: 0, ...label };
    default:
      return { shape: "file", color: base, ringColor: base, ringSize: 0, gapColor: p.bg, gapSize: 0, ...label };
  }
}

/** The "N more items" node. */
export function groupLook(p: Palette): Look {
  return { shape: "group", color: p.bg, ringColor: p.group, ringSize: 0.16, gapColor: p.bg, gapSize: 0, labelWeight: 400, labelColor: p.text2 };
}

/** The same look faded into the background (unrelated to the selection, or filtered out). */
export function dimLook(l: Look, p: Palette, t = 0.8): Look {
  return { ...l, color: mix(l.color, p.bg, t), ringColor: mix(l.ringColor, p.bg, t), gapColor: mix(l.gapColor, p.bg, t), labelColor: mix(l.labelColor, p.bg, t) };
}

export const SHAPE_NAMES: Record<Shape, string> = {
  root: "Scanned drive or folder", file: "File", folder: "Folder", open: "Open folder (its contents are around it)",
  safe: "Safe to remove (regenerates or is junk)", probably: "Probably safe (look first)", dont: "Don't touch",
  link: "Link or junction (never followed)", cloud: "Cloud placeholder (not on this disk)", group: "More items, too small to show",
};

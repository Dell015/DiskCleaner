// The graph on screen. Two sigma layers share one graph: the front layer draws what matters, the
// back layer draws everything unrelated to the selection, blurred and grey (when the graphics
// settings allow it). Between them a 2D canvas draws glow and the light pulses of the scan.
// This file only draws; lod.ts decides what is visible and encode.ts how each node looks.

import Graph from "graphology";
import Sigma from "sigma";
import { createNodeBorderProgram } from "@sigma/node-border";
import EdgeCurveProgram from "@sigma/edge-curve";
import type { Visible, VisItem } from "../core/lod";
import type { Tree } from "../core/tree";
import { human } from "../core/util";
import { dimLook, groupLook, lookOf, type ColorMode, type Look } from "./encode";
import { ForceLayout } from "./layout";
import { PRESETS, type Quality } from "./quality";
import { applyCss, mix, paletteOf, rgba, STYLES, type Palette, type Style } from "./style";

/** Radius of the root, in graph units. Bigger means bigger, but softer than area. */
export const R0 = 34;
export const SIZE_EXP = 0.42;
const MIN_R = 0.9;
/** Smallest radius drawn, in pixels. */
const MIN_PX = 2.2;
const GROW_MS = 360;
const FRESH_MS = 1100;
const PULSE_MS = 650;
const LEAVE_MS = 240;
const FONT = "'Atkinson Hyperlegible Next Variable', 'Segoe UI', system-ui, sans-serif";

export interface GraphHandlers {
  select(key: string | null): void;
  toggle(key: string): void;
  hover(key: string | null, x: number, y: number): void;
  cameraMoved(): void;
  context(key: string, x: number, y: number): void;
}

type Attrs = Record<string, unknown>;

export function radiusFor(bytes: number, rootBytes: number): number {
  return Math.max(MIN_R, R0 * Math.pow(Math.max(0, bytes) / Math.max(1, rootBytes), SIZE_EXP));
}

const NodeProgram = createNodeBorderProgram({
  borders: [
    { size: { attribute: "ringSize", defaultValue: 0 }, color: { attribute: "ringColor" } },
    { size: { attribute: "gapSize", defaultValue: 0 }, color: { attribute: "gapColor" } },
    { size: { fill: true }, color: { attribute: "color" } },
  ],
});

export class GraphView {
  readonly graph = new Graph<Attrs, Attrs>();
  readonly sigma: Sigma<Attrs, Attrs>;
  private back: Sigma<Attrs, Attrs>;
  private fx: HTMLCanvasElement;
  private layout: ForceLayout;
  style: Style = STYLES.deep;
  dark = true;
  pal: Palette = STYLES.deep.dark;
  quality: Quality = PRESETS.high;
  mode: ColorMode = "risk";
  focusOwner = -1;
  rootNode = 0;
  /** After the first growth, changes are laid out locally so the picture holds still. */
  stable = false;
  private tree: Tree | null = null;
  private hovered: string | null = null;
  private selected: string | null = null;
  private highlight = new Set<string>();
  private chain = new Set<string>();
  private born = new Map<string, number>();
  private fresh = new Map<string, number>();
  private leaving = new Map<string, { t: number; x: number; y: number; to: string }>();
  private glowKeys: string[] = [];
  private head: string | null = null;
  private dragging: string | null = null;
  private dragMoved = false;
  private animating = false;
  private frozen = false;
  private cameraTimer = 0;
  private lastFit = 0;
  private unitPx = 1;

  constructor(private readonly host: HTMLElement, private readonly on: GraphHandlers) {
    host.classList.add("graph-host");
    const backEl = document.createElement("div");
    backEl.className = "layer back";
    this.fx = document.createElement("canvas");
    this.fx.className = "layer fx";
    const frontEl = document.createElement("div");
    frontEl.className = "layer front";
    host.append(backEl, this.fx, frontEl);

    const common = {
      itemSizesReference: "positions" as const,
      zoomToSizeRatioFunction: (r: number) => r,
      autoRescale: true,
      labelFont: FONT,
      labelSize: 12,
      labelWeight: "600",
      labelColor: { attribute: "labelColor" },
      minCameraRatio: 0.0003,
      maxCameraRatio: 20,
      zIndex: true,
      allowInvalidContainer: true,
      renderEdgeLabels: false,
      enableEdgeEvents: false,
      defaultNodeType: "styled",
      nodeProgramClasses: { styled: NodeProgram },
      edgeProgramClasses: { curved: EdgeCurveProgram },
      defaultDrawNodeLabel: (ctx: CanvasRenderingContext2D, d: Attrs, s: { labelSize: number; labelFont: string }) => this.drawLabel(ctx, d, s),
      defaultDrawNodeHover: (ctx: CanvasRenderingContext2D, d: Attrs, s: { labelSize: number; labelFont: string }) => this.drawLabel(ctx, { ...d, label: d.label ?? d.hoverLabel, labelWeight: 700 }, s),
    };
    // Sigma's own typings do not know the extra attributes the label function reads.
    this.back = new Sigma(this.graph, backEl, {
      ...common,
      labelRenderedSizeThreshold: 1e9,
      nodeReducer: (key: string, a: Attrs) => this.reduce(key, a, "back"),
      edgeReducer: (key: string, a: Attrs) => this.reduceEdge(key, a, "back"),
    } as never);
    this.sigma = new Sigma(this.graph, frontEl, {
      ...common,
      labelRenderedSizeThreshold: 3,
      labelDensity: this.quality.labels,
      labelGridCellSize: 110,
      nodeReducer: (key: string, a: Attrs) => this.reduce(key, a, "front"),
      edgeReducer: (key: string, a: Attrs) => this.reduceEdge(key, a, "front"),
    } as never);
    this.layout = new ForceLayout(this.graph);
    this.bindEvents();
    this.setStyle(this.style, this.dark);
    new ResizeObserver(() => this.sizeFx()).observe(host);
    this.sizeFx();
  }

  // ---------- style and settings ----------

  setStyle(style: Style, dark: boolean): void {
    this.style = style;
    this.dark = dark;
    this.pal = paletteOf(style, dark);
    applyCss(this.pal, style);
    this.relook();
  }

  setQuality(q: Quality): void {
    this.quality = q;
    this.sigma.setSetting("labelDensity", q.labels);
    this.host.classList.toggle("no-blur", !q.focusBlur);
    this.refreshAll();
  }

  setMode(mode: ColorMode): void {
    this.mode = mode;
    this.relook();
  }

  /** Work out every node's look again (style, theme or colour mode changed). */
  relook(): void {
    const tree = this.tree;
    if (!tree) return;
    this.graph.updateEachNodeAttributes((_k, a) => {
      const node = a.node as number;
      const look = node >= 0 ? lookOf(tree, node, Boolean(a.open), node === this.rootNode, this.mode, this.pal) : groupLook(this.pal);
      return { ...a, ...look };
    });
    this.refreshAll();
  }

  private refreshAll(): void {
    this.sigma.refresh({ skipIndexation: true });
    this.back.refresh({ skipIndexation: true });
  }

  // ---------- data in ----------

  /** Bring the graph in line with the visible set. */
  sync(tree: Tree, vis: Visible, animate: boolean): { added: number; removed: number } {
    this.tree = tree;
    const g = this.graph;
    const now = performance.now();
    const motion = animate && this.quality.motion !== "off";
    const drop: string[] = [];
    g.forEachNode((k, a) => {
      if (!vis.byKey.has(k) && !a.leaving) drop.push(k);
    });
    let removed = 0;
    for (const k of drop) {
      const a = g.getNodeAttributes(k);
      const to = a.parentKey as string | undefined;
      // Closing a folder: its contents fold back into it instead of vanishing.
      if (motion && this.stable && this.quality.motion === "full" && to && vis.byKey.has(to)) {
        g.setNodeAttribute(k, "leaving", true);
        this.leaving.set(k, { t: now, x: a.x as number, y: a.y as number, to });
      } else {
        g.dropNode(k);
        this.born.delete(k);
      }
      removed++;
    }
    if (this.selected && !vis.byKey.has(this.selected)) this.selected = null;

    const newKids = new Map<string, VisItem[]>();
    for (const item of vis.items) {
      if (!g.hasNode(item.key) && item.parentKey) {
        const list = newKids.get(item.parentKey);
        if (list) list.push(item);
        else newKids.set(item.parentKey, [item]);
      }
    }
    let added = 0;
    let resized = false;
    const rootBytes = tree.agg[this.rootNode] || tree.agg[0];
    for (const item of vis.items) {
      const attrs = this.attrsFor(tree, item, rootBytes);
      if (g.hasNode(item.key)) {
        const old = g.getNodeAttributes(item.key);
        if (old.leaving) {
          this.leaving.delete(item.key);
          g.removeNodeAttribute(item.key, "leaving");
        }
        if (old.size !== attrs.size || old.color !== attrs.color || old.label !== attrs.label || old.open !== attrs.open || old.ringColor !== attrs.ringColor) {
          if (Math.abs((old.size as number) - (attrs.size as number)) > 0.05 * (attrs.size as number)) resized = true;
          g.mergeNodeAttributes(item.key, attrs);
        }
        continue;
      }
      const pos = this.seed(item, attrs.size as number, newKids);
      g.addNode(item.key, { ...attrs, ...pos });
      added++;
      if (motion) this.born.set(item.key, now);
      if (item.parentKey && g.hasNode(item.parentKey)) {
        const e = g.addDirectedEdgeWithKey(`${item.parentKey}>${item.key}`, item.parentKey, item.key, {
          size: item.group ? 0.7 : 1,
          weight: item.group ? 0.5 : 1,
          curvature: this.curvatureFor(item.key),
        });
        if (motion) this.fresh.set(e, now);
      }
    }
    // The biggest visible folders get a soft light (if glow is on).
    this.glowKeys = vis.items
      .filter((it) => it.node >= 0 && it.node !== this.rootNode)
      .sort((a, b) => tree.agg[b.node] - tree.agg[a.node])
      .slice(0, 10)
      .map((it) => it.key);
    if (added > 0 || removed > 0) this.kickLayout(added + removed);
    else if (resized) {
      this.layout.resize();
      this.layout.heat(0.05);
    }
    if (!this.frozen) this.fitBox();
    this.updateHighlight();
    this.animate();
    return { added, removed };
  }

  private curvatureFor(key: string): number {
    let h = 0;
    for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
    return (h & 1 ? 1 : -1) * this.style.curvature;
  }

  private attrsFor(tree: Tree, item: VisItem, rootBytes: number): Attrs {
    if (item.group) {
      const g = item.group;
      return {
        node: -1, parentKey: item.parentKey, open: false, zIndex: 0,
        // Named on hover only: these are secondary.
        label: null,
        hoverLabel: `${g.count.toLocaleString("en-US")} more, ${human(g.bytes)}`,
        size: radiusFor(g.bytes, rootBytes),
        ...groupLook(this.pal),
      };
    }
    const i = item.node;
    const isRoot = i === this.rootNode;
    const raw = i === 0 ? "This PC" : tree.names[i];
    return {
      node: i, parentKey: item.parentKey, open: item.open, zIndex: isRoot ? 2 : 1,
      label: raw.replace(/[\\/]+$/, "") || raw,
      size: radiusFor(tree.agg[i], rootBytes),
      ...lookOf(tree, i, item.open, isRoot, this.mode, this.pal),
    };
  }

  /** Where a new node starts: after the first growth, at its folder (it spreads out from there). */
  private seed(item: VisItem, size: number, newKids: Map<string, VisItem[]>): { x: number; y: number } {
    const g = this.graph;
    if (!item.parentKey || !g.hasNode(item.parentKey)) return { x: 0, y: 0 };
    const p = g.getNodeAttributes(item.parentKey);
    const px = p.x as number;
    const py = p.y as number;
    const pr = p.size as number;
    const kids = newKids.get(item.parentKey) ?? [item];
    const rank = Math.max(0, kids.indexOf(item));
    const count = kids.length;
    let base = 0;
    let arc = Math.PI * 2;
    const gpKey = g.inNeighbors(item.parentKey)[0];
    if (gpKey) {
      const gp = g.getNodeAttributes(gpKey);
      base = Math.atan2(py - (gp.y as number), px - (gp.x as number));
      arc = Math.PI * 1.3;
    }
    const angle = arc >= Math.PI * 2 ? (rank / count) * arc : base + ((rank + 0.5) / count - 0.5) * arc;
    // Emerging from the folder: start just outside its rim and let the layout push them out.
    const dist = this.stable ? pr + size * 0.5 : pr + size + 2 + Math.sqrt(count) * 1.5 + (rank % 3) * 0.8;
    return { x: px + Math.cos(angle) * dist, y: py + Math.sin(angle) * dist };
  }

  // ---------- layout and camera ----------

  private kickLayout(changes: number): void {
    this.layout.rebuild(this.stable);
    this.layout.heat(this.stable ? 0.5 : Math.min(0.6, 0.15 + changes / 400));
    this.animate();
  }

  layoutRunning(): boolean {
    return this.layout.running();
  }

  freeze(on: boolean): void {
    this.frozen = on;
    if (!on) this.fitBox();
  }

  isFrozen(): boolean {
    return this.frozen;
  }

  private fitBox(): void {
    if (this.graph.order === 0) return;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    this.graph.forEachNode((_k, a) => {
      if (a.leaving) return;
      const x = a.x as number, y = a.y as number, r = a.size as number;
      x0 = Math.min(x0, x - r); y0 = Math.min(y0, y - r);
      x1 = Math.max(x1, x + r); y1 = Math.max(y1, y + r);
    });
    if (!Number.isFinite(x0)) return;
    // Never frame less than a minimum area, so the first nodes start small and the graph visibly
    // reaches outward as the scan finds more.
    const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    const half = Math.max((x1 - x0) / 2, (y1 - y0) / 2, R0 * 5) * 1.06 + 1;
    const box = { x: [cx - half, cx + half] as [number, number], y: [cy - half, cy + half] as [number, number] };
    this.sigma.setCustomBBox(box);
    this.back.setCustomBBox(box);
  }

  fit(): void {
    this.fitBox();
    this.move({ x: 0.5, y: 0.5, ratio: 1, angle: 0 }, 480);
  }

  /** Move the camera; a long trip pulls back first so you see where you are going. */
  private move(target: { x: number; y: number; ratio: number; angle?: number }, ms: number): void {
    const cam = this.sigma.getCamera();
    const st = cam.getState();
    const t = { ...target, angle: target.angle ?? st.angle };
    if (this.quality.motion !== "full") {
      cam.setState(t);
      return;
    }
    const dist = Math.hypot(t.x - st.x, t.y - st.y);
    const span = Math.max(st.ratio, t.ratio);
    if (dist > span * 0.9) {
      const mid = { x: (st.x + t.x) / 2, y: (st.y + t.y) / 2, ratio: Math.min(1.2, Math.max(span * 1.8, dist * 1.1)), angle: t.angle };
      void cam.animate(mid, { duration: ms * 0.45, easing: "quadraticOut" }).then(() => cam.animate(t, { duration: ms * 0.75, easing: "cubicInOut" }));
    } else {
      void cam.animate(t, { duration: ms, easing: "cubicInOut" });
    }
  }

  focus(key: string): void {
    if (!this.graph.hasNode(key)) return;
    const d = this.sigma.getNodeDisplayData(key);
    if (!d) return;
    const r = this.pixelRadius(key);
    const cam = this.sigma.getCamera();
    // Zoom so the node is drawn about 34 px wide (radius), but never zoom out to do it.
    const target = r > 0 ? Math.min(cam.ratio, Math.max(0.0003, cam.ratio * (r / 34))) : 0.2;
    this.move({ x: d.x, y: d.y, ratio: Math.min(target, 1) }, 480);
  }

  pixelRadius(key: string): number {
    if (!this.graph.hasNode(key)) return 0;
    const a = this.graph.getNodeAttributes(key);
    const x = a.x as number, y = a.y as number, r = a.size as number;
    const p = this.sigma.graphToViewport({ x, y });
    const q = this.sigma.graphToViewport({ x: x + r, y });
    return Math.hypot(q.x - p.x, q.y - p.y);
  }

  onScreen(key: string, margin = 60): boolean {
    if (!this.graph.hasNode(key)) return false;
    const a = this.graph.getNodeAttributes(key);
    const p = this.sigma.graphToViewport({ x: a.x as number, y: a.y as number });
    const { width, height } = this.sigma.getDimensions();
    const r = this.pixelRadius(key);
    return p.x + r > -margin && p.y + r > -margin && p.x - r < width + margin && p.y - r < height + margin;
  }

  // ---------- selection, highlight ----------

  setSelected(key: string | null): void {
    this.selected = key;
    this.updateHighlight();
    this.host.classList.toggle("focused", key !== null && this.quality.focusBlur);
    this.refreshAll();
    this.animate();
  }

  getSelected(): string | null {
    return this.selected;
  }

  setHead(key: string | null): void {
    this.head = key;
  }

  setFocusOwner(owner: number): void {
    this.focusOwner = owner;
    this.refreshAll();
  }

  private updateHighlight(): void {
    this.highlight.clear();
    this.chain.clear();
    const center = this.selected ?? this.hovered;
    if (!center || !this.graph.hasNode(center)) return;
    this.highlight.add(center);
    this.chain.add(center);
    for (let k: string | undefined = this.graph.inNeighbors(center)[0]; k; k = this.graph.inNeighbors(k)[0]) {
      this.highlight.add(k);
      this.chain.add(k);
    }
    for (const k of this.graph.outNeighbors(center)) this.highlight.add(k);
  }

  /** Is a node part of what the selection (or hover) is about? */
  private related(key: string): boolean {
    return this.highlight.size === 0 || this.highlight.has(key);
  }

  private reduce(key: string, a: Attrs, layer: "front" | "back"): Attrs {
    const res: Attrs = { ...a };
    const selecting = this.selected !== null && this.highlight.size > 0;
    const blur = selecting && this.quality.focusBlur;
    const related = this.related(key);
    const ownerOff = this.focusOwner >= 0 && this.tree !== null && ((a.node as number) < 0 || this.tree.owner[a.node as number] !== this.focusOwner);

    if (layer === "back") {
      // The back layer only draws the unrelated part, and only while something is selected.
      if (!blur || related) return { ...res, hidden: true, label: null };
      return { ...res, ...dimLook(a as unknown as Look, this.pal, 0.55), label: null, size: this.sizeNow(key, a) };
    }
    if (blur && !related) return { ...res, hidden: true, label: null };
    res.size = this.sizeNow(key, a);
    if ((!related && !blur) || ownerOff) {
      Object.assign(res, dimLook(a as unknown as Look, this.pal, this.selected ? 0.82 : 0.65));
      res.label = null;
      res.zIndex = 0;
    } else if (this.highlight.has(key)) {
      res.zIndex = 3;
      if (this.chain.has(key)) res.forceLabel = true;
    }
    if (key === this.selected) {
      res.ringColor = this.pal.accent;
      res.ringSize = Math.max(0.24, Number(a.ringSize) || 0);
      res.zIndex = 4;
    } else if (key === this.hovered || key === this.dragging) {
      res.ringColor = mix(this.pal.accent, String(a.ringColor), 0.3);
      res.ringSize = Math.max(0.2, Number(a.ringSize) || 0);
    }
    if (key === this.head) res.ringColor = this.pal.accent;
    return res;
  }

  private sizeNow(key: string, a: Attrs): number {
    let size = Math.max(a.size as number, MIN_PX / this.unitPx);
    const born = this.born.get(key);
    const now = performance.now();
    if (born !== undefined) {
      const t = Math.min(1, (now - born) / GROW_MS);
      size *= 0.1 + 0.9 * (1 - (1 - t) * (1 - t));
    }
    const leave = this.leaving.get(key);
    if (leave) size *= Math.max(0.05, 1 - (now - leave.t) / LEAVE_MS);
    return size;
  }

  private reduceEdge(key: string, attrs: Attrs, layer: "front" | "back"): Attrs {
    const g = this.graph;
    const px = 1 / this.unitPx;
    const s = g.source(key);
    const t = g.target(key);
    const curved = this.quality.curved && this.style.curved;
    const a: Attrs = { ...attrs, type: curved ? "curved" : "line", color: rgba(this.pal.edge, this.style.edgeAlpha * 1.6 > 1 ? 1 : this.style.edgeAlpha * 1.6), size: (attrs.size as number) * px };
    const selecting = this.selected !== null && this.highlight.size > 0;
    const blur = selecting && this.quality.focusBlur;
    const both = this.related(s) && this.related(t);
    if (layer === "back") {
      if (!blur || both) return { ...a, hidden: true };
      return { ...a, color: mix(this.pal.edge, this.pal.bg, 0.4) };
    }
    if (blur && !both) return { ...a, hidden: true };
    if (this.highlight.size > 0) {
      if (!both) return { ...a, color: mix(this.pal.edge, this.pal.bg, 0.6) };
      const onChain = this.chain.has(s) && this.chain.has(t);
      return { ...a, color: onChain ? this.pal.accent : mix(this.pal.accent, this.pal.edge, 0.55), size: (onChain ? 1.8 : 1.2) * px, zIndex: 2 };
    }
    const born = this.fresh.get(key);
    if (born !== undefined) {
      const k = Math.min(1, (performance.now() - born) / FRESH_MS);
      return { ...a, color: mix(this.pal.accent, this.pal.edge, k * k), size: (1.8 - 0.8 * k) * px, zIndex: 2 };
    }
    return a;
  }

  // ---------- labels, glow, pulses ----------

  private drawLabel(ctx: CanvasRenderingContext2D, d: Attrs, s: { labelSize: number; labelFont: string }): void {
    const label = d.label as string | null;
    if (!label) return;
    const weight = (d.labelWeight as number) || 500;
    const size = weight >= 700 ? s.labelSize + 2 : s.labelSize;
    ctx.font = `${weight} ${size}px ${s.labelFont}`;
    const x = (d.x as number) + (d.size as number) + 5;
    const y = (d.y as number) + size / 3;
    ctx.lineJoin = "round";
    ctx.lineWidth = 3.5;
    ctx.strokeStyle = rgba(this.pal.bg, 0.85);
    ctx.strokeText(label, x, y);
    ctx.fillStyle = (d.labelColor as string) || this.pal.text;
    ctx.fillText(label, x, y);
  }

  private sizeFx(): void {
    const r = this.host.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.fx.width = Math.max(1, Math.round(r.width * dpr));
    this.fx.height = Math.max(1, Math.round(r.height * dpr));
    this.fx.style.width = `${r.width}px`;
    this.fx.style.height = `${r.height}px`;
    this.drawFx();
  }

  private drawFx(): void {
    const ctx = this.fx.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, this.fx.width, this.fx.height);
    ctx.globalCompositeOperation = "lighter";
    const g = this.graph;
    const strength = this.style.glow;
    if (this.quality.glow && strength > 0) {
      const keys = new Set(this.glowKeys);
      if (this.selected) keys.add(this.selected);
      if (this.head) keys.add(this.head);
      for (const k of keys) {
        if (!g.hasNode(k)) continue;
        const a = g.getNodeAttributes(k);
        if (this.selected && !this.related(k)) continue;
        const p = this.sigma.graphToViewport({ x: a.x as number, y: a.y as number });
        const r = this.pixelRadius(k);
        const reach = Math.max(18, r * 3.2);
        const c = k === this.selected || k === this.head ? this.pal.accent : mix(String(a.ringColor ?? a.color), this.pal.glow, 0.5);
        const grad = ctx.createRadialGradient(p.x, p.y, r * 0.6, p.x, p.y, reach);
        grad.addColorStop(0, rgba(c, 0.22 * strength));
        grad.addColorStop(1, rgba(c, 0));
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(p.x, p.y, reach, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    if (this.quality.pulses && this.quality.motion === "full" && this.fresh.size > 0) {
      const now = performance.now();
      for (const [e, t0] of this.fresh) {
        const t = (now - t0) / PULSE_MS;
        if (t > 1 || !g.hasEdge(e)) continue;
        const s = g.getNodeAttributes(g.source(e));
        const d = g.getNodeAttributes(g.target(e));
        const a = this.sigma.graphToViewport({ x: s.x as number, y: s.y as number });
        const b = this.sigma.graphToViewport({ x: d.x as number, y: d.y as number });
        const k = 1 - (1 - t) * (1 - t);
        const x = a.x + (b.x - a.x) * k;
        const y = a.y + (b.y - a.y) * k;
        const grad = ctx.createRadialGradient(x, y, 0, x, y, 7);
        grad.addColorStop(0, rgba(this.pal.accent, 0.9 * (1 - t * 0.5)));
        grad.addColorStop(1, rgba(this.pal.accent, 0));
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(x, y, 7, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    ctx.globalCompositeOperation = "source-over";
  }

  private animate(): void {
    if (this.animating) return;
    this.animating = true;
    const step = (): void => {
      const now = performance.now();
      for (const [k, t] of this.born) if (now - t > GROW_MS) this.born.delete(k);
      for (const [k, t] of this.fresh) if (now - t > FRESH_MS || !this.graph.hasEdge(k)) this.fresh.delete(k);
      for (const [k, l] of this.leaving) {
        const t = Math.min(1, (now - l.t) / LEAVE_MS);
        if (t >= 1 || !this.graph.hasNode(l.to)) {
          this.leaving.delete(k);
          if (this.graph.hasNode(k)) this.graph.dropNode(k);
          continue;
        }
        const to = this.graph.getNodeAttributes(l.to);
        const e = t * t;
        this.graph.mergeNodeAttributes(k, { x: l.x + ((to.x as number) - l.x) * e, y: l.y + ((to.y as number) - l.y) * e });
      }
      if (!this.frozen && this.layout.running() && now - this.lastFit > 400) {
        this.lastFit = now;
        this.fitBox();
      }
      const busy = this.born.size > 0 || this.fresh.size > 0 || this.leaving.size > 0;
      if (busy) this.refreshAll();
      if (busy || this.layout.running()) requestAnimationFrame(step);
      else {
        this.animating = false;
        this.drawFx();
      }
    };
    requestAnimationFrame(step);
  }

  // ---------- events ----------

  private bindEvents(): void {
    const s = this.sigma;
    s.on("beforeRender", () => {
      const p = s.graphToViewport({ x: 0, y: 0 });
      const q = s.graphToViewport({ x: 1, y: 0 });
      this.unitPx = Math.max(1e-6, Math.hypot(q.x - p.x, q.y - p.y));
    });
    s.on("afterRender", () => this.drawFx());
    s.on("enterNode", ({ node, event }) => {
      this.hovered = node;
      if (!this.selected) this.updateHighlight();
      this.refreshAll();
      this.on.hover(node, event.x, event.y);
      s.getContainer().style.cursor = "pointer";
    });
    s.on("leaveNode", () => {
      this.hovered = null;
      if (!this.selected) this.updateHighlight();
      this.refreshAll();
      this.on.hover(null, 0, 0);
      s.getContainer().style.cursor = "";
    });
    s.on("clickNode", ({ node }) => {
      if (this.dragMoved) return;
      this.on.select(node);
    });
    s.on("doubleClickNode", (e) => {
      e.preventSigmaDefault();
      this.on.toggle(e.node);
    });
    s.on("rightClickNode", (e) => {
      e.preventSigmaDefault();
      e.event.original.preventDefault();
      this.on.context(e.node, e.event.x, e.event.y);
    });
    s.on("clickStage", () => this.on.select(null));
    s.on("downNode", (e) => {
      this.dragging = e.node;
      this.dragMoved = false;
    });
    s.getMouseCaptor().on("mousemovebody", (e) => {
      if (!this.dragging) return;
      const pos = s.viewportToGraph(e);
      this.graph.mergeNodeAttributes(this.dragging, { x: pos.x, y: pos.y, fixed: true });
      this.layout.hold(this.dragging, pos.x, pos.y);
      this.dragMoved = true;
      e.preventSigmaDefault();
      e.original.preventDefault();
      e.original.stopPropagation();
    });
    const drop = (): void => {
      if (!this.dragging) return;
      const moved = this.dragMoved;
      const key = this.dragging;
      this.dragging = null;
      this.layout.release(key, moved);
      if (moved) this.animate();
      setTimeout(() => (this.dragMoved = false), 0);
    };
    s.getMouseCaptor().on("mouseup", drop);
    s.on("upNode", drop);
    s.getCamera().on("updated", (state) => {
      this.back.getCamera().setState(state);
      // Throttled, not debounced: folders open while you are still zooming.
      if (!this.cameraTimer) {
        this.cameraTimer = window.setTimeout(() => {
          this.cameraTimer = 0;
          this.on.cameraMoved();
        }, 160);
      }
    });
  }

  /** Pixels per graph unit at the current zoom. */
  pixelsPerUnit(): number {
    return this.unitPx;
  }

  /** Draw a small overview of the whole graph and the part on screen. */
  drawMinimap(c: HTMLCanvasElement): void {
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth, hgt = c.clientHeight;
    if (c.width !== Math.round(w * dpr)) { c.width = Math.round(w * dpr); c.height = Math.round(hgt * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, hgt);
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    this.graph.forEachNode((_k, a) => {
      const x = a.x as number, y = a.y as number;
      if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y;
    });
    if (!Number.isFinite(x0)) return;
    const pad = 6;
    const sc = Math.min((w - pad * 2) / Math.max(1, x1 - x0), (hgt - pad * 2) / Math.max(1, y1 - y0));
    const ox = (w - (x1 - x0) * sc) / 2, oy = (hgt - (y1 - y0) * sc) / 2;
    // Graph y grows upward on screen in sigma; flip to match the main view.
    const map = (x: number, y: number): [number, number] => [ox + (x - x0) * sc, hgt - (oy + (y - y0) * sc)];
    this.graph.forEachNode((_k, a) => {
      const [mx, my] = map(a.x as number, a.y as number);
      ctx.fillStyle = String(a.ringSize && (a.ringSize as number) > 0.25 ? a.ringColor : a.color);
      const r = Math.max(0.8, Math.min(4, (a.size as number) * sc));
      ctx.beginPath();
      ctx.arc(mx, my, r, 0, Math.PI * 2);
      ctx.fill();
    });
    const { width, height } = this.sigma.getDimensions();
    const a = this.sigma.viewportToGraph({ x: 0, y: 0 });
    const b = this.sigma.viewportToGraph({ x: width, y: height });
    const [ax, ay] = map(a.x, a.y);
    const [bx, by] = map(b.x, b.y);
    ctx.strokeStyle = this.pal.accent;
    ctx.lineWidth = 1.5;
    ctx.strokeRect(Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay));
    this.minimapMap = { x0, y0, sc, ox, oy, h: hgt };
  }

  private minimapMap: { x0: number; y0: number; sc: number; ox: number; oy: number; h: number } | null = null;

  /** Centre the view on a point of the minimap. */
  minimapGo(mx: number, my: number): void {
    const m = this.minimapMap;
    if (!m) return;
    const gx = m.x0 + (mx - m.ox) / m.sc;
    const gy = m.y0 + (m.h - my - m.oy) / m.sc;
    const vp = this.sigma.graphToViewport({ x: gx, y: gy });
    const fr = this.sigma.viewportToFramedGraph(vp);
    this.move({ x: fr.x, y: fr.y, ratio: this.sigma.getCamera().ratio }, 380);
  }

  isPinned(key: string): boolean {
    return this.graph.hasNode(key) && Boolean(this.graph.getNodeAttribute(key, "fixed"));
  }

  unpin(key: string): void {
    if (!this.graph.hasNode(key)) return;
    this.graph.removeNodeAttribute(key, "fixed");
    this.layout.unpin(key);
    this.animate();
  }

  /** A picture of the graph as it is on screen (PNG data URL), for "Save picture". */
  snapshot(): string {
    const r = this.host.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const out = document.createElement("canvas");
    out.width = Math.round(r.width * dpr);
    out.height = Math.round(r.height * dpr);
    const ctx = out.getContext("2d");
    if (!ctx) return "";
    const grad = ctx.createRadialGradient(out.width / 2, out.height / 2, 0, out.width / 2, out.height / 2, Math.max(out.width, out.height) * 0.7);
    grad.addColorStop(0, this.pal.bg);
    grad.addColorStop(1, mix(this.pal.bg, this.pal.bgEdge, this.style.vignette));
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, out.width, out.height);
    for (const layer of [this.back, null, this.sigma] as const) {
      const canvases = layer ? Object.values(layer.getCanvases()) : [this.fx];
      for (const c of canvases) ctx.drawImage(c, 0, 0, out.width, out.height);
    }
    return out.toDataURL("image/png");
  }

  reset(): void {
    this.layout.stop();
    for (const k of this.leaving.keys()) if (this.graph.hasNode(k)) this.graph.dropNode(k);
    this.leaving.clear();
    this.graph.clear();
    this.born.clear();
    this.fresh.clear();
    this.selected = null;
    this.hovered = null;
    this.highlight.clear();
    this.chain.clear();
    this.frozen = false;
    this.stable = false;
    this.layout = new ForceLayout(this.graph);
    this.host.classList.remove("focused");
    void this.sigma.getCamera().animate({ x: 0.5, y: 0.5, ratio: 1, angle: 0 }, { duration: 1 });
  }
}

// Visual styles ("looks"). A style is a palette plus a few drawing choices. The owner picks one;
// the others stay available in Settings. Colours are chosen per style, never generic defaults.

import type { RiskId } from "../core/format";

export type StyleId = "deep" | "instrument" | "mycelium";

export interface Palette {
  /** Graph background, centre and edge of the vignette. */
  bg: string;
  bgEdge: string;
  /** Panels and controls. */
  panel: string;
  panel2: string;
  line: string;
  text: string;
  text2: string;
  /** Links between nodes. */
  edge: string;
  /** Selection, focus, the scan head and pulses. */
  accent: string;
  folder: string;
  file: string;
  link: string;
  cloud: string;
  group: string;
  risk: Record<RiskId, string>;
  /** File categories, in dc-core order (video, image, audio, archive, code, document, game, installer, executable, temporary, other). */
  cats: string[];
  apps: string[];
  windows: string;
  noOwner: string;
  glow: string;
}

export interface Style {
  id: StyleId;
  name: string;
  summary: string;
  curved: boolean;
  curvature: number;
  edgeAlpha: number;
  glow: number;
  vignette: number;
  dark: Palette;
  light: Palette;
}

// Risk colours are from the Okabe-Ito colour-blind-safe set, shifted for each background; risk is
// also always drawn as a shape (see encode.ts), never colour alone.
const RISK_DARK: Record<RiskId, string> = { safe: "#2cc5a0", probably_safe: "#e8a93a", dont_touch: "#f2743a" };
const RISK_LIGHT: Record<RiskId, string> = { safe: "#00876a", probably_safe: "#a86e00", dont_touch: "#c2470a" };
const CATS_DARK = ["#e7708f", "#5cc28f", "#45bccd", "#e7a259", "#7895ee", "#dcc663", "#a884e6", "#c0907a", "#93a1b4", "#69737f", "#aab4c1"];
const CATS_LIGHT = ["#c2456a", "#2f9363", "#16879a", "#b8742a", "#4565c7", "#9c8418", "#7a55c2", "#93624b", "#5f6d80", "#7f8893", "#56606c"];
const APPS_DARK = ["#6f9cf0", "#ec7aa4", "#4fc8b6", "#f0a862", "#a487ee", "#86cc63", "#e8cb52", "#58bce9", "#e0806a", "#bcc962", "#cf88e2", "#62d6d6"];
const APPS_LIGHT = ["#3d6fd1", "#c64a7e", "#18927f", "#c2752c", "#7356c9", "#4f8f2c", "#a58716", "#1f86b8", "#b8533c", "#7d8a24", "#9a52b0", "#1d8f8f"];

const LIGHT_BASE: Omit<Palette, "accent" | "folder" | "file" | "edge" | "glow"> = {
  bg: "#e9eeec", bgEdge: "#dde4e1", panel: "#f4f7f5", panel2: "#e6ece9", line: "#c7d1cc", text: "#18222a", text2: "#55626b",
  link: "#7a5cc4", cloud: "#2b8fc4", group: "#b5c0bb", risk: RISK_LIGHT, cats: CATS_LIGHT, apps: APPS_LIGHT, windows: "#6f8095", noOwner: "#a3aeb3",
};

export const STYLES: Record<StyleId, Style> = {
  deep: {
    id: "deep",
    name: "Deep field",
    summary: "Cinematic. Deep blue night, soft glow on the biggest folders, gently curved links.",
    curved: true, curvature: 0.18, edgeAlpha: 0.55, glow: 1, vignette: 1,
    dark: {
      bg: "#0b1322", bgEdge: "#050912", panel: "#0f1828", panel2: "#152136", line: "#22324a", text: "#dfe7f2", text2: "#8d9cb3",
      edge: "#2a3b57", accent: "#62bdf0", folder: "#6d8fbd", file: "#9aaecb", link: "#a88cf0", cloud: "#7cc6f2", group: "#33445e",
      risk: RISK_DARK, cats: CATS_DARK, apps: APPS_DARK, windows: "#7f93b3", noOwner: "#3c4b63", glow: "#8fc7ff",
    },
    light: { ...LIGHT_BASE, edge: "#c3cfd8", accent: "#1f7fb8", folder: "#5a7698", file: "#8798ae", glow: "#7fb2e0" },
  },
  instrument: {
    id: "instrument",
    name: "Instrument",
    summary: "Precise. Graphite, crisp rings, straight hairline links, no glow.",
    curved: false, curvature: 0, edgeAlpha: 0.7, glow: 0, vignette: 0.35,
    dark: {
      bg: "#15191d", bgEdge: "#101316", panel: "#1a1f24", panel2: "#20262c", line: "#2c343c", text: "#e3e7ea", text2: "#919ba4",
      edge: "#323b44", accent: "#5ab4dd", folder: "#a3adb7", file: "#6f7a85", link: "#a88cf0", cloud: "#7cc6f2", group: "#3a434c",
      risk: RISK_DARK, cats: CATS_DARK, apps: APPS_DARK, windows: "#8d99a6", noOwner: "#454e57", glow: "#ffffff",
    },
    light: { ...LIGHT_BASE, edge: "#bcc6c2", accent: "#1f7fb8", folder: "#4f5b66", file: "#8a949d", glow: "#ffffff" },
  },
  mycelium: {
    id: "mycelium",
    name: "Mycelium",
    summary: "Organic. Moss-dark ground, bone-coloured threads that curve like roots, a faint warm glow.",
    curved: true, curvature: 0.32, edgeAlpha: 0.5, glow: 0.6, vignette: 0.9,
    dark: {
      bg: "#0f1512", bgEdge: "#080c0a", panel: "#131a16", panel2: "#19221d", line: "#28342d", text: "#e6e3d6", text2: "#9a9d8c",
      edge: "#3b4836", accent: "#8fd6c4", folder: "#cbbf98", file: "#8f957a", link: "#b49ae6", cloud: "#8fc9e0", group: "#323d33",
      risk: RISK_DARK, cats: CATS_DARK, apps: APPS_DARK, windows: "#a7a58c", noOwner: "#3b453c", glow: "#efe2b0",
    },
    light: { ...LIGHT_BASE, bg: "#ebede6", bgEdge: "#dfe2d8", edge: "#c9c9b6", accent: "#2b8a78", folder: "#7a6f4c", file: "#9a9a84", glow: "#d9c98a" },
  },
};

export function paletteOf(style: Style, dark: boolean): Palette {
  return dark ? style.dark : style.light;
}

/** Mix a hex colour toward another (t = 0 keeps `a`, 1 gives `b`). */
export function mix(a: string, b: string, t: number): string {
  const pa = parseHex(a);
  const pb = parseHex(b);
  if (!pa || !pb) return a;
  const c = pa.map((x, i) => Math.round(x + (pb[i] - x) * t));
  return `#${c.map((x) => x.toString(16).padStart(2, "0")).join("")}`;
}

export function parseHex(c: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{6})$/i.exec(c.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgba(c: string, a: number): string {
  const p = parseHex(c);
  return p ? `rgba(${p[0]},${p[1]},${p[2]},${a})` : c;
}

/** Put a style's palette into CSS variables, so panels and the graph share one set of colours. */
export function applyCss(p: Palette, style: Style): void {
  const r = document.documentElement.style;
  const set = (k: string, v: string): void => r.setProperty(k, v);
  set("--bg", p.bg); set("--bg-edge", p.bgEdge); set("--panel", p.panel); set("--panel-2", p.panel2); set("--line", p.line);
  set("--text", p.text); set("--text-2", p.text2); set("--accent", p.accent); set("--edge", p.edge);
  set("--safe", p.risk.safe); set("--probably", p.risk.probably_safe); set("--dont", p.risk.dont_touch);
  set("--folder", p.folder); set("--file", p.file);
  set("--vignette", String(style.vignette));
}

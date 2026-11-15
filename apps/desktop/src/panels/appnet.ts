// An app's network on its page: the app in the middle, a line to every place it keeps things,
// and what can be cleared hanging further out. Circles follow the space; colours follow what
// removing them would do. Clicking a circle opens it in the Files view.

import { NET_H, NET_W, type NetNode } from "../core/apps";
import { human } from "../core/util";

const NS = "http://www.w3.org/2000/svg";

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>): SVGElementTagNameMap[K] {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

const KIND_TEXT: Record<NetNode["kind"], string> = {
  app: "the app",
  program: "needed to run",
  data: "settings and data",
  saves: "your saves",
  temp: "temporary files",
  more: "more places",
  safe: "safe to clear",
  probably_safe: "probably safe to clear",
};

/** Shorten a label for the drawing. */
function short(text: string, max = 18): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

export function appNetwork(nodes: NetNode[], icon: string | undefined, open: (node: number) => void): SVGSVGElement {
  const svg = el("svg", { class: "ap-net", viewBox: `0 0 ${NET_W} ${NET_H}`, role: "img", "aria-label": "Where this app keeps things" });
  const lines = el("g", { class: "lines" });
  const circles = el("g", { class: "nodes" });
  nodes.forEach((d, i) => {
    if (i > 0 && d.from >= 0) {
      const p = nodes[d.from];
      lines.append(el("line", { x1: p.x, y1: p.y, x2: d.x, y2: d.y, class: `k-${d.kind}` }));
    }
    const g = el("g", { class: `n k-${d.kind}${d.node >= 0 ? " link" : ""}`, transform: `translate(${d.x.toFixed(1)} ${d.y.toFixed(1)})` });
    const title = el("title", {});
    title.textContent = `${d.label}: ${human(d.bytes)} (${KIND_TEXT[d.kind]})`;
    g.append(title, el("circle", { r: d.r.toFixed(1) }));
    if (d.kind === "app") {
      if (icon) g.append(el("image", { href: icon, x: -16, y: -16, width: 32, height: 32 }));
      else {
        const t = el("text", { class: "letter", "text-anchor": "middle", dy: "0.35em" });
        t.textContent = (d.label.trim()[0] ?? "?").toUpperCase();
        g.append(t);
      }
    }
    // Labels below each circle; above it for the outer circles in the top half, so they do not
    // run into their line.
    const above = d.from > 0 && d.y < NET_H / 2;
    const name = el("text", { class: "lbl", "text-anchor": "middle", y: above ? -d.r - 18 : d.r + 13 });
    name.textContent = short(d.label);
    const size = el("text", { class: "lbl sz", "text-anchor": "middle", y: above ? -d.r - 5 : d.r + 26 });
    size.textContent = human(d.bytes);
    g.append(name, size);
    if (d.node >= 0) {
      g.setAttribute("tabindex", "0");
      g.addEventListener("click", () => open(d.node));
      g.addEventListener("keydown", (e) => {
        if ((e as KeyboardEvent).key === "Enter") open(d.node);
      });
    }
    circles.append(g);
  });
  svg.append(lines, circles);
  return svg;
}

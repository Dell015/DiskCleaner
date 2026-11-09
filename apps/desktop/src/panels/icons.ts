// Small SVG pictures of node shapes, drawn with the same rules as the graph (encode.ts), so the
// key, the findings rows and the details always match what is on screen.

import type { Look, Shape } from "../graph/encode";

const NS = "http://www.w3.org/2000/svg";

/** A node drawn as concentric circles: ring, gap, fill (sizes are shares of the radius). */
export function shapeIcon(look: Pick<Look, "color" | "ringColor" | "ringSize" | "gapColor" | "gapSize">, size = 14): SVGSVGElement {
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "-1 -1 2 2");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("shape");
  let r = 0.94;
  const circle = (radius: number, fill: string): void => {
    if (radius <= 0) return;
    const c = document.createElementNS(NS, "circle");
    c.setAttribute("r", String(radius));
    c.setAttribute("fill", fill);
    svg.append(c);
  };
  circle(r, look.ringColor);
  r -= look.ringSize * 0.94;
  circle(r, look.gapColor);
  r -= look.gapSize * 0.94;
  circle(r, look.color);
  return svg;
}

export type { Shape };

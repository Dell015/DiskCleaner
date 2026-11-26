// Which of your files Rhizome can show in its preview window (D143). The same list as the
// desktop app's Rust side (`src-tauri/src/viewer.rs` `kind_of`): web pages and SVG are never
// shown, because they can hold scripts.

import { KIND } from "./format";
import type { Tree } from "./tree";

export type ViewKind = "video" | "picture" | "music" | "pdf" | "text";

const KINDS: Record<string, ViewKind> = {
  mp4: "video", m4v: "video", webm: "video", mov: "video", mkv: "video", avi: "video", wmv: "video",
  jpg: "picture", jpeg: "picture", jfif: "picture", png: "picture", gif: "picture", webp: "picture", bmp: "picture", avif: "picture", ico: "picture",
  mp3: "music", m4a: "music", aac: "music", wav: "music", ogg: "music", oga: "music", opus: "music", flac: "music",
  pdf: "pdf",
  txt: "text", md: "text", log: "text", csv: "text", json: "text", xml: "text", ini: "text", cfg: "text", yaml: "text", yml: "text", toml: "text",
};

/** What a file shows as, by its name; null: not shown. */
export function viewKind(name: string): ViewKind | null {
  const m = /\.([a-z0-9]+)$/i.exec(name.trim());
  return m ? KINDS[m[1].toLowerCase()] ?? null : null;
}

/** Files under `node` that can be shown (of `only` kinds when given), biggest first, at most `max`. */
export function viewableIn(tree: Tree, node: number, only?: ViewKind[], max = 500): number[] {
  const out: number[] = [];
  const stack = [node];
  while (stack.length && out.length < max * 4) {
    const n = stack.pop() as number;
    for (let c = tree.firstChild[n]; c >= 0; c = tree.nextSibling[c]) {
      if (tree.kind[c] === KIND.dir) stack.push(c);
      else {
        const k = viewKind(tree.names[c]);
        if (k && (!only || only.includes(k))) out.push(c);
      }
    }
  }
  return out.sort((a, b) => tree.agg[b] - tree.agg[a]).slice(0, max);
}

/** The files next to `node` (same folder) that can be shown, in name order, and where it is. */
export function viewableSiblings(tree: Tree, node: number): { list: number[]; at: number } {
  const p = tree.parent[node];
  if (p < 0) return { list: [node], at: 0 };
  const list: number[] = [];
  for (let c = tree.firstChild[p]; c >= 0; c = tree.nextSibling[c]) if (tree.kind[c] === KIND.file && viewKind(tree.names[c])) list.push(c);
  list.sort((a, b) => tree.names[a].localeCompare(tree.names[b], undefined, { numeric: true }));
  const at = Math.max(0, list.indexOf(node));
  return { list: list.length ? list : [node], at };
}

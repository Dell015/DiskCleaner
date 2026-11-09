import type { Tree } from "./tree";

export interface Hit { node: number; path: string }

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Find entries by name, or by the end of a path when the text has a backslash.
 * Biggest first. Case is ignored.
 */
export function search(tree: Tree, query: string, limit = 40): Hit[] {
  const q = query.trim().replace(/\//g, "\\");
  if (q.length === 0) return [];
  const parts = q.split("\\").filter((p) => p.length > 0);
  const last = parts[parts.length - 1] ?? "";
  if (!last) return [];
  const re = new RegExp(escape(last), "i");
  const wantPath = parts.length > 1 ? q.toLowerCase().replace(/\\+$/, "") : null;
  const found: number[] = [];
  for (let i = 1; i < tree.n; i++) {
    if (!re.test(tree.names[i])) continue;
    if (wantPath && !tree.path(i).toLowerCase().includes(wantPath)) continue;
    found.push(i);
  }
  found.sort((a, b) => tree.agg[b] - tree.agg[a] || a - b);
  return found.slice(0, limit).map((node) => ({ node, path: tree.path(node) }));
}

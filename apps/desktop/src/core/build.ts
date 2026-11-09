import type { ViewData } from "./format";
import { indexAnnotations, Tree } from "./tree";

/** An empty tree ready to receive the entries of `data` (owners and marks attached). */
export function emptyTree(data: ViewData): Tree {
  const { ownerOf, markOf } = indexAnnotations(data.assignments, data.marks);
  const t = new Tree(data.names.length, data.owners, data.marks, data.categories, ownerOf, markOf);
  t.source = { names: data.names, parent: data.parent };
  return t;
}

/** Append entries `from` (inclusive) to `to` (exclusive) of `data` to `tree`. */
export function appendRange(tree: Tree, data: ViewData, from: number, to: number): void {
  for (let i = from; i < to; i++) {
    tree.append(data.parent[i], data.names[i], data.kind[i], data.size[i], data.mtime[i], data.flags[i], data.cat[i]);
  }
}

/** The whole tree at once. */
export function buildTree(data: ViewData): Tree {
  const t = emptyTree(data);
  appendRange(t, data, 0, data.names.length);
  return t;
}

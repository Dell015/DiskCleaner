// Side column (findings, apps), the details drawer, and the key. Text-only DOM, no HTML strings.

import { FLAG, KIND, type Mark, type RiskId } from "../core/format";
import type { Tree } from "../core/tree";
import { count, date, human } from "../core/util";
import { groupLook, lookOf, SHAPE_NAMES, type ColorMode, type Look, type Shape } from "../graph/encode";
import { mix, type Palette } from "../graph/style";
import { clear, h } from "./dom";
import { pickBox, type PickHandlers } from "./explorer";
import { shapeIcon } from "./icons";

export interface PanelActions {
  reveal(node: number): void;
  toggle(node: number): void;
  focusHere(node: number): void;
  showMore(parent: number): void;
  unpin(node: number): void;
  isPinned(node: number): boolean;
  isOpen(node: number): boolean;
  focusOwner(owner: number): void;
  close(): void;
  /** Picking for removing (only when removing is available). */
  pick?: PickHandlers;
}

export type FindingFilter = "offered" | "shown" | "refused";

export const CATEGORY_LABELS = ["Video", "Pictures", "Audio", "Archives", "Code", "Documents", "Game files", "Installers", "Programs", "Temporary", "Other"];
export const RISK_LABELS: Record<RiskId, string> = { safe: "Safe", probably_safe: "Probably safe", dont_touch: "Don't touch" };

/** The look a mark gives (for icons next to findings). */
function markLook(m: Mark, p: Palette): Look {
  const shape: Shape = m.risk === "safe" ? "safe" : m.risk === "probably_safe" ? "probably" : "dont";
  if (shape === "safe") return { shape, color: p.risk.safe, ringColor: mix(p.risk.safe, p.text, 0.35), ringSize: 0.16, gapColor: p.bg, gapSize: 0, labelWeight: 600, labelColor: p.text };
  if (shape === "probably") return { shape, color: p.folder, ringColor: p.risk.probably_safe, ringSize: 0.3, gapColor: p.panel, gapSize: 0.14, labelWeight: 600, labelColor: p.text };
  return { shape, color: p.risk.dont_touch, ringColor: p.risk.dont_touch, ringSize: 0.3, gapColor: p.panel, gapSize: 0.2, labelWeight: 600, labelColor: p.text };
}

function share(f: number): string {
  if (f <= 0) return "0%";
  if (f < 0.0001) return "under 0.01%";
  return `${(f * 100).toFixed(f < 0.01 ? 2 : 1)}%`;
}

function evidenceList(m: Mark): HTMLElement | null {
  if (m.evidence.length === 0) return null;
  const max = Math.max(100, m.evidence.reduce((a, e) => a + e.points, 0));
  const ul = h("ul", { class: "evidence" });
  for (const e of m.evidence) {
    const bar = h("span", { class: "pts-bar" });
    bar.style.width = `${Math.max(4, (e.points / max) * 100)}%`;
    ul.append(h("li", {}, h("span", { class: "pts" }, `+${e.points}`, bar), h("span", { class: "why" }, e.text)));
  }
  return ul;
}

/** Findings as rows: shape, title, size; the path underneath. */
export function renderFindings(el: HTMLElement, tree: Tree, p: Palette, filter: FindingFilter, shown: number, act: PanelActions, setFilter: (f: FindingFilter) => void, more: () => void): void {
  clear(el);
  if (tree.marks.length === 0) {
    el.append(
      h("p", { class: "lead" }, "This file has no findings."),
      h("p", { class: "quiet" }, "Findings come from the cleanup rules and the leftover check. Make the view file on Windows with your apps snapshot:"),
      h("code", { class: "cmd" }, "dc view-data scan.json --apps apps.json --out view.json"),
    );
    return;
  }
  const all = tree.marks.map((m, idx) => ({ m, idx }));
  const groups: Record<FindingFilter, { m: Mark; idx: number }[]> = {
    offered: all.filter((x) => x.m.offered),
    shown: all.filter((x) => !x.m.offered && x.m.source !== "refused"),
    refused: all.filter((x) => x.m.source === "refused"),
  };
  const sum = (l: { m: Mark }[]): number => l.reduce((a, x) => a + x.m.bytes, 0);
  const tabs = h("div", { class: "subtabs", attrs: { role: "tablist" } });
  for (const [f, label] of [["offered", "Offered"], ["shown", "Shown only"], ["refused", "Refused"]] as [FindingFilter, string][]) {
    const l = groups[f];
    tabs.append(h("button", { class: filter === f ? "on" : "", onclick: () => setFilter(f), attrs: { role: "tab", "aria-selected": String(filter === f) } },
      h("span", { class: "t" }, label), h("span", { class: "n" }, f === "refused" ? `${count(l.length)} folders` : `${human(sum(l))} in ${count(l.length)}`)));
  }
  el.append(tabs);
  el.append(h("p", { class: "quiet" },
    filter === "offered" ? "Suggestions with evidence. Look at each one before removing anything; this view never deletes." :
      filter === "shown" ? "Found, but not offered: the evidence is weak, or it may hold your data or installed games." :
        "Matched, but refused because of what is inside or where it really is."));
  const list = groups[filter].slice().sort((a, b) => b.m.bytes - a.m.bytes || a.idx - b.idx);
  if (list.length === 0) {
    el.append(h("p", { class: "quiet" }, "Nothing here."));
    return;
  }
  const ol = h("ol", { class: "rows" });
  for (const { m } of list.slice(0, shown)) {
    const box = m.source === "refused" ? null : pickBox(act.pick, m.node, m.title);
    ol.append(h("li", { class: box ? "pickable" : "" }, box, h("button", { class: "row", onclick: () => act.reveal(m.node), attrs: { "data-node": String(m.node) } },
      shapeIcon(markLook(m, p), 14),
      h("span", { class: "title" }, m.title),
      h("span", { class: "size" }, m.source === "refused" ? "" : human(m.bytes)),
      h("span", { class: "path" }, tree.path(m.node)),
    )));
  }
  el.append(ol);
  if (list.length > shown) el.append(h("button", { class: "more", onclick: more }, `Show ${count(Math.min(60, list.length - shown))} more`));
}

/** The details drawer for one entry (or a "more items" group). */
export function renderDetails(el: HTMLElement, tree: Tree, p: Palette, mode: ColorMode, rootNode: number, node: number | null, group: { parent: number; count: number; bytes: number } | null, act: PanelActions): void {
  clear(el);
  const close = h("button", { class: "close", onclick: () => act.close(), title: "Close (Esc)", attrs: { "aria-label": "Close details" } }, "×");
  if (group) {
    el.append(
      h("header", { class: "d-head" }, shapeIcon(groupLook(p), 22), h("div", {}, h("h2", {}, `${count(group.count)} more items`), h("div", { class: "path" }, tree.path(group.parent))), close),
      h("div", { class: "big" }, human(group.bytes)),
      h("p", { class: "quiet" }, "Each is too small to get its own node at this level of detail. Zoom in on the folder, or show them all here."),
      h("div", { class: "actions" }, h("button", { class: "primary", onclick: () => act.showMore(group.parent) }, "Show them")),
    );
    return;
  }
  if (node === null) return;
  const i = node;
  const isDir = tree.kind[i] === KIND.dir;
  const look = lookOf(tree, i, act.isOpen(i), i === rootNode, mode, p);
  const name = i === 0 ? "This PC" : tree.names[i].replace(/[\\/]+$/, "") || tree.names[i];
  const crumbs = h("nav", { class: "crumbs", attrs: { "aria-label": "Folders above" } });
  for (const a of tree.ancestors(i)) {
    if (a === 0) continue;
    crumbs.append(h("button", { onclick: () => act.reveal(a) }, tree.names[a].replace(/[\\/]+$/, "") || tree.names[a]));
  }
  el.append(h("header", { class: "d-head" }, shapeIcon(look, 22), h("div", {}, h("h2", {}, name), crumbs), close));

  const total = tree.agg[0] || 1;
  const bar = h("div", { class: "share" }, h("span", {}));
  (bar.firstElementChild as HTMLElement).style.width = `${Math.max(0.5, (tree.agg[i] / total) * 100)}%`;
  el.append(h("div", { class: "big" }, human(tree.agg[i])), bar, h("div", { class: "quiet" }, `${share(tree.agg[i] / total)} of everything scanned`));

  const f = tree.flags[i];
  const notes = [
    f & FLAG.link ? "a link or junction; never followed" : "",
    f & FLAG.cloud ? "a cloud placeholder; not stored on this disk" : "",
    f & FLAG.hardlink ? "has more than one name (hard link)" : "",
    f & FLAG.notCounted ? "a second name of a hard link; counted once" : "",
    f & FLAG.system ? "marked as a system file" : "",
    f & FLAG.hidden ? "hidden" : "",
    f & FLAG.compressed ? "compressed by Windows" : "",
  ].filter(Boolean);
  const owner = tree.owner[i];
  const facts = h("dl", { class: "facts" },
    h("dt", {}, "Path"), h("dd", { class: "path" }, i === 0 ? `${tree.childCount[0]} scanned location(s)` : tree.path(i)),
    h("dt", {}, "Kind"), h("dd", {}, SHAPE_NAMES[look.shape].split(" (")[0] + (!isDir && tree.cat[i] >= 0 ? `, ${CATEGORY_LABELS[tree.cat[i]]?.toLowerCase() ?? "other"}` : "")),
    isDir ? h("dt", {}, "Inside") : null, isDir ? h("dd", {}, `${count(tree.files[i])} files in ${count(tree.childCount[i])} items`) : null,
    h("dt", {}, isDir ? "Newest change" : "Changed"), h("dd", {}, date(isDir ? tree.newest[i] : tree.mtime[i])),
    h("dt", {}, "Belongs to"), h("dd", {}, owner >= 0 ? `${tree.owners[owner].name}${tree.owners[owner].publisher ? `, ${tree.owners[owner].publisher}` : ""}` : "No app claims it"),
    notes.length ? h("dt", {}, "Notes") : null, notes.length ? h("dd", {}, notes.join("; ")) : null,
  );
  el.append(facts);

  // What the checks say: its own mark, or the riskier one from a folder above.
  const mi = tree.mark[i];
  if (mi >= 0) {
    const m = tree.marks[mi];
    const here = tree.markHere[i] === mi;
    el.append(h("section", { class: `verdict v-${m.risk}` },
      h("div", { class: "v-top" }, shapeIcon(markLook(m, p), 16), h("b", {}, RISK_LABELS[m.risk]), m.confidence ? h("span", { class: "quiet" }, `${m.confidence} confidence`) : null),
      h("div", { class: "v-title" }, m.title),
      here ? null : h("div", { class: "quiet" }, "Because it is inside ", h("button", { class: "linkish", onclick: () => act.reveal(m.node) }, tree.path(m.node))),
      evidenceList(m),
      m.detail ? h("p", { class: "quiet" }, m.detail) : null,
      h("p", { class: "v-note" }, m.offered ? "Offered as a suggestion. Look before you remove anything." : m.source === "refused" ? "Refused, never offered." : "Shown only, never offered."),
    ));
  } else if (i !== rootNode && i !== 0) {
    el.append(h("section", { class: "verdict v-none" }, h("div", { class: "v-top" }, h("b", {}, "No verdict")), h("p", { class: "quiet" }, "No rule or leftover check says anything about this. Unknown means hands off.")));
  }

  const actions = h("div", { class: "actions" });
  if (isDir && tree.childCount[i] > 0) {
    actions.append(h("button", { class: "primary", onclick: () => act.toggle(i) }, act.isOpen(i) ? "Close folder" : "Open folder"));
    if (i !== rootNode) actions.append(h("button", { onclick: () => act.focusHere(i) }, "Focus here"));
  }
  if (act.isPinned(i)) actions.append(h("button", { onclick: () => act.unpin(i) }, "Unpin"));
  const copy = h("button", {}, "Copy path");
  copy.addEventListener("click", () => {
    void navigator.clipboard?.writeText(tree.path(i)).then(() => (copy.textContent = "Copied"), () => (copy.textContent = "Could not copy"));
  });
  actions.append(copy);
  el.append(actions);

  if (isDir && tree.childCount[i] > 0) {
    const { top } = tree.topChildren(i, 12);
    const max = Math.max(1, tree.agg[top[0]] ?? 1);
    const ol = h("ol", { class: "rows inside" });
    for (const c of top) {
      const b = h("span", { class: "minibar" });
      b.style.width = `${Math.max(1, (tree.agg[c] / max) * 100)}%`;
      ol.append(h("li", {}, h("button", { class: "row", onclick: () => act.reveal(c) },
        shapeIcon(lookOf(tree, c, false, false, mode, p), 12),
        h("span", { class: "title" }, tree.names[c]),
        h("span", { class: "size" }, human(tree.agg[c])),
        b)));
    }
    el.append(h("h3", {}, "Biggest inside"), ol);
  }
}

/** Space per app; a folder counts once for the app that owns it. */
export function appTotals(tree: Tree): { owner: number; bytes: number; folders: number[] }[] {
  const per = new Map<number, { bytes: number; folders: number[] }>();
  for (let i = 0; i < tree.n; i++) {
    const o = tree.ownerHere[i];
    if (o < 0) continue;
    const above = tree.parent[i] >= 0 ? tree.owner[tree.parent[i]] : -1;
    if (above === o) continue;
    const e = per.get(o) ?? { bytes: 0, folders: [] };
    e.bytes += tree.agg[i];
    e.folders.push(i);
    per.set(o, e);
  }
  return [...per.entries()].map(([owner, v]) => ({ owner, ...v, folders: v.folders.sort((a, b) => tree.agg[b] - tree.agg[a]) })).sort((a, b) => b.bytes - a.bytes);
}

/** What kind of place a folder is, in words (where an app keeps things). */
export function placeKind(path: string): string {
  const k = path.toLowerCase().replace(/\//g, "\\");
  if (/^[a-z]:\\[^\\]+$/.test(k)) return "Top of the drive";
  if (/\\program files( \(x86\))?\\/.test(k)) return "Program files";
  if (/\\appdata\\(local|roaming|locallow)\\/.test(k)) return k.includes("\\appdata\\local\\temp\\") ? "Temporary files" : "App data";
  if (/^[a-z]:\\programdata\\/.test(k)) return "Shared app data";
  if (/\\(documents\\my games|saved games)\\/.test(k)) return "Game saves";
  if (/\\documents\\/.test(k)) return "Documents";
  return "Elsewhere";
}

export function renderApps(el: HTMLElement, tree: Tree, p: Palette, focused: number, act: PanelActions): void {
  clear(el);
  const totals = appTotals(tree);
  if (totals.length === 0) {
    el.append(h("p", { class: "lead" }, "No app information in this file."), h("p", { class: "quiet" }, "Make the view file with --apps to see which app owns what."));
    return;
  }
  el.append(h("p", { class: "quiet" }, "Space taken by each app's folders. Pick one to see every place it keeps things and light up only those."));
  if (focused >= 0) el.append(h("button", { class: "more", onclick: () => act.focusOwner(-1) }, "Show every app"));
  const max = Math.max(1, totals[0].bytes);
  const ol = h("ol", { class: "rows" });
  for (const t of totals.slice(0, 200)) {
    const o = tree.owners[t.owner];
    const color = o.kind === "windows" ? p.windows : p.apps[t.owner % p.apps.length];
    const b = h("span", { class: "minibar" });
    b.style.width = `${Math.max(1, (t.bytes / max) * 100)}%`;
    b.style.background = color;
    ol.append(h("li", { class: focused === t.owner ? "on" : "" }, h("button", { class: "row", attrs: { "aria-expanded": String(focused === t.owner) }, onclick: () => act.focusOwner(focused === t.owner ? -1 : t.owner) },
      shapeIcon({ color, ringColor: mix(color, p.text, 0.35), ringSize: 0.16, gapColor: p.bg, gapSize: 0 }, 12),
      h("span", { class: "title" }, o.name),
      h("span", { class: "size" }, human(t.bytes)),
      h("span", { class: "path" }, t.folders.length > 1 ? `${count(t.folders.length)} places: ${[...new Set(t.folders.map((f) => placeKind(tree.path(f)).toLowerCase()))].join(", ")}` : tree.path(t.folders[0])),
      b)));
    // The picked app: every place it keeps things, so traces outside its own folder are easy to see.
    if (focused === t.owner) {
      const places = h("ul", { class: "places", attrs: { "aria-label": `Where ${o.name} keeps things` } });
      for (const f of t.folders.slice(0, 60)) {
        places.append(h("li", {}, h("button", { class: "place", title: tree.path(f), onclick: () => act.reveal(f) },
          h("span", { class: "kind" }, placeKind(tree.path(f))),
          h("span", { class: "size" }, human(tree.agg[f])),
          h("span", { class: "path" }, tree.path(f)))));
      }
      if (t.folders.length > 60) places.append(h("li", { class: "quiet" }, `and ${count(t.folders.length - 60)} more`));
      ol.append(h("li", { class: "places-wrap" }, h("p", { class: "quiet" }, `Where ${o.name} keeps things`), places));
    }
  }
  el.append(ol);
}

/** The key: what each shape means, drawn with the real rules. */
export function renderKey(el: HTMLElement, p: Palette, mode: ColorMode, cats: string[], appNames: { name: string; color: string }[]): void {
  clear(el);
  const item = (look: Pick<Look, "color" | "ringColor" | "ringSize" | "gapColor" | "gapSize">, label: string): HTMLElement => h("span", { class: "k" }, shapeIcon(look, 12), label);
  const folder = { color: p.folder, ringColor: mix(p.folder, p.text, 0.35), ringSize: 0.16, gapColor: p.bg, gapSize: 0 };
  if (mode === "risk") {
    el.append(
      item({ color: p.risk.safe, ringColor: mix(p.risk.safe, p.text, 0.35), ringSize: 0.16, gapColor: p.bg, gapSize: 0 }, "Safe"),
      item({ color: p.folder, ringColor: p.risk.probably_safe, ringSize: 0.3, gapColor: p.bg, gapSize: 0.14 }, "Probably safe"),
      item({ color: p.risk.dont_touch, ringColor: p.risk.dont_touch, ringSize: 0.3, gapColor: p.bg, gapSize: 0.2 }, "Don't touch"),
      item(folder, "Folder"),
      item({ color: p.bg, ringColor: p.folder, ringSize: 0.22, gapColor: p.bg, gapSize: 0 }, "Open folder"),
      item({ color: p.file, ringColor: p.file, ringSize: 0, gapColor: p.bg, gapSize: 0 }, "File"),
      item(groupLook(p), "More items"),
    );
  } else if (mode === "app") {
    for (const a of appNames) el.append(item({ color: a.color, ringColor: a.color, ringSize: 0, gapColor: p.bg, gapSize: 0 }, a.name));
    el.append(item({ color: p.noOwner, ringColor: p.noOwner, ringSize: 0, gapColor: p.bg, gapSize: 0 }, "No app"));
  } else {
    cats.forEach((c, i) => el.append(item({ color: p.cats[i], ringColor: p.cats[i], ringSize: 0, gapColor: p.bg, gapSize: 0 }, c)));
  }
}

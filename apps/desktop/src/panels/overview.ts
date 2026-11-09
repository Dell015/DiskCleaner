// The Overview page: everything on the disk at a glance. Apps with size and last use, programs
// nobody installed, Downloads (or any folder) sorted by type with hints, big and old files.
// Read-only: every row only opens the entry in the Files view.

import { KIND } from "../core/format";
import { ago, appRows, bigFiles, BUCKETS, portablePrograms, sortFolder, spaceBuckets, userFolders, type AppRow, type Bucket, type Sorted } from "../core/inventory";
import type { Tree } from "../core/tree";
import { count, human } from "../core/util";
import { clear, h } from "./dom";
import { placeKind } from "./panels";

export interface OverviewHandlers {
  /** Show an entry in the Files view. */
  open(node: number): void;
  /** Stop sorting a folder of your choice (back to Downloads). */
  clearSort(): void;
}

type AppSort = "size" | "used" | "name";
const SIX_MONTHS = 182 * 86_400;
const BUCKET_COLORS: Record<Bucket, string> = { apps: "var(--accent)", yours: "var(--safe)", windows: "var(--folder)", other: "var(--text-2)" };

export class OverviewView {
  private el: HTMLElement;
  private open = new Set<string>();
  private appSort: AppSort = "size";
  private unusedOnly = false;
  private appsShown = 25;
  private last: { tree: Tree; version: number; sortRoot: number; live: boolean } | null = null;

  constructor(host: HTMLElement, private readonly on: OverviewHandlers) {
    this.el = host;
    host.classList.add("overview");
  }

  /**
   * Draw the page for `tree`. `sortRoot` is a folder the user asked to sort (-1: Downloads).
   * `now` is the time the scan was made (for "how long ago").
   */
  show(tree: Tree, now: number, sortRoot: number, live: boolean, force = false): void {
    const same = this.last && this.last.tree === tree && this.last.version === tree.version && this.last.sortRoot === sortRoot && this.last.live === live;
    if (same && !force) return;
    // A newly picked folder to sort is shown from the top; otherwise keep the reading place.
    const top = this.last && this.last.sortRoot !== sortRoot ? 0 : this.el.scrollTop;
    this.last = { tree, version: tree.version, sortRoot, live };
    this.now = now;
    clear(this.el);
    const page = h("div", { class: "ov-page" });
    page.append(this.summary(tree, live));
    if (live) page.append(h("p", { class: "ov-note" }, "The scan is still running. Apps, their sizes and the findings arrive when it is done; nothing is changed meanwhile."));
    const sorts: number[] = sortRoot >= 0 ? [sortRoot] : userFolders(tree).filter((f) => tree.names[f].toLowerCase() === "downloads");
    for (const f of sorts) page.append(this.sorted(tree, sortFolder(tree, f, now), sortRoot >= 0));
    if (!live) page.append(this.apps(tree, now));
    page.append(this.portable(tree));
    page.append(this.files(tree, now));
    this.el.append(page);
    this.el.scrollTop = top;
  }

  private now = 0;

  private redraw(): void {
    if (this.last) this.show(this.last.tree, this.now, this.last.sortRoot, this.last.live, true);
  }

  private section(id: string, title: string, sub: string | Node, ...body: (Node | null)[]): HTMLElement {
    return h("section", { class: "ov-sec", attrs: { "aria-labelledby": `ov-${id}` } },
      h("header", { class: "ov-head" }, h("h2", { attrs: { id: `ov-${id}` } }, title), typeof sub === "string" ? h("p", { class: "quiet" }, sub) : sub),
      ...body);
  }

  private row(tree: Tree, i: number, extra: string, now: number): HTMLElement {
    const changed = tree.kind[i] === KIND.dir ? tree.newest[i] : tree.mtime[i];
    return h("button", { class: "ov-row", title: tree.path(i), onclick: () => this.on.open(i) },
      h("span", { class: "nm" }, tree.names[i]),
      h("span", { class: "sz" }, human(tree.agg[i])),
      h("span", { class: "path" }, tree.path(i)),
      h("span", { class: "meta" }, [extra, changed ? `changed ${ago(changed, now)}` : ""].filter(Boolean).join(" · ")));
  }

  /** A list that shows `first` rows and can be opened to show more. */
  private list(id: string, items: number[], render: (i: number) => HTMLElement, first = 8): HTMLElement {
    const all = this.open.has(id);
    const ol = h("div", { class: "ov-list" });
    for (const i of items.slice(0, all ? 200 : first)) ol.append(render(i));
    if (items.length > first) {
      ol.append(h("button", { class: "ov-more", onclick: () => { if (all) this.open.delete(id); else this.open.add(id); this.redraw(); } }, all ? "Show fewer" : `Show ${count(Math.min(200, items.length) - first)} more`));
    }
    return ol;
  }

  // ---------- where the space goes ----------

  private summary(tree: Tree, live: boolean): HTMLElement {
    const b = spaceBuckets(tree);
    const total = Math.max(1, tree.agg[0]);
    const bar = h("div", { class: "ov-bar", attrs: { role: "img", "aria-label": BUCKETS.map((k) => `${k.label} ${human(b[k.id])}`).join(", ") } });
    const legend = h("div", { class: "ov-legend" });
    for (const k of BUCKETS) {
      if (b[k.id] <= 0) continue;
      const part = h("span", {});
      part.style.width = `${(b[k.id] / total) * 100}%`;
      part.style.background = BUCKET_COLORS[k.id];
      bar.append(part);
      const dot = h("i", {});
      dot.style.background = BUCKET_COLORS[k.id];
      legend.append(h("span", {}, dot, h("b", {}, human(b[k.id])), ` ${k.label}`));
    }
    const name = tree.n > 1 ? tree.names[1] : "This PC";
    return h("header", { class: "ov-top" },
      h("div", {}, h("h1", {}, live ? `Reading ${name}` : name), h("p", { class: "quiet" }, `${count(tree.files[0])} files, ${human(tree.agg[0])} in all`)),
      bar, legend,
      !live && b.other > 0 ? h("p", { class: "quiet" }, "\"Everything else\" is space that no app claims and that is not in your Users folders, for example tools unpacked by hand, game libraries outside Program Files, or old copies.") : null);
  }

  // ---------- apps ----------

  private apps(tree: Tree, now: number): HTMLElement {
    let rows = appRows(tree);
    const total = rows.length;
    if (total === 0) return this.section("apps", "Apps", "No app information in this scan. Scan with the desktop app, or make the view file with --apps.");
    if (this.unusedOnly) rows = rows.filter((r) => { const o = tree.owners[r.owner]; return o.last_used !== undefined && now - o.last_used >= SIX_MONTHS; });
    const by: Record<AppSort, (a: AppRow, b: AppRow) => number> = {
      size: (a, b) => (b.bytes || tree.owners[b.owner].stated_bytes || 0) - (a.bytes || tree.owners[a.owner].stated_bytes || 0),
      used: (a, b) => (tree.owners[a.owner].last_used ?? Infinity) - (tree.owners[b.owner].last_used ?? Infinity),
      name: (a, b) => tree.owners[a.owner].name.localeCompare(tree.owners[b.owner].name),
    };
    rows.sort(by[this.appSort]);
    const known = appRows(tree).filter((r) => tree.owners[r.owner].last_used !== undefined).length;
    const tools = h("div", { class: "ov-tools" },
      h("span", { class: "quiet" }, "Sort"),
      ...(["size", "used", "name"] as AppSort[]).map((k) => h("button", { class: this.appSort === k ? "on" : "", attrs: { "aria-pressed": String(this.appSort === k) }, onclick: () => { this.appSort = k; this.redraw(); } }, k === "size" ? "Size" : k === "used" ? "Least used first" : "Name")),
      h("label", { class: "ov-check" }, (() => { const c = h("input", { attrs: { type: "checkbox" } }); c.checked = this.unusedOnly; c.addEventListener("change", () => { this.unusedOnly = c.checked; this.redraw(); }); return c; })(), " Not used for 6 months"));
    const table = h("div", { class: "ov-apps", attrs: { role: "table", "aria-label": "Apps" } },
      h("div", { class: "ov-app head", attrs: { role: "row" } }, h("span", {}, "App"), h("span", { class: "r" }, "Size"), h("span", {}, "Last used"), h("span", {}, "Installed"), h("span", {}, "Where")));
    for (const r of rows.slice(0, this.appsShown)) table.append(...this.appRow(tree, r, now));
    const more = rows.length > this.appsShown ? h("button", { class: "ov-more", onclick: () => { this.appsShown += 50; this.redraw(); } }, `Show ${count(Math.min(50, rows.length - this.appsShown))} more of ${count(rows.length)}`) : null;
    const note = known === 0
      ? "When each app was last used is not known for this scan (Windows' record of started programs was not read)."
      : "\"Last used\" comes from Windows' own record of programs started from the Start menu, the taskbar or Explorer. Programs started another way may show \"not known\".";
    return this.section("apps", `Apps (${count(total)})`, note, tools, table, more);
  }

  private appRow(tree: Tree, r: AppRow, now: number): HTMLElement[] {
    const o = tree.owners[r.owner];
    const id = `app:${o.name}:${r.owner}`;
    const opened = this.open.has(id);
    const old = o.last_used !== undefined && now - o.last_used >= SIX_MONTHS;
    const size = r.bytes > 0 ? h("span", { class: "r b" }, human(r.bytes)) : h("span", { class: "r dim", title: "Nothing of it is in the scanned folders; this is the size its installer stated" }, o.stated_bytes ? `about ${human(o.stated_bytes)}` : "not here");
    const kinds = [...new Set(r.places.map((p) => placeKind(tree.path(p))))];
    const row = h("button", { class: `ov-app${opened ? " open" : ""}`, attrs: { role: "row", "aria-expanded": String(opened) }, onclick: () => { if (opened) this.open.delete(id); else this.open.add(id); this.redraw(); } },
      h("span", { class: "nm" }, h("b", {}, o.name), o.publisher ? h("small", {}, o.publisher) : null),
      size,
      h("span", { class: old ? "warn" : "" }, ago(o.last_used, now)),
      h("span", { class: "dim" }, o.installed ?? ""),
      h("span", { class: "dim" }, r.places.length === 0 ? "" : r.places.length === 1 ? kinds[0] : `${count(r.places.length)} places`));
    if (!opened) return [row];
    const detail = h("div", { class: "ov-app-detail" });
    const facts: string[] = [];
    if (o.version) facts.push(`version ${o.version}`);
    if (o.runs !== undefined) facts.push(`started ${count(o.runs)} times`);
    facts.push(o.uninstaller ? "has its own uninstaller" : "no uninstaller registered");
    detail.append(h("p", { class: "quiet" }, facts.join(" · ")));
    if (r.places.length === 0) detail.append(h("p", { class: "quiet" }, o.folder ? `Installed in ${o.folder}, which is outside the scanned folders.` : "None of its files are in the scanned folders."));
    for (const p of r.places.slice(0, 40)) {
      detail.append(h("button", { class: "ov-row", title: tree.path(p), onclick: () => this.on.open(p) },
        h("span", { class: "nm" }, placeKind(tree.path(p))), h("span", { class: "sz" }, human(tree.agg[p])), h("span", { class: "path" }, tree.path(p))));
    }
    return [row, detail];
  }

  // ---------- programs nobody installed ----------

  private portable(tree: Tree): HTMLElement {
    const found = portablePrograms(tree);
    if (found.length === 0) return this.section("portable", "Programs nobody installed", "None found outside Windows' own folders.");
    const bytes = found.reduce((a, p) => a + tree.agg[p.node], 0);
    return this.section("portable", `Programs nobody installed (${count(found.length)}, ${human(bytes)})`,
      "Programs that no installed app claims: tools and games unpacked or downloaded and run straight away. Windows does not list these in its apps list.",
      this.list("portable", found.map((p) => p.node), (i) => {
        const p = found.find((x) => x.node === i)!;
        return this.row(tree, i, tree.kind[i] === KIND.dir ? `${count(p.programs)} program${p.programs === 1 ? "" : "s"} inside` : "a program", this.now);
      }));
  }

  // ---------- a messy folder, sorted ----------

  private sorted(tree: Tree, s: Sorted, chosen: boolean): HTMLElement {
    const root = s.root;
    const total = Math.max(1, s.groups.reduce((a, g) => a + g.bytes, 0));
    const grid = h("div", { class: "ov-groups" });
    for (const g of s.groups) {
      const id = `group:${root}:${g.id}`;
      const opened = this.open.has(id);
      const share = h("span", { class: "share" }, h("span", {}));
      (share.firstElementChild as HTMLElement).style.width = `${Math.max(1, (g.bytes / total) * 100)}%`;
      grid.append(h("button", { class: `ov-group${opened ? " on" : ""}`, attrs: { "aria-expanded": String(opened) }, onclick: () => { if (opened) this.open.delete(id); else this.open.add(id); this.redraw(); } },
        h("span", { class: "label" }, g.label),
        h("b", {}, human(g.bytes)),
        h("span", { class: "quiet" }, `${count(g.count)} file${g.count === 1 ? "" : "s"}`),
        share,
        g.oldCount > 0 ? h("span", { class: "old" }, `${human(g.oldBytes)} untouched for a year`) : h("span", { class: "old none" }, "")));
    }
    const lists: HTMLElement[] = [];
    for (const g of s.groups) {
      const id = `group:${root}:${g.id}`;
      if (!this.open.has(id)) continue;
      lists.push(h("div", { class: "ov-sub" }, h("h3", {}, `${g.label}: biggest first`), this.list(`${id}:list`, g.top, (i) => this.row(tree, i, "", this.now), 12)));
    }
    const hintBox = h("div", { class: "ov-hints" });
    const hint = (kind: "installer_of_installed" | "unpacked" | "untouched", title: string, why: string): void => {
      const items = s.hints.filter((x) => x.kind === kind);
      if (items.length === 0) return;
      const bytes = items.reduce((a, x) => a + tree.agg[x.node], 0);
      const id = `hint:${root}:${kind}`;
      const opened = this.open.has(id);
      hintBox.append(h("button", { class: `ov-hint${opened ? " on" : ""}`, attrs: { "aria-expanded": String(opened) }, onclick: () => { if (opened) this.open.delete(id); else this.open.add(id); this.redraw(); } },
        h("b", {}, title), h("span", {}, `${count(items.length)} · ${human(bytes)}`), h("span", { class: "quiet" }, why)));
      if (opened) {
        hintBox.append(this.list(`${id}:list`, items.map((x) => x.node), (i) => {
          const x = items.find((y) => y.node === i)!;
          const extra = kind === "installer_of_installed" ? `${tree.owners[x.other]?.name ?? "an app"} is installed` : kind === "unpacked" ? `unpacked into "${tree.names[x.other]}"` : "";
          return this.row(tree, i, extra, this.now);
        }, 10));
      }
    };
    hint("installer_of_installed", "Installers of apps you already have", "The app is installed, so its installer is usually no longer needed.");
    hint("unpacked", "Archives already unpacked", "A folder with the same name is right next to it.");
    hint("untouched", "Not changed for a year or more", "Old downloads and folders nobody has touched.");
    const head = h("div", { class: "ov-head-row" }, h("span", { class: "path" }, tree.path(root)), chosen ? h("button", { onclick: () => this.on.clearSort() }, "Back to Downloads") : null);
    return this.section(`sorted-${root}`, chosen ? `Sorted: ${tree.names[root].replace(/[\\/]+$/, "") || tree.names[root]}` : `${tree.names[root]} (${human(tree.agg[root])})`, head, grid, ...lists, hintBox);
  }

  // ---------- big and old files ----------

  private files(tree: Tree, now: number): HTMLElement {
    const f = bigFiles(tree, now, 200);
    return h("div", { class: "ov-two" },
      this.section("big", "Biggest files", "The largest single files anywhere in the scan.", this.list("big", f.big, (i) => this.row(tree, i, "", now), 10)),
      this.section("old", `Not changed for a year (${human(f.oldBytes)} in ${count(f.oldCount)} files)`, "Biggest first, outside Windows' own folders. Old is not the same as unneeded: look before you decide.", this.list("old", f.old, (i) => this.row(tree, i, "", now), 10)));
  }
}

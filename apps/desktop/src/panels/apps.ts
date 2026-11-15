// The Apps view: every app as a card with its icon, size and last use; picking one opens its
// page with every place it keeps things, sorted by what removing them would do. Read-only apart
// from the pick boxes, which only select items for the usual review screen.

import { appCards, clearables, placeRole, ROLE_TEXT, type AppCard, type PlaceRole } from "../core/apps";
import { ago } from "../core/inventory";
import type { Tree } from "../core/tree";
import { count, human } from "../core/util";
import { clear, h } from "./dom";
import { pickBox, type PickHandlers } from "./explorer";
import { placeKind } from "./panels";

export interface AppsHandlers {
  /** Show an entry in the Files view. */
  open(node: number): void;
  /** Show this app's network on the map (everything else faded). */
  showOnMap(owner: number): void;
  /** The app's icon when it is loaded (desktop app). */
  appIcon(owner: number): string | undefined;
  /** Picking for removing (only when removing is available). */
  pick?: PickHandlers;
}

type Sort = "size" | "used" | "name";

const ROLES: PlaceRole[] = ["program", "data", "saves", "temp"];

export class AppsView {
  private el: HTMLElement;
  private sort: Sort = "size";
  private filter = "";
  private showSystem = false;
  /** The app whose page is open, or -1 for the grid. */
  owner = -1;
  private last: { tree: Tree; version: number; owner: number } | null = null;
  private now = 0;

  constructor(host: HTMLElement, private readonly on: AppsHandlers) {
    this.el = host;
    host.classList.add("appsview");
  }

  show(tree: Tree, now: number, force = false): void {
    const same = this.last && this.last.tree === tree && this.last.version === tree.version && this.last.owner === this.owner;
    if (same && !force) return;
    const keepScroll = this.last && this.last.owner === this.owner ? this.el.scrollTop : 0;
    this.last = { tree, version: tree.version, owner: this.owner };
    this.now = now;
    clear(this.el);
    const page = h("div", { class: "ap-page" });
    if (this.owner >= 0 && this.owner < tree.owners.length) page.append(...this.detail(tree, this.owner));
    else page.append(...this.grid(tree));
    this.el.append(page);
    this.el.scrollTop = keepScroll;
  }

  /** Open an app's page (or the grid with -1). */
  openApp(owner: number): void {
    this.owner = owner;
    this.el.scrollTop = 0;
  }

  private icon(owner: number, name: string, big: boolean): HTMLElement {
    const url = this.on.appIcon(owner);
    const size = big ? "32" : "24";
    if (url) return h("img", { class: "ap-icon", attrs: { src: url, alt: "", width: size, height: size, draggable: "false" } });
    // No icon: the first letter on a quiet tile.
    return h("span", { class: "ap-icon letter", attrs: { "aria-hidden": "true" } }, (name.trim()[0] ?? "?").toUpperCase());
  }

  private grid(tree: Tree): HTMLElement[] {
    const cards = appCards(tree);
    const q = this.filter.trim().toLowerCase();
    const match = (c: AppCard): boolean => !q || tree.owners[c.owner].name.toLowerCase().includes(q) || (tree.owners[c.owner].publisher ?? "").toLowerCase().includes(q);
    const sorted = (list: AppCard[]): AppCard[] => {
      const o = (c: AppCard) => tree.owners[c.owner];
      if (this.sort === "name") return [...list].sort((a, b) => o(a).name.localeCompare(o(b).name));
      if (this.sort === "used") return [...list].sort((a, b) => (o(a).last_used ?? 0) - (o(b).last_used ?? 0) || b.bytes - a.bytes);
      return list;
    };
    const used = cards.filter((c) => !c.system && c.bytes > 0 && match(c));
    const system = cards.filter((c) => c.system && c.bytes > 0 && match(c));
    const elsewhere = cards.filter((c) => c.bytes === 0 && match(c));

    const head = h("div", { class: "ap-head" },
      h("h2", {}, "Apps"),
      h("p", { class: "quiet" }, `${count(cards.length)} installed apps. Pick one to see every place it keeps things, what can go without harming it, and what it needs.`));
    const search = h("input", { class: "ap-filter", attrs: { type: "search", placeholder: "Find an app", "aria-label": "Find an app", value: this.filter } }) as HTMLInputElement;
    search.addEventListener("input", () => {
      this.filter = search.value;
      const at = search.selectionStart;
      this.redraw(tree);
      const again = this.el.querySelector<HTMLInputElement>(".ap-filter");
      again?.focus();
      if (again && at !== null) again.setSelectionRange(at, at);
    });
    const sortSeg = h("div", { class: "seg small", attrs: { role: "group", "aria-label": "Sort apps" } });
    for (const [id, label] of [["size", "Biggest"], ["used", "Least used"], ["name", "Name"]] as [Sort, string][]) {
      sortSeg.append(h("button", { class: this.sort === id ? "on" : "", attrs: { "aria-pressed": String(this.sort === id) }, onclick: () => { this.sort = id; this.redraw(tree); } }, label));
    }
    const out: HTMLElement[] = [head, h("div", { class: "ap-tools" }, search, sortSeg)];
    out.push(this.cardGrid(tree, sorted(used)));
    if (!used.length) out.push(h("p", { class: "quiet" }, q ? "No app matches." : "No apps in this scan."));
    if (system.length) {
      const d = h("details", { class: "ap-more" }, h("summary", {}, `System parts (${count(system.length)}): drivers, runtimes and Windows components`));
      if (this.showSystem) d.setAttribute("open", "");
      d.addEventListener("toggle", () => { this.showSystem = (d as HTMLDetailsElement).open; });
      d.append(this.cardGrid(tree, sorted(system)));
      out.push(d);
    }
    if (elsewhere.length) {
      const d = h("details", { class: "ap-more" },
        h("summary", {}, `${count(elsewhere.length)} more apps with nothing in this scan`),
        h("p", { class: "quiet" }, "Their folders are on a drive or in a folder that was not scanned. Sizes are Windows' own estimate."),
        this.cardGrid(tree, sorted(elsewhere)));
      out.push(d);
    }
    return out;
  }

  private cardGrid(tree: Tree, cards: AppCard[]): HTMLElement {
    const grid = h("ul", { class: "ap-grid" });
    for (const c of cards) {
      const o = tree.owners[c.owner];
      const size = c.bytes > 0 ? human(c.bytes) : o.stated_bytes ? `~${human(o.stated_bytes)}` : "";
      const sub = c.bytes > 0
        ? `${count(c.places.length)} place${c.places.length === 1 ? "" : "s"} · used ${ago(o.last_used, this.now)}`
        : o.folder ?? "no folder recorded";
      grid.append(h("li", {}, h("button", {
        class: "ap-card",
        title: o.publisher ? `${o.name}, by ${o.publisher}` : o.name,
        onclick: () => { this.openApp(c.owner); this.redraw(tree); },
      }, this.icon(c.owner, o.name, false), h("span", { class: "nm" }, o.name), h("span", { class: "sz" }, size), h("span", { class: "sub" }, sub))));
    }
    return grid;
  }

  private detail(tree: Tree, owner: number): HTMLElement[] {
    const o = tree.owners[owner];
    const card = appCards(tree).find((c) => c.owner === owner) ?? { owner, bytes: 0, places: [], system: false };
    const facts = [
      o.publisher && `by ${o.publisher}`,
      o.launcher && `installed with ${o.launcher}`,
      o.version && `version ${o.version}`,
      o.installed && `installed ${o.installed}`,
      `last used ${ago(o.last_used, this.now)}`,
    ].filter(Boolean).join(" · ");
    const back = h("button", { class: "ap-back", onclick: () => { this.openApp(-1); this.redraw(tree); } }, "← All apps");
    const head = h("div", { class: "ap-detail-head" },
      this.icon(owner, o.name, true),
      h("div", { class: "ap-title" }, h("h2", {}, o.name), h("p", { class: "quiet" }, facts)),
      h("div", { class: "ap-total" }, h("b", {}, human(card.bytes)), h("span", { class: "quiet" }, `in ${count(card.places.length)} place${card.places.length === 1 ? "" : "s"}`)));
    const actions = h("div", { class: "ap-actions" },
      h("button", { class: "primary", attrs: card.places.length === 0 ? { disabled: "" } : {}, onclick: () => this.on.showOnMap(owner) }, "Show on the map"));
    const out: HTMLElement[] = [back, head, actions];
    if (!card.places.length) {
      out.push(h("p", { class: "lead" }, "Nothing of this app is in this scan."), h("p", { class: "quiet" }, o.folder ? `Windows records its folder as ${o.folder}. Scan that drive or folder to see it.` : "Windows has no folder recorded for it."));
      return out;
    }

    // What can go without harming it.
    const clear = clearables(tree, card.places);
    const sec = h("section", { class: "ap-sec" });
    const clearBytes = clear.reduce((a, c) => a + c.bytes, 0);
    sec.append(h("h3", {}, h("span", { class: "dot safe" }), clear.length ? `Can be cleared: ${human(clearBytes)}` : "Nothing to clear safely"),
      h("p", { class: "quiet" }, clear.length ? "Caches, logs and similar files DiskCleaner checked. The app makes them again when it needs them." : "DiskCleaner found nothing it is sure the app can do without."));
    if (clear.length) {
      const ul = h("ul", { class: "ap-list" });
      for (const c of clear.slice(0, 80)) {
        const box = this.on.pick ? pickBox(this.on.pick, c.node, tree.names[c.node]) : null;
        ul.append(h("li", {}, ...(box ? [box] : []), h("button", { class: "ap-row", title: tree.path(c.node), onclick: () => this.on.open(c.node) },
          h("span", { class: `verdict ${c.risk}` }, c.risk === "safe" ? "Safe" : "Probably safe"),
          h("span", { class: "nm" }, c.title),
          h("span", { class: "sz" }, human(c.bytes)),
          h("span", { class: "path" }, tree.path(c.node)))));
      }
      if (clear.length > 80) ul.append(h("li", { class: "quiet" }, `and ${count(clear.length - 80)} more`));
      sec.append(ul);
    }
    out.push(sec);

    // Every place, by what it is for.
    const byRole = new Map<PlaceRole, number[]>();
    for (const p of card.places) {
      const r = placeRole(tree, p, o.folder);
      byRole.set(r, [...(byRole.get(r) ?? []), p]);
    }
    for (const r of ROLES) {
      const list = byRole.get(r);
      if (!list?.length) continue;
      // What can be cleared is counted above, not again here.
      const inside = clear.filter((c) => list.some((p) => c.node === p || tree.isAncestor(p, c.node))).reduce((a, c) => a + c.bytes, 0);
      const bytes = list.reduce((a, p) => a + tree.agg[p], 0) - inside;
      const s = h("section", { class: "ap-sec" },
        h("h3", {}, h("span", { class: `dot ${r === "temp" ? "probably_safe" : "keep"}` }), `${ROLE_TEXT[r].title}: ${human(bytes)}`),
        h("p", { class: "quiet" }, inside > 0 ? `${ROLE_TEXT[r].why} (Not counting the ${human(inside)} that can be cleared.)` : ROLE_TEXT[r].why));
      const ul = h("ul", { class: "ap-list" });
      for (const p of list.slice(0, 60)) {
        ul.append(h("li", {}, h("button", { class: "ap-row", title: tree.path(p), onclick: () => this.on.open(p) },
          h("span", { class: "verdict keep" }, placeKind(tree.path(p))),
          h("span", { class: "nm" }, tree.names[p]),
          h("span", { class: "sz" }, human(tree.agg[p])),
          h("span", { class: "path" }, tree.path(p)))));
      }
      if (list.length > 60) ul.append(h("li", { class: "quiet" }, `and ${count(list.length - 60)} more`));
      s.append(ul);
      out.push(s);
    }
    return out;
  }

  private redraw(tree: Tree): void {
    this.last = null;
    this.show(tree, this.now, true);
  }
}

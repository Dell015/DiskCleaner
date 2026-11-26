// The Library view: every app as a card with its icon, size and last use, grouped by kind
// (games, launchers, developer tools, ...), and your own files grouped by what they are (videos,
// pictures, documents, code projects, ...). Picking an app opens its page with every place it
// keeps things, sorted by what removing them would do; picking a file group opens where those
// files are. Read-only apart from the pick boxes, which only select items for the usual review
// screen, and the kind a person chooses for an app (remembered on this PC).

import { appCards, clearables, gamesOf, launcherOf, layoutNetwork, placeRole, ROLE_TEXT, type AppCard, type PlaceRole } from "../core/apps";
import { appKind, APP_KINDS, KIND_TEXT, type AppKind, webAppOf } from "../core/kinds";
import { viewableIn, viewKind, type ViewKind } from "../core/view";
import { GROUP_TEXT, GROUPS, yourFiles, type Group, type GroupId } from "../core/yours";
import { uninstallSteps } from "../core/uninstall";
import { appNetwork } from "./appnet";
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
  /** The kinds the person chose for apps (by app name). */
  kinds(): Record<string, AppKind>;
  /** Remember a kind for an app (null: back to Rhizome's own guess). */
  setKind(name: string, kind: AppKind | null): void;
  /** Can apps be uninstalled from here (desktop app with removing on)? */
  canUninstall?(): boolean;
  /** Start an app's own uninstaller (or Windows' Installed apps page). */
  uninstall?(name: string, publisher?: string): Promise<{ how: "uninstaller" | "settings"; why: string | null }>;
  /** Which of these apps are still installed. */
  stillInstalled?(names: string[]): Promise<boolean[]>;
  /** Which of these folders are still there. */
  stillThere?(paths: string[]): Promise<boolean[]>;
  /** Size [bytes, files] of each folder now, or null when it is gone (no full scan). */
  folderSizes?(paths: string[]): Promise<([number, number] | null)[]>;
  /** Open the preview window on these files, at `at` (D143). */
  preview?(nodes: number[], at: number): void;
  /** Can apps be moved to another drive from here (B11)? */
  canMove?(): boolean;
  /** Open the "Move to another drive" window for this app. */
  moveApp?(owner: number): void;
  /** A note about this app's earlier move (waiting for a check, moved), or null. */
  moveNote?(name: string): HTMLElement | null;
  /** Remove folders an uninstalled app left behind (checked again by the removal rules). */
  removeLeftBehind?(app: string, paths: string[]): Promise<{ held: [string, number][]; skipped: [string, string][] }>;
}

/** One folder an uninstalled app left behind, as it is now. */
interface Left {
  path: string;
  bytes: number;
  files: number;
  tick: boolean;
  /** After removing: "removed", or why it was skipped. */
  result?: string;
}

/** Where one app's uninstall stands. */
interface Job {
  /** "launcher": the app is removed from inside its launcher (Windows has no uninstaller for it). */
  status: "confirm" | "running" | "settings" | "launcher" | "done" | "failed";
  /** The error, or why Windows' page was opened instead. */
  why?: string;
  /** Its install folder: when that is gone, it is uninstalled (launcher games). */
  folder?: string;
  /** What it left behind (after it is done; undefined while being checked). */
  left?: Left[];
  /** "I've looked, I don't need what's in them." */
  looked?: boolean;
  removing?: boolean;
}

/** How each launcher removes a game it installed. */
function launcherHow(launcher: string, game: string): string {
  const l = launcher.toLowerCase();
  if (l.includes("riot")) return `Open Riot Client, open Settings (top right), pick ${game} on the left, then click Uninstall.`;
  if (l.includes("steam")) return `In Steam's Library, right-click ${game}, then Manage → Uninstall.`;
  if (l.includes("epic")) return `In Epic Games Launcher's Library, click the ⋯ under ${game}, then Uninstall.`;
  if (l.includes("gog")) return `In GOG Galaxy, open ${game}, click the settings icon next to Play, then Manage installation → Uninstall.`;
  if (l.includes("ubisoft")) return `In Ubisoft Connect, open Games, right-click ${game}, then Uninstall.`;
  if (l.includes("hoyoplay")) return `In HoYoPlay, pick ${game}, open the menu next to Start Game, then Uninstall game.`;
  return `Uninstall ${game} from inside ${launcher}.`;
}


type Sort = "size" | "used" | "name";
/** What the Library shows: everything, one kind of app, or your files. */
export type Section = "all" | AppKind | "files";

const ROLES: PlaceRole[] = ["program", "data", "saves", "temp"];

export class AppsView {
  private el: HTMLElement;
  private sort: Sort = "size";
  private filter = "";
  private showSystem = false;
  private section: Section = "all";
  /** Uninstalls started from here, by app name. */
  private jobs = new Map<string, Job>();
  private poll: number | null = null;
  /** Places of apps uninstalled from here, by app name (from the scan, before uninstalling). */
  private placesOf = new Map<string, string[]>();
  /** The app whose page is open, or -1 for the grid. */
  owner = -1;
  /** The file group whose page is open (when no app page is). */
  private group: GroupId | null = null;
  private last: { tree: Tree; version: number; owner: number; group: GroupId | null } | null = null;
  private now = 0;

  constructor(host: HTMLElement, private readonly on: AppsHandlers) {
    this.el = host;
    host.classList.add("appsview");
  }

  show(tree: Tree, now: number, force = false): void {
    const same = this.last && this.last.tree === tree && this.last.version === tree.version && this.last.owner === this.owner && this.last.group === this.group;
    if (same && !force) return;
    // A new scan renumbers the apps (and an uninstalled one is gone): back to the Library.
    if (this.last && this.last.tree !== tree) {
      this.owner = -1;
      this.group = null;
    }
    const keepScroll = this.last && this.last.owner === this.owner && this.last.group === this.group ? this.el.scrollTop : 0;
    this.last = { tree, version: tree.version, owner: this.owner, group: this.group };
    this.now = now;
    this.closeMenu();
    clear(this.el);
    const page = h("div", { class: "ap-page" });
    if (this.owner >= 0 && this.owner < tree.owners.length) page.append(...this.detail(tree, this.owner));
    else if (this.group) page.append(...this.groupPage(tree, this.group));
    else page.append(...this.grid(tree));
    this.el.append(page);
    this.el.scrollTop = keepScroll;
  }

  /** The Library at one of its sections (all apps, a kind, or your files). */
  setSection(s: Section): void {
    this.section = s;
    this.owner = -1;
    this.group = null;
    this.el.scrollTop = 0;
  }

  /** Open an app's page (or the grid with -1). */
  openApp(owner: number): void {
    this.owner = owner;
    if (owner >= 0) this.group = null;
    this.el.scrollTop = 0;
  }

  /** Open a group of your files (or the grid with null). */
  openGroup(g: GroupId | null): void {
    this.owner = -1;
    this.group = g;
    this.el.scrollTop = 0;
  }

  private cardsMemo: { tree: Tree; version: number; cards: AppCard[] } | null = null;

  /** Every app's card, worked out once per scan (it walks the whole scan). */
  private cards(tree: Tree): AppCard[] {
    const m = this.cardsMemo;
    if (m && m.tree === tree && m.version === tree.version) return m.cards;
    const cards = appCards(tree);
    this.cardsMemo = { tree, version: tree.version, cards };
    return cards;
  }

  private kindOf(tree: Tree, owner: number): AppKind {
    return appKind(tree, owner, this.on.kinds());
  }

  private icon(owner: number, name: string, big: boolean): HTMLElement {
    const url = this.on.appIcon(owner);
    const size = big ? "32" : "24";
    if (url) return h("img", { class: "ap-icon", attrs: { src: url, alt: "", width: size, height: size, draggable: "false" } });
    // No icon: the first letter on a quiet tile.
    return h("span", { class: "ap-icon letter", attrs: { "aria-hidden": "true" } }, (name.trim()[0] ?? "?").toUpperCase());
  }

  private grid(tree: Tree): HTMLElement[] {
    const cards = this.cards(tree);
    const q = this.filter.trim().toLowerCase();
    const match = (c: AppCard): boolean => !q || tree.owners[c.owner].name.toLowerCase().includes(q) || (tree.owners[c.owner].publisher ?? "").toLowerCase().includes(q);
    const sorted = (list: AppCard[]): AppCard[] => {
      const o = (c: AppCard) => tree.owners[c.owner];
      if (this.sort === "name") return [...list].sort((a, b) => o(a).name.localeCompare(o(b).name));
      if (this.sort === "used") return [...list].sort((a, b) => (o(a).last_used ?? 0) - (o(b).last_used ?? 0) || b.bytes - a.bytes);
      return list;
    };
    const kinds = new Map<number, AppKind>(cards.map((c) => [c.owner, this.kindOf(tree, c.owner)]));
    const inScan = cards.filter((c) => c.bytes > 0);
    const byKind = new Map<AppKind, AppCard[]>();
    for (const c of inScan) {
      const k = kinds.get(c.owner) ?? "other";
      byKind.set(k, [...(byKind.get(k) ?? []), c]);
    }
    const groups = yourFiles(tree);
    const filesBytes = groups.reduce((a, g) => a + g.bytes, 0);
    const appsBytes = inScan.reduce((a, c) => a + c.bytes, 0);

    const head = h("div", { class: "ap-head" },
      h("h2", {}, "Library"),
      h("p", { class: "quiet" }, `${count(cards.length)} installed apps (${human(appsBytes)}) and ${human(filesBytes)} of your own files. Pick one to see every place it keeps things, what can go, and what must stay.`));

    // Chips: everything, each kind of app present, and your files.
    const chips = h("div", { class: "ap-chips", attrs: { role: "tablist", "aria-label": "Show" } });
    const chip = (id: Section, label: string, bytes: number): void => {
      const on = this.section === id;
      chips.append(h("button", { class: on ? "on" : "", attrs: { role: "tab", "aria-selected": String(on) }, onclick: () => { this.section = id; this.redraw(tree); } },
        h("span", {}, label), bytes > 0 ? h("span", { class: "n" }, human(bytes)) : null));
    };
    chip("all", "Everything", 0);
    if (groups.length) chip("files", "Your files", filesBytes);
    for (const k of APP_KINDS) {
      const list = byKind.get(k);
      if (list?.length) chip(k, KIND_TEXT[k].title, list.reduce((a, c) => a + c.bytes, 0));
    }

    const search = h("input", { class: "ap-filter", attrs: { type: "search", placeholder: "Find an app", "aria-label": "Find an app", value: this.filter } }) as HTMLInputElement;
    search.addEventListener("input", () => {
      this.filter = search.value;
      const at = search.selectionStart;
      if (this.filter && this.section === "files") this.section = "all";
      this.redraw(tree);
      const again = this.el.querySelector<HTMLInputElement>(".ap-filter");
      again?.focus();
      if (again && at !== null) again.setSelectionRange(at, at);
    });
    const sortSeg = h("div", { class: "seg small", attrs: { role: "group", "aria-label": "Sort apps" } });
    for (const [id, label] of [["size", "Biggest"], ["used", "Least used"], ["name", "Name"]] as [Sort, string][]) {
      sortSeg.append(h("button", { class: this.sort === id ? "on" : "", attrs: { "aria-pressed": String(this.sort === id) }, onclick: () => { this.sort = id; this.redraw(tree); } }, label));
    }
    const out: HTMLElement[] = [head, chips, h("div", { class: "ap-tools" }, search, sortSeg)];

    // Your files first: a short row of groups.
    if ((this.section === "all" && !q) || this.section === "files") {
      if (groups.length) {
        out.push(h("section", { class: "ap-kind ap-files" },
          h("h3", {}, "Your files", h("span", { class: "quiet" }, ` · ${human(filesBytes)}`)),
          h("p", { class: "quiet" }, "Files no app owns, outside Windows and program folders, sorted by what they are. Each file counts once."),
          this.groupGrid(groups)));
      } else if (this.section === "files") out.push(h("p", { class: "quiet" }, "None of your own files are in this scan."));
    }

    // Apps, kind by kind.
    let shown = 0;
    for (const k of APP_KINDS) {
      if (this.section !== "all" && this.section !== k) continue;
      const list = (byKind.get(k) ?? []).filter(match);
      if (!list.length) continue;
      shown += list.length;
      const bytes = list.reduce((a, c) => a + c.bytes, 0);
      const title = h("span", {}, KIND_TEXT[k].title, h("span", { class: "quiet" }, ` · ${count(list.length)} · ${human(bytes)}`));
      if (k === "system" && this.section === "all" && !q) {
        const d = h("details", { class: "ap-more" }, h("summary", {}, title), h("p", { class: "quiet" }, KIND_TEXT[k].about));
        if (this.showSystem) d.setAttribute("open", "");
        d.addEventListener("toggle", () => { this.showSystem = (d as HTMLDetailsElement).open; });
        d.append(this.cardGrid(tree, sorted(list), kinds));
        out.push(d);
        continue;
      }
      out.push(h("section", { class: "ap-kind", attrs: { "data-kind": k } }, h("h3", {}, title), h("p", { class: "quiet" }, KIND_TEXT[k].about), this.cardGrid(tree, sorted(list), kinds)));
    }
    if (this.section !== "files" && !shown) out.push(h("p", { class: "quiet" }, q ? "No app matches." : "No apps of this kind in this scan."));

    const elsewhere = cards.filter((c) => c.bytes === 0 && match(c) && (this.section === "all" || this.section === kinds.get(c.owner)));
    if (elsewhere.length && this.section !== "files") {
      out.push(h("details", { class: "ap-more" },
        h("summary", {}, `${count(elsewhere.length)} more apps with nothing in this scan`),
        h("p", { class: "quiet" }, "Their folders are on a drive or in a folder that was not scanned. Sizes are Windows' own estimate."),
        this.cardGrid(tree, sorted(elsewhere), kinds)));
    }
    return out;
  }

  private groupGrid(groups: Group[]): HTMLElement {
    const grid = h("ul", { class: "ap-grid" });
    for (const id of GROUPS) {
      const g = groups.find((x) => x.id === id);
      if (!g) continue;
      const t = GROUP_TEXT[id];
      const sub = g.kinds.slice(0, 2).map((k) => k.label.toLowerCase()).join(", ");
      grid.append(h("li", {}, h("button", { class: "ap-card", title: t.about, onclick: () => { this.openGroup(id); this.redrawLast(); } },
        h("span", { class: `ap-icon letter g-${id}`, attrs: { "aria-hidden": "true" } }, t.mark),
        h("span", { class: "nm" }, t.title), h("span", { class: "sz" }, human(g.bytes)),
        h("span", { class: "sub" }, `${count(g.files)} files · ${sub}`))));
    }
    return grid;
  }

  private cardGrid(tree: Tree, cards: AppCard[], kinds: Map<number, AppKind>): HTMLElement {
    const grid = h("ul", { class: "ap-grid" });
    const all = this.cards(tree);
    const bytesOf = new Map(all.map((c) => [c.owner, c.bytes]));
    const sameName = new Map<string, number>();
    for (const c of all) sameName.set(tree.owners[c.owner].name, (sameName.get(tree.owners[c.owner].name) ?? 0) + 1);
    for (const c of cards) {
      const o = tree.owners[c.owner];
      const size = c.bytes > 0 ? human(c.bytes) : o.stated_bytes ? `~${human(o.stated_bytes)}` : "";
      let sub = c.bytes > 0
        ? `${count(c.places.length)} place${c.places.length === 1 ? "" : "s"} · used ${ago(o.last_used, this.now)}`
        : o.folder ?? "no folder recorded";
      // A launcher says what it installed; a game says where it came from.
      if (kinds.get(c.owner) === "launcher") {
        const games = gamesOf(tree, c.owner);
        if (games.length) sub = `+ ${count(games.length)} game${games.length === 1 ? "" : "s"}, ${human(games.reduce((a, g) => a + (bytesOf.get(g) ?? 0), 0))} · ${sub}`;
      } else if (o.launcher && c.bytes > 0) sub = `via ${o.launcher} · used ${ago(o.last_used, this.now)}`;
      // A website installed as an app, or another app with the same name: say which one it is.
      const web = webAppOf(o.publisher, o.folder);
      if (web) sub = `web app in ${web} · ${sub}`;
      else if ((sameName.get(o.name) ?? 0) > 1) sub = `${o.publisher ? `by ${o.publisher}` : shortWhere(o.folder ?? (c.places[0] !== undefined ? tree.path(c.places[0]) : ""))} · ${sub}`;
      const btn = h("button", {
        class: "ap-card",
        title: `${o.publisher ? `${o.name}, by ${o.publisher}` : o.name}\nRight-click to change its kind`,
        onclick: () => { this.openApp(c.owner); this.redraw(tree); },
      }, this.icon(c.owner, o.name, false), h("span", { class: "nm" }, o.name), h("span", { class: "sz" }, size), h("span", { class: "sub" }, sub));
      btn.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        this.kindMenu(tree, c.owner, e.clientX, e.clientY);
      });
      grid.append(h("li", {}, btn));
    }
    return grid;
  }

  /** The small "Move to" menu for an app's kind. */
  private kindMenu(tree: Tree, owner: number, x: number, y: number): void {
    this.closeMenu();
    const o = tree.owners[owner];
    const now = this.kindOf(tree, owner);
    const chosen = this.on.kinds()[o.name];
    const menu = h("div", { class: "ap-menu", attrs: { role: "menu", "aria-label": `Kind of ${o.name}` } }, h("div", { class: "quiet" }, `Move ${o.name} to`));
    const pick = (k: AppKind | null): void => {
      this.on.setKind(o.name, k);
      this.closeMenu();
      this.redraw(tree);
    };
    for (const k of APP_KINDS) {
      menu.append(h("button", { class: k === now ? "on" : "", attrs: { role: "menuitemradio", "aria-checked": String(k === now) }, onclick: () => pick(k) }, KIND_TEXT[k].title));
    }
    if (chosen) menu.append(h("button", { class: "reset", attrs: { role: "menuitem" }, onclick: () => pick(null) }, "Let Rhizome decide"));
    menu.style.left = `${Math.min(x, window.innerWidth - 230)}px`;
    menu.style.top = `${Math.min(y, window.innerHeight - 40 - 30 * (APP_KINDS.length + 2))}px`;
    document.body.append(menu);
    (menu.querySelector("button.on") as HTMLElement | null ?? menu.querySelector("button"))?.focus();
    const away = (e: Event): void => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !menu.contains(e.target as Node)) this.closeMenu();
    };
    this.menuAway = away;
    window.setTimeout(() => {
      document.addEventListener("mousedown", away, true);
      document.addEventListener("keydown", away, true);
    }, 0);
  }

  private menuAway: ((e: Event) => void) | null = null;

  private closeMenu(): void {
    document.querySelectorAll(".ap-menu").forEach((m) => m.remove());
    if (this.menuAway) {
      document.removeEventListener("mousedown", this.menuAway, true);
      document.removeEventListener("keydown", this.menuAway, true);
      this.menuAway = null;
    }
  }

  /** A group of your files: each kind with the folders holding it and its biggest files. */
  private groupPage(tree: Tree, id: GroupId): HTMLElement[] {
    const t = GROUP_TEXT[id];
    const g = yourFiles(tree).find((x) => x.id === id);
    const back = h("button", { class: "ap-back", onclick: () => { this.openGroup(null); this.redraw(tree); } }, "← Library");
    const head = h("div", { class: "ap-detail-head" },
      h("span", { class: `ap-icon letter big g-${id}`, attrs: { "aria-hidden": "true" } }, t.mark),
      h("div", { class: "ap-title" }, h("h2", {}, t.title), h("p", { class: "quiet" }, t.about)),
      h("div", { class: "ap-total" }, h("b", {}, human(g?.bytes ?? 0)), h("span", { class: "quiet" }, `${count(g?.files ?? 0)} files`)));
    const out: HTMLElement[] = [back, head];
    if (!g) {
      out.push(h("p", { class: "quiet" }, "None in this scan."));
      return out;
    }
    out.push(h("p", { class: "quiet ap-note" }, id === "projects"
      ? `Your work: nothing here is offered for removing.${g.rebuildable ? ` ${human(g.rebuildable)} of it is in folders the project's tools make again (node_modules, target, .venv, build, ...); delete those from inside the project if you need the space.` : ""}`
      : "Your own files: nothing here is offered for removing. Click a folder or file to look at it in Files."));
    for (const k of g.kinds) {
      const s = h("section", { class: "ap-sec" }, h("h3", {}, `${k.label}: ${human(k.bytes)}`, h("span", { class: "quiet" }, ` · ${count(k.files)} files`)));
      const only = VIEW_OF[id];
      const play = (nodes: number[], at: number, folder: boolean): HTMLElement | null => {
        if (!this.on.preview || !nodes.length) return null;
        const k = viewKind(tree.names[nodes[at]]);
        const word = k === "video" || k === "music" ? "Play" : "View";
        return h("button", { class: "ap-play", title: folder ? `${word} what is in this folder` : `${word} it here`, onclick: () => this.on.preview?.(nodes, at) }, `▶ ${folder ? `${word} all` : word}`);
      };
      const row = (node: number, label: string, size: number, note: string, viewer: HTMLElement | null = null): HTMLElement =>
        h("li", { class: viewer ? "ap-viewrow" : "" }, h("button", { class: "ap-row", title: tree.path(node), onclick: () => this.on.open(node) },
          h("span", { class: "verdict keep" }, note), h("span", { class: "nm" }, label), h("span", { class: "sz" }, human(size)), h("span", { class: "path" }, tree.path(node))), viewer);
      if (id === "projects") {
        const ul = h("ul", { class: "ap-list" });
        for (const n of k.biggest) ul.append(row(n, tree.names[n], tree.agg[n], "Project"));
        s.append(ul);
      } else {
        if (k.folders.length) {
          const ul = h("ul", { class: "ap-list" });
          for (const f of k.folders) {
            const inside = only ? viewableIn(tree, f.node, only, 300) : [];
            ul.append(row(f.node, tree.names[f.node].replace(/[\\/]+$/, "") || tree.path(f.node), f.bytes, `${count(f.files)} files`, play(inside, 0, true)));
          }
          s.append(h("div", { class: "quiet" }, "Folders holding the most"), ul);
        }
        if (k.biggest.length > 1) {
          const ul = h("ul", { class: "ap-list" });
          const shown = k.biggest.slice(0, 8);
          const viewable = shown.filter((n) => viewKind(tree.names[n]) !== null);
          for (const n of shown) ul.append(row(n, tree.names[n], tree.agg[n], "File", viewKind(tree.names[n]) ? play(viewable, viewable.indexOf(n), false) : null));
          s.append(h("div", { class: "quiet" }, "Biggest files"), ul);
        }
      }
      out.push(s);
    }
    return out;
  }

  private detail(tree: Tree, owner: number): HTMLElement[] {
    const o = tree.owners[owner];
    const card = this.cards(tree).find((c) => c.owner === owner) ?? { owner, bytes: 0, places: [], system: false };
    const web = webAppOf(o.publisher, o.folder);
    const facts = [
      web ? `a website installed as an app by ${web}` : o.publisher && `by ${o.publisher}`,
      o.launcher && `installed with ${o.launcher}`,
      o.version && `version ${o.version}`,
      o.installed && `installed ${o.installed}`,
      `last used ${ago(o.last_used, this.now)}`,
    ].filter(Boolean).join(" · ");
    const back = h("button", { class: "ap-back", onclick: () => { this.openApp(-1); this.redraw(tree); } }, "← Library");
    const head = h("div", { class: "ap-detail-head" },
      this.icon(owner, o.name, true),
      h("div", { class: "ap-title" }, h("h2", {}, o.name), h("p", { class: "quiet" }, facts)),
      h("div", { class: "ap-total" }, h("b", {}, human(card.bytes)), h("span", { class: "quiet" }, `in ${count(card.places.length)} place${card.places.length === 1 ? "" : "s"}`)));
    const kind = this.kindOf(tree, owner);
    const select = h("select", { class: "ap-kindsel", attrs: { "aria-label": "Kind of app" } }) as HTMLSelectElement;
    for (const k of APP_KINDS) select.append(h("option", { attrs: k === kind ? { value: k, selected: "" } : { value: k } }, KIND_TEXT[k].one));
    select.addEventListener("change", () => {
      this.on.setKind(o.name, select.value as AppKind);
      this.redraw(tree);
    });
    const actions = h("div", { class: "ap-actions" },
      h("button", { class: "primary", attrs: card.places.length === 0 ? { disabled: "" } : {}, onclick: () => this.on.showOnMap(owner) }, "Show on the map"),
      h("label", { class: "ap-kindlab" }, "Kind ", select),
      this.on.kinds()[o.name] ? h("button", { onclick: () => { this.on.setKind(o.name, null); this.redraw(tree); } }, "Let Rhizome decide") : null,
      this.on.canMove?.() && o.kind === "app" && (o.folder || card.places.length) && kind !== "system"
        ? h("button", { class: "ap-move", title: "Copy it to another drive, check every byte, and leave a link so it keeps working", onclick: () => this.on.moveApp?.(owner) }, "Move to another drive")
        : null);
    const note = this.on.moveNote?.(o.name) ?? null;
    const out: HTMLElement[] = note ? [back, head, actions, note] : [back, head, actions];
    const un = this.uninstallSection(tree, owner, kind);
    if (un) out.push(un);

    // A launcher's games, and the launcher a game came from.
    const games = gamesOf(tree, owner);
    if (games.length) {
      const all = this.cards(tree);
      const list = games.map((g) => all.find((c) => c.owner === g) ?? { owner: g, bytes: 0, places: [], system: false }).sort((a, b) => b.bytes - a.bytes);
      out.push(h("section", { class: "ap-sec" },
        h("h3", {}, `Games ${o.name} installed: ${human(list.reduce((a, c) => a + c.bytes, 0))}`),
        h("p", { class: "quiet" }, "Each game lives in its own folder and has its own page. Uninstall or move a game from the launcher, and only remove the launcher after its games."),
        this.cardGrid(tree, list, new Map(list.map((c) => [c.owner, "game" as AppKind])))));
    }
    const from = launcherOf(tree, owner);
    if (from >= 0) {
      out.push(h("p", { class: "quiet ap-note" }, "Installed with ", h("button", { class: "linkish", onclick: () => { this.openApp(from); this.redraw(tree); } }, tree.owners[from].name),
        ". Uninstall, repair or move it from there, so the launcher's records stay right."));
    }
    if (!card.places.length) {
      out.push(h("p", { class: "lead" }, "Nothing of this app is in this scan."), h("p", { class: "quiet" }, o.folder ? `Windows records its folder as ${o.folder}. Scan that drive or folder to see it.` : "Windows has no folder recorded for it."));
      return out;
    }

    // What can go without harming it.
    const clear = clearables(tree, card.places);

    // The app's network: every place it keeps things, and what can be cleared in them.
    const roles = new Map(card.places.map((p) => [p, placeRole(tree, p, o.folder)] as [number, PlaceRole]));
    const net = h("figure", { class: "ap-netwrap" }, appNetwork(layoutNetwork(tree, card.places, roles, clear, o.name), this.on.appIcon(owner), (n) => this.on.open(n)),
      h("figcaption", { class: "ap-key" },
        ...([["program", "Needed to run"], ["data", "Settings and data"], ["saves", "Your saves"], ["temp", "Temporary"], ["safe", "Safe to clear"], ["probably_safe", "Probably safe"]] as [string, string][])
          .map(([k, t]) => h("span", {}, h("i", { class: `k-${k}` }), t)),
        h("span", { class: "quiet" }, "Circle size is space. Click one to open it.")));
    out.push(net);
    const sec = h("section", { class: "ap-sec" });
    const clearBytes = clear.reduce((a, c) => a + c.bytes, 0);
    sec.append(h("h3", {}, h("span", { class: "dot safe" }), clear.length ? `Can be cleared: ${human(clearBytes)}` : "Nothing to clear safely"),
      h("p", { class: "quiet" }, clear.length ? "Caches, logs and similar files Rhizome checked. The app makes them again when it needs them." : "Rhizome found nothing it is sure the app can do without."));
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

  /** The guided uninstall on an app's page: its games and helpers first, the app last. */
  private uninstallSection(tree: Tree, owner: number, kind: AppKind): HTMLElement | null {
    const o = tree.owners[owner];
    if (!this.on.canUninstall?.() || !this.on.uninstall) return null;
    if (kind === "system") {
      return h("section", { class: "ap-sec ap-un" }, h("h3", {}, "Uninstall"),
        h("p", { class: "quiet" }, "This is a part other apps need (a driver, runtime or Windows component), so Rhizome does not offer to uninstall it."));
    }
    const steps = uninstallSteps(tree, owner, kind);
    const sec = h("section", { class: "ap-sec ap-un" }, h("h3", {}, steps.length > 1 ? `Uninstall ${o.name} and what it installed` : `Uninstall ${o.name}`),
      h("p", { class: "quiet" }, steps.length > 1
        ? "In this order: what it installed first, then itself, so each one can still be uninstalled properly. Each uses its own uninstaller, the same one Windows Settings runs."
        : "This starts the app's own uninstaller, the same one Windows Settings runs. Its window opens; follow it there."));
    const cards = new Map(this.cards(tree).map((c) => [c.owner, c]));
    const ol = h("ol", { class: "ap-steps" });
    const pendingBefore = (k: number): string[] => steps.slice(0, k).filter((s) => this.jobs.get(tree.owners[s.owner].name)?.status !== "done").map((s) => tree.owners[s.owner].name);
    steps.forEach((step, k) => {
      const so = tree.owners[step.owner];
      const job = this.jobs.get(so.name);
      const bytes = cards.get(step.owner)?.bytes ?? 0;
      const li = h("li", { class: `ap-step ${job?.status ?? ""}` },
        h("div", { class: "ap-step-head" },
          this.icon(step.owner, so.name, false),
          h("span", { class: "nm" }, so.name),
          h("span", { class: "quiet" }, step.role),
          h("span", { class: "sz" }, bytes > 0 ? human(bytes) : "")));
      const notes: (HTMLElement | string)[] = [];
      if (step.restart) notes.push("Restart your PC after this one: it is a driver that runs from the moment Windows starts.");
      if (!so.uninstaller && !so.launcher) notes.push("Windows records no uninstaller for it; Rhizome will open Windows' Installed apps page instead.");
      const before = pendingBefore(k);
      if (step.owner === owner && before.length && !job) notes.push(`Still installed: ${before.join(", ")}. Uninstall ${before.length === 1 ? "it" : "them"} first, or ${so.name} may not be able to remove ${before.length === 1 ? "it" : "them"} later.`);
      for (const n of notes) li.append(h("p", { class: "quiet" }, n));
      const row = h("div", { class: "ap-step-do" });
      const start = (): void => {
        this.placesOf.set(so.name, (cards.get(step.owner)?.places ?? []).map((p) => tree.path(p)));
        // Windows has no uninstaller for a game its launcher manages: the launcher removes it.
        if (!so.uninstaller && so.launcher) {
          this.jobs.set(so.name, { status: "launcher", folder: so.folder });
          this.watch();
          this.redraw(tree);
          return;
        }
        this.jobs.set(so.name, { status: "running" });
        this.redraw(tree);
        this.on.uninstall?.(so.name, so.publisher).then((r) => {
          this.jobs.set(so.name, r.how === "settings" ? { status: "settings", why: r.why ?? "" } : { status: "running" });
          this.watch();
          this.redrawLast();
        }, (e: unknown) => {
          this.jobs.set(so.name, { status: "failed", why: String(e) });
          this.redrawLast();
        });
      };
      if (!job || job.status === "failed") {
        if (job?.status === "failed") row.append(h("p", { class: "ap-err" }, job.why ?? "It could not be started."));
        row.append(h("button", { class: before.length && step.owner === owner ? "" : "danger", onclick: () => { this.jobs.set(so.name, { status: "confirm" }); this.redraw(tree); } },
          job ? "Try again…" : before.length && step.owner === owner ? "Uninstall anyway…" : "Uninstall…"));
      } else if (job.status === "confirm") {
        const viaLauncher = !so.uninstaller && so.launcher;
        row.append(h("p", {}, viaLauncher
          ? `${so.name} is removed from inside ${so.launcher}, not with a Windows uninstaller. Rhizome will show you where, then watch its folder and notice when it is gone.`
          : `Rhizome will start ${so.name}'s own uninstaller. Its window opens: follow it there. Windows may ask for permission first.`),
          h("div", { class: "ap-actions" },
            h("button", { class: "danger", onclick: start }, viaLauncher ? "Show me how" : "Start uninstaller"),
            h("button", { onclick: () => { this.jobs.delete(so.name); this.redraw(tree); } }, "Cancel")));
      } else if (job.status === "running" || job.status === "settings" || job.status === "launcher") {
        const text = job.status === "running" ? `${so.name}'s uninstaller is open. Finish it in its own window; Rhizome notices when it is done.`
          : job.status === "settings" ? `Windows' Installed apps page is open (${job.why}). Uninstall ${so.name} there; Rhizome notices when it is done.`
            : `${launcherHow(so.launcher ?? "its launcher", so.name)} Rhizome notices when its folder is gone.`;
        // Never stuck: the person can say it is done, or stop waiting.
        row.append(h("p", { class: "ap-wait" }, h("span", { class: "spinner small" }), text),
          h("div", { class: "ap-actions" },
            h("button", { onclick: () => { this.jobs.set(so.name, { status: "done" }); this.redraw(tree); } }, "I've uninstalled it"),
            h("button", { onclick: () => { this.jobs.delete(so.name); this.redraw(tree); } }, "Stop waiting")));
      } else {
        row.append(h("p", { class: "ap-ok" }, step.restart ? "Uninstalled. Restart your PC before going on." : "Uninstalled."));
        row.append(this.leftBox(so.name, job));
      }
      li.append(row);
      ol.append(li);
    });
    sec.append(ol);
    return sec;
  }

  /** Look at just this app's folders now (no full scan): which are left, and how big. */
  private checkLeft(name: string): void {
    const job = this.jobs.get(name);
    const paths = this.placesOf.get(name) ?? [];
    if (!job || job.left || !this.on.folderSizes) return;
    if (!paths.length) {
      job.left = [];
      return;
    }
    void this.on.folderSizes(paths).then((sizes) => {
      job.left = paths.flatMap((path, k) => {
        const s = sizes[k];
        return s ? [{ path, bytes: s[0], files: s[1], tick: false }] : [];
      });
      this.redrawLast();
    }, () => {
      job.left = [];
      this.redrawLast();
    });
  }

  /** What an uninstalled app left behind, ready to tick and remove. */
  private leftBox(name: string, job: Job): HTMLElement {
    const box = h("div", { class: "ap-left" });
    if (!job.left) {
      this.checkLeft(name);
      box.append(h("p", { class: "ap-wait" }, h("span", { class: "spinner small" }), "Checking its folders for what it left behind…"));
      return box;
    }
    const left = job.left;
    const still = left.filter((l) => l.result !== "removed");
    if (!still.length) {
      box.classList.add("done");
      box.append(h("p", {}, left.length ? "Everything it left behind is in the Removed tab." : "It left nothing behind in the folders it used."));
      return box;
    }
    const total = still.reduce((a, l) => a + l.bytes, 0);
    box.append(h("h3", {}, `Left behind: ${human(total)} in ${count(still.length)} folder${still.length === 1 ? "" : "s"}`),
      h("p", { class: "quiet" }, "Only the folders it used were checked. Tick what you want gone; it goes to the Removed tab first, so you can still restore it."));
    const ul = h("ul", { class: "ap-list" });
    for (const l of left) {
      const tick = h("input", { class: "pick", attrs: { type: "checkbox", "aria-label": `Remove ${l.path}` } }) as HTMLInputElement;
      tick.checked = l.tick;
      tick.disabled = Boolean(l.result) || Boolean(job.removing);
      tick.addEventListener("change", () => { l.tick = tick.checked; this.redrawLast(); });
      ul.append(h("li", {}, tick, h("div", { class: "ap-row static", title: l.path },
        h("span", { class: `verdict ${l.result === "removed" ? "safe" : "probably_safe"}` }, l.result === "removed" ? "Removed" : "Left behind"),
        h("span", { class: "nm" }, l.path.split("\\").pop() ?? l.path),
        h("span", { class: "sz" }, `${human(l.bytes)} · ${count(l.files)} files`),
        h("span", { class: "path" }, l.path),
        l.result && l.result !== "removed" ? h("span", { class: "ap-err" }, `Not removed: ${l.result}`) : null)));
    }
    box.append(ul);
    const picked = left.filter((l) => l.tick && !l.result);
    const looked = h("input", { attrs: { type: "checkbox" } }) as HTMLInputElement;
    looked.checked = Boolean(job.looked);
    looked.addEventListener("change", () => { job.looked = looked.checked; this.redrawLast(); });
    const go = h("button", { class: "danger", onclick: () => void this.removeLeft(name, job, picked) },
      job.removing ? "Removing…" : picked.length ? `Remove ${count(picked.length)} (${human(picked.reduce((a, l) => a + l.bytes, 0))})` : "Remove");
    if (!picked.length || !job.looked || job.removing || !this.on.removeLeftBehind) go.setAttribute("disabled", "");
    box.append(h("label", { class: "ap-looked" }, looked, " I looked: I don't need what is in them (settings, logs, saves)."), go);
    return box;
  }

  private async removeLeft(name: string, job: Job, picked: Left[]): Promise<void> {
    if (!this.on.removeLeftBehind || !picked.length) return;
    job.removing = true;
    this.redrawLast();
    try {
      const r = await this.on.removeLeftBehind(name, picked.map((l) => l.path));
      const held = new Set(r.held.map(([p]) => p.toLowerCase()));
      const why = new Map(r.skipped.map(([p, w]) => [p.toLowerCase(), w]));
      for (const l of picked) l.result = held.has(l.path.toLowerCase()) ? "removed" : why.get(l.path.toLowerCase()) ?? "it was not removed";
    } catch (e) {
      for (const l of picked) l.result = String(e);
    }
    job.removing = false;
    this.redrawLast();
  }

  /** Notice when uninstallers finish (Windows' list no longer has the app). */
  private watch(): void {
    if (this.poll !== null || !this.on.stillInstalled) return;
    const tick = async (): Promise<void> => {
      const all = [...this.jobs.entries()];
      // Apps with an uninstaller: gone from Windows' list. Launcher games: their folder is gone.
      const listed = all.filter(([, j]) => j.status === "running" || j.status === "settings").map(([n]) => n);
      const byFolder = all.filter(([, j]) => j.status === "launcher" && j.folder).map(([n, j]) => [n, j.folder as string] as const);
      if (!listed.length && !all.some(([, j]) => j.status === "launcher")) {
        if (this.poll !== null) window.clearInterval(this.poll);
        this.poll = null;
        return;
      }
      try {
        let changed = false;
        if (listed.length) {
          const still = await (this.on.stillInstalled as (n: string[]) => Promise<boolean[]>)(listed);
          listed.forEach((n, k) => {
            if (still[k] === false) {
              this.jobs.set(n, { status: "done" });
              changed = true;
            }
          });
        }
        if (byFolder.length && this.on.stillThere) {
          const there = await this.on.stillThere(byFolder.map(([, f]) => f));
          byFolder.forEach(([n], k) => {
            if (there[k] === false) {
              this.jobs.set(n, { status: "done" });
              changed = true;
            }
          });
        }
        if (changed) this.redrawLast();
      } catch {
        /* try again next time */
      }
    };
    this.poll = window.setInterval(() => void tick(), 3000);
  }

  private redrawLast(): void {
    if (this.last) this.redraw(this.last.tree);
  }

  private redraw(tree: Tree): void {
    this.last = null;
    this.show(tree, this.now, true);
  }
}

/** "C:\\Users\\me\\AppData\\Roaming\\Spotify" → "AppData\\Roaming\\Spotify": enough to tell two apps apart. */
function shortWhere(path: string): string {
  const parts = path.replace(/\//g, "\\").split("\\").filter(Boolean);
  const i = parts.findIndex((p) => /^appdata$/i.test(p));
  if (i >= 0) return parts.slice(i, i + 3).join("\\");
  return parts.slice(-2).join("\\") || "no folder recorded";
}

/** Which files each group of your own files can show in the preview window. */
const VIEW_OF: Partial<Record<GroupId, ViewKind[]>> = {
  videos: ["video"],
  pictures: ["picture"],
  music: ["music"],
  documents: ["pdf", "text"],
  downloads: ["video", "picture", "music", "pdf", "text"],
};

// Home (D140): the calm first screen. How full the scanned drive is, four big buttons for what
// people come to do, the biggest things in plain words (what it is, where, when last used, size),
// and how much can be freed safely. Everything else is one click away. Read-only: buttons only
// open other views.

import { appCards, biggestPlace, clearables, type AppCard } from "../core/apps";
import { ago, spaceBuckets, type Bucket } from "../core/inventory";
import { appKind, KIND_TEXT, type AppKind } from "../core/kinds";
import type { Tree } from "../core/tree";
import { count, human } from "../core/util";
import { GROUP_TEXT, yourFiles, type GroupId } from "../core/yours";
import { clear, h } from "./dom";
import type { MoveInfo } from "../platform";
import { needsAttention } from "./move";

export interface HomeHandlers {
  /** Kinds the person chose for apps. */
  kinds(): Record<string, AppKind>;
  /** Open the guided "Free up space" (at step 1, 2 or 3). */
  freeUp(step: number): void;
  /** The scanned drive or folder as a list, biggest first. */
  biggest(): void;
  /** The Library at its apps, or at the files part. */
  apps(): void;
  files(): void;
  openApp(owner: number): void;
  openGroup(group: GroupId): void;
  /** An app's icon (or a letter tile). */
  appIcon(owner: number, name: string): HTMLElement;
  /** Removed items still waiting (null: removing not available here). */
  removed(): { count: number; bytes: number } | null;
  showRemoved(): void;
  /** The detailed numbers page (the old Overview). */
  overview(): void;
  /** How many days removed items can be put back. */
  retention(): number;
  /** Apps moved to another drive (B11); empty when moving is off. */
  moves?(): MoveInfo[];
  /** Open a move that needs the person (check it, or finish it). */
  checkMove?(m: MoveInfo): void;
  /** The list of moved apps. */
  openMoves?(): void;
}

/** Not used in this long counts as "not used lately". */
export const UNUSED_SECS = 182 * 86_400;

const BAR: { id: Bucket; label: string; color: string; about?: string }[] = [
  { id: "apps", label: "Apps", color: "var(--accent)" },
  { id: "yours", label: "Your files", color: "#e08a3c" },
  { id: "windows", label: "Windows", color: "#5b6b8c" },
  { id: "other", label: "Other", color: "var(--line)", about: "Space no app claims, outside your own folders: tools unpacked by hand, old copies, game libraries outside Program Files." },
];

interface Thing {
  name: string;
  sub: string;
  used: string;
  usedTone: "" | "good" | "warn";
  bytes: number;
  icon: HTMLElement;
  open: () => void;
}

/** The numbers Home shows, worked out once per scan. */
export interface HomeFacts {
  total: number;
  buckets: Record<Bucket, number>;
  freeable: number;
  freeableCount: number;
  unused: AppCard[];
  unusedBytes: number;
  games: AppCard[];
  gamesBytes: number;
}

/** What Home needs from a scan (also used by the Free up space steps). */
export function homeFacts(tree: Tree, now: number, kinds: Record<string, AppKind>): HomeFacts {
  const roots: number[] = [];
  for (let c = tree.firstChild[0]; c >= 0; c = tree.nextSibling[c]) roots.push(c);
  const clear = clearables(tree, roots);
  const cards = appCards(tree).filter((c) => c.bytes > 0);
  const kindOf = (c: AppCard): AppKind => appKind(tree, c.owner, kinds);
  const unused = cards.filter((c) => {
    const last = tree.owners[c.owner].last_used;
    return last !== undefined && last > 0 && now - last > UNUSED_SECS && kindOf(c) !== "system";
  });
  const games = cards.filter((c) => kindOf(c) === "game");
  return {
    total: tree.agg[0],
    buckets: spaceBuckets(tree),
    freeable: clear.reduce((a, c) => a + c.bytes, 0),
    freeableCount: clear.length,
    unused,
    unusedBytes: unused.reduce((a, c) => a + c.bytes, 0),
    games,
    gamesBytes: games.reduce((a, c) => a + c.bytes, 0),
  };
}

export class HomeView {
  private last: { tree: Tree; version: number } | null = null;
  private facts: HomeFacts | null = null;

  constructor(private el: HTMLElement, private on: HomeHandlers) {}

  show(tree: Tree, now: number, scanned: string, force = false): void {
    if (!force && this.last && this.last.tree === tree && this.last.version === tree.version) return;
    this.last = { tree, version: tree.version };
    this.facts = homeFacts(tree, now, this.on.kinds());
    const f = this.facts;
    clear(this.el);
    const page = h("div", { class: "home" });

    // How full it is.
    const names: string[] = [];
    for (let c = tree.firstChild[0]; c >= 0; c = tree.nextSibling[c]) names.push(tree.names[c].replace(/[\\/]+$/, ""));
    const bar = h("div", { class: "hm-bar", attrs: { role: "img", "aria-label": BAR.map((b) => `${b.label} ${human(f.buckets[b.id])}`).join(", ") } });
    const legend = h("div", { class: "hm-legend" });
    const total = Math.max(1, BAR.reduce((a, b) => a + f.buckets[b.id], 0));
    for (const b of BAR) {
      const part = h("span", {});
      part.style.width = `${(f.buckets[b.id] / total) * 100}%`;
      part.style.background = b.color;
      bar.append(part);
      const dot = h("span", { class: "dot" });
      dot.style.background = b.color;
      legend.append(h("span", b.about ? { attrs: { title: b.about } } : {}, dot, `${b.label} `, h("b", {}, human(f.buckets[b.id]))));
    }
    page.append(h("section", { class: "hm-head" },
      h("p", { class: "quiet" }, `${names.join(", ") || "Scan"} · ${scanned}`),
      h("h1", {}, `${human(f.total)} used`),
      bar, legend));

    // What people come to do.
    const tile = (title: string, text: string, onclick: () => void, primary = false, icon = ""): HTMLElement =>
      h("button", { class: `hm-task${primary ? " primary" : ""}`, onclick }, h("span", { class: "hm-ico", attrs: { "aria-hidden": "true" } }, icon), h("span", { class: "hm-tt" }, h("b", {}, title), h("span", {}, text)));
    page.append(h("section", { class: "hm-tasks", attrs: { "aria-label": "What do you want to do?" } },
      tile("Free up space", f.freeable > 0 ? `Step by step, safest first. ${human(f.freeable)} can go right away.` : "Step by step, safest first.", () => this.on.freeUp(1), true, "↧"),
      tile("What's taking my space?", "Everything, biggest first, in plain words.", () => this.on.biggest(), false, "◎"),
      tile("My apps", "By kind and size, when you last used each one.", () => this.on.apps(), false, "▦"),
      tile("My files", "Videos, pictures, documents, projects, downloads.", () => this.on.files(), false, "▤")));

    // Biggest things, and what is worth a look.
    const things = this.things(tree, now);
    const list = h("ul", { class: "hm-list" });
    for (const t of things) {
      list.append(h("li", {}, h("button", { class: "hm-row", onclick: t.open },
        t.icon,
        h("span", { class: "hm-what" }, h("b", {}, t.name), h("span", {}, t.sub)),
        h("span", { class: `hm-used ${t.usedTone}` }, t.used),
        h("span", { class: "hm-size" }, human(t.bytes)),
        h("span", { class: "hm-go", attrs: { "aria-hidden": "true" } }, "›"))));
    }
    const left = h("section", { class: "hm-main" },
      h("div", { class: "hm-h2" }, h("h2", {}, "Biggest things")),
      list);

    const right = h("aside", { class: "hm-side" });
    // Moved apps waiting for the person's check (or stopped in the middle): first, always.
    const moves = this.on.moves?.() ?? [];
    for (const m of moves.filter(needsAttention)) {
      right.append(h("div", { class: "hm-move" },
        h("p", { class: "hm-lt" }, m.state === "waiting" ? "Waiting for your check" : "A move did not finish"),
        h("p", {}, m.state === "waiting" ? `${m.app} was moved to ${m.dest}. Does it work?` : `Rhizome stopped while moving ${m.app}. Finish it safely.`),
        h("button", { class: "primary", onclick: () => this.on.checkMove?.(m) }, m.state === "waiting" ? "Check it" : "Finish safely")));
    }
    right.append(h("div", { class: "hm-free" },
      h("p", {}, "Can be freed safely"),
      h("b", {}, human(f.freeable)),
      h("p", {}, f.freeable > 0 ? `${count(f.freeableCount)} caches and leftovers that apps make again by themselves. You can put anything back for ${this.on.retention()} days.` : "Nothing found that is sure to be safe. Your apps and files are left alone."),
      f.freeable > 0 ? h("button", { onclick: () => this.on.freeUp(1) }, "Review and free it") : null));
    const look = h("div", { class: "hm-look" }, h("p", { class: "hm-lt" }, "Worth a look"));
    const lookRow = (label: string, value: string, onclick: () => void): HTMLElement => h("button", { class: "hm-lrow", onclick }, h("span", {}, label), h("b", {}, value));
    if (f.unused.length) look.append(lookRow("Apps not used in 6+ months", `${count(f.unused.length)} · ${human(f.unusedBytes)}`, () => this.on.freeUp(2)));
    if (f.games.length) look.append(lookRow("Games you could move to another drive", human(f.gamesBytes), () => this.on.freeUp(3)));
    if (look.childElementCount > 1) right.append(look);
    const rm = this.on.removed();
    if (rm && rm.count > 0) right.append(h("button", { class: "hm-lrow hm-removed", onclick: () => this.on.showRemoved() }, h("span", {}, `Removed, can still be put back`), h("b", {}, `${count(rm.count)} · ${human(rm.bytes)}`)));
    if (moves.length) right.append(h("button", { class: "hm-lrow hm-removed", onclick: () => this.on.openMoves?.() }, h("span", {}, "Moved apps"), h("b", {}, count(moves.filter((m) => m.state === "done" || m.state === "waiting").length))));
    right.append(h("button", { class: "textlink", onclick: () => this.on.overview() }, "All the numbers"));

    page.append(h("div", { class: "hm-cols" }, left, right));
    this.el.append(page);
  }

  /** The biggest apps and groups of your own files, in plain words. */
  private things(tree: Tree, now: number): Thing[] {
    const kinds = this.on.kinds();
    const out: Thing[] = [];
    for (const c of appCards(tree)) {
      if (c.bytes <= 0) continue;
      const o = tree.owners[c.owner];
      const kind = appKind(tree, c.owner, kinds);
      if (kind === "system") continue;
      const place = biggestPlace(tree, c.owner);
      const where = place >= 0 ? shortPlace(tree.path(place)) : "";
      const sub = [KIND_TEXT[kind].one, o.launcher ? `installed by ${o.launcher}` : "", where].filter(Boolean).join(" · ");
      const old = o.last_used !== undefined && o.last_used > 0 && now - o.last_used > UNUSED_SECS;
      out.push({
        name: o.name, sub, bytes: c.bytes,
        used: o.last_used ? `Used ${ago(o.last_used, now)}` : "Last use not known",
        usedTone: old ? "warn" : "",
        icon: this.on.appIcon(c.owner, o.name),
        open: () => this.on.openApp(c.owner),
      });
    }
    for (const g of yourFiles(tree)) {
      const t = GROUP_TEXT[g.id];
      out.push({
        name: g.id === "projects" ? "Your code projects" : `Your ${t.title.toLowerCase()}`,
        sub: `${count(g.files)} files · ${g.kinds.slice(0, 2).map((k) => k.label.toLowerCase()).join(", ")}`,
        bytes: g.bytes,
        used: "",
        usedTone: "",
        icon: h("span", { class: "ap-icon letter", attrs: { "aria-hidden": "true" } }, t.mark),
        open: () => this.on.openGroup(g.id),
      });
    }
    return out.sort((a, b) => b.bytes - a.bytes).slice(0, 7);
  }
}

/** "C:\Games\Genshin Impact game" → "C:\Games"; "C:\Users\me\AppData\Local\X" → "AppData". */
export function shortPlace(path: string): string {
  const p = path.replace(/\//g, "\\");
  const m = /\\appdata\\(local|roaming|locallow)\\/i.exec(p);
  if (m) return "AppData";
  const parts = p.split("\\").filter(Boolean);
  if (parts.length <= 2) return parts.join("\\");
  return `${parts[0]}\\${parts[1]}`;
}

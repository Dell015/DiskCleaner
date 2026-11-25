// "Free up space" (D140): one guided flow, safest first. 1. caches and leftovers apps make again
// (ticked through the usual picking, then the usual review); 2. apps not used in 6+ months (each
// opens its page, where it can be uninstalled); 3. big games (moving them to another drive comes
// with B11; for now each opens its page). Stop after any step. Read-only here: removing still goes
// through the review screen.

import { clearables, type AppCard, type Clearable } from "../core/apps";
import { ago } from "../core/inventory";
import type { Tree } from "../core/tree";
import { count, human } from "../core/util";
import { clear, h } from "./dom";
import { pickBox, type PickHandlers } from "./explorer";
import { homeFacts } from "./home";
import type { AppKind } from "../core/kinds";

export interface FreeHandlers {
  kinds(): Record<string, AppKind>;
  pick?: PickHandlers;
  /** Tick or untick many at once (only those that can be picked). */
  setPicked(nodes: number[], on: boolean): void;
  /** Open the review screen for what is ticked. */
  review(): void;
  /** How much is ticked now. */
  picked(): { count: number; bytes: number };
  openApp(owner: number): void;
  appIcon(owner: number, name: string): HTMLElement;
  home(): void;
  retention(): number;
}

const STEPS = ["Caches and leftovers", "Apps you don't use", "Big games"];

export class FreeView {
  step = 1;
  private last: { tree: Tree; version: number; step: number; now: number } | null = null;

  constructor(private el: HTMLElement, private on: FreeHandlers) {}

  go(step: number): void {
    this.step = Math.min(3, Math.max(1, step));
    this.el.scrollTop = 0;
  }

  show(tree: Tree, now: number, force = false): void {
    if (!force && this.last && this.last.tree === tree && this.last.version === tree.version && this.last.step === this.step) return;
    this.last = { tree, version: tree.version, step: this.step, now };
    clear(this.el);
    const f = homeFacts(tree, now, this.on.kinds());
    const page = h("div", { class: "home free" });
    const steps = h("ol", { class: "fr-steps", attrs: { "aria-label": "Steps" } });
    const amounts = [f.freeable, f.unusedBytes, f.gamesBytes];
    STEPS.forEach((label, k) => {
      const n = k + 1;
      const on = n === this.step;
      steps.append(h("li", {}, h("button", { class: on ? "on" : "", attrs: on ? { "aria-current": "step" } : {}, onclick: () => this.goAndShow(tree, now, n) },
        h("span", { class: "fr-n" }, String(n)), h("span", {}, h("b", {}, label), h("span", {}, amounts[k] > 0 ? human(amounts[k]) : "nothing found")))));
    });
    page.append(
      h("div", { class: "fr-head" },
        h("button", { class: "textlink fr-back", onclick: () => this.on.home() }, "← Home"),
        h("h1", {}, "Free up space"),
        h("p", { class: "quiet" }, "Safest first. Do as many steps as you like, and stop after any of them.")),
      steps);
    if (this.step === 1) page.append(...this.cleanup(tree, f.freeable));
    else if (this.step === 2) page.append(...this.appList(tree, now, f.unused, "These apps have not been started in over six months (as Windows records it). Open one to see what it keeps and to uninstall it with its own uninstaller.", "No app is known to be unused for six months. (Rhizome only knows for apps Windows records starts of.)"));
    else page.append(...this.appList(tree, now, f.games, "Games are big and you may still play them: moving one to another drive frees C: without losing it. Moving apps comes in the next update; for now, open a game to see where it is, or uninstall it from its launcher.", "No games found in this scan."));
    const next = this.step < 3 ? h("button", { class: "primary", onclick: () => this.goAndShow(tree, now, this.step + 1) }, `Next: ${STEPS[this.step]}`) : h("button", { class: "primary", onclick: () => this.on.home() }, "Done");
    page.append(h("div", { class: "fr-nav" }, this.step > 1 ? h("button", { onclick: () => this.goAndShow(tree, now, this.step - 1) }, "Back") : h("span", {}), next));
    this.el.append(page);
  }

  private goAndShow(tree: Tree, now: number, step: number): void {
    this.go(step);
    this.show(tree, now, true);
  }

  /** Step 1: what the checks offer, biggest first, ticked through the usual picking. */
  private cleanup(tree: Tree, total: number): HTMLElement[] {
    const roots: number[] = [];
    for (let c = tree.firstChild[0]; c >= 0; c = tree.nextSibling[c]) roots.push(c);
    const items = clearables(tree, roots);
    if (!items.length) return [h("p", { class: "lead" }, "Nothing found that is sure to be safe to remove. Your apps and files are left alone.")];
    const out: HTMLElement[] = [];
    const pick = this.on.pick;
    if (!pick?.enabled()) {
      out.push(h("p", { class: "warn" }, "Removing is not available right now (it needs the desktop app and a scan from the last hour). You can still look through the list."));
    }
    const safe = items.filter((c) => c.risk === "safe").map((c) => c.node);
    const ticked = this.on.picked();
    out.push(h("div", { class: "fr-sum" },
      h("p", {}, h("b", {}, human(total)), ` in ${count(items.length)} places. "Safe" ones are made again by their apps; "Look first" ones usually are, but check them.`),
      pick?.enabled() ? h("div", { class: "fr-actions" },
        h("button", { onclick: () => { this.on.setPicked(safe, true); this.refresh(); } }, `Tick all safe (${count(safe.length)})`),
        h("button", { onclick: () => { this.on.setPicked(items.map((c) => c.node), false); this.refresh(); } }, "Untick all"),
        h("button", { class: "primary", attrs: ticked.count ? {} : { disabled: "" }, onclick: () => this.on.review() }, ticked.count ? `Review and remove ${human(ticked.bytes)}…` : "Review and remove…")) : null));
    const ul = h("ul", { class: "hm-list fr-list" });
    for (const c of items.slice(0, 60)) ul.append(this.clearRow(tree, c));
    out.push(ul);
    if (items.length > 60) out.push(h("p", { class: "quiet" }, `…and ${count(items.length - 60)} smaller ones. Findings (top right) lists them all.`));
    out.push(h("p", { class: "quiet" }, `Removed things wait ${this.on.retention()} days in a holding folder before they are deleted for good, so you can put them back.`));
    return out;
  }

  private clearRow(tree: Tree, c: Clearable): HTMLElement {
    const box = pickBox(this.on.pick, c.node, tree.names[c.node]);
    box?.addEventListener("change", () => setTimeout(() => this.refresh(), 0));
    return h("li", { class: "fr-row" }, box ?? h("span", {}),
      h("span", { class: "hm-what" }, h("b", {}, c.title), h("span", { class: "path" }, tree.path(c.node))),
      h("span", { class: `fr-risk ${c.risk}` }, c.risk === "safe" ? "Safe" : "Look first"),
      h("span", { class: "hm-size" }, human(c.bytes)));
  }

  /** Steps 2 and 3: apps, each opening its page. */
  private appList(tree: Tree, now: number, cards: AppCard[], intro: string, none: string): HTMLElement[] {
    if (!cards.length) return [h("p", { class: "lead" }, none)];
    const ul = h("ul", { class: "hm-list" });
    for (const c of [...cards].sort((a, b) => b.bytes - a.bytes)) {
      const o = tree.owners[c.owner];
      ul.append(h("li", {}, h("button", { class: "hm-row", onclick: () => this.on.openApp(c.owner) },
        this.on.appIcon(c.owner, o.name),
        h("span", { class: "hm-what" }, h("b", {}, o.name), h("span", {}, o.launcher ? `Installed by ${o.launcher}` : o.publisher ?? "")),
        h("span", { class: "hm-used" }, o.last_used ? `Used ${ago(o.last_used, now)}` : "Last use not known"),
        h("span", { class: "hm-size" }, human(c.bytes)),
        h("span", { class: "hm-go", attrs: { "aria-hidden": "true" } }, "›"))));
    }
    return [h("p", { class: "lead" }, intro), ul];
  }

  private refresh(): void {
    if (this.last) {
      const { tree, now } = this.last;
      this.last = null;
      this.show(tree, now, true);
    }
  }
}

// The simple view: a folder's contents as a sortable list, biggest first, like a file manager.
// Only the rows on screen are drawn, so a folder with 100,000 entries scrolls as fast as one with 10.

import { KIND } from "../core/format";
import type { Tree } from "../core/tree";
import { count, date, human } from "../core/util";
import { lookOf, type ColorMode } from "../graph/encode";
import type { Palette } from "../graph/style";
import { clear, h } from "./dom";
import { shapeIcon } from "./icons";
import { RISK_LABELS } from "./panels";

const ROW_H = 34;

export type SortKey = "size" | "name" | "changed" | "items";

export interface ExplorerHandlers {
  select(node: number | null): void;
  open(node: number): void;
  up(): void;
  context(node: number, x: number, y: number): void;
}

export class ExplorerView {
  private head: HTMLElement;
  private cols: HTMLElement;
  private scroller: HTMLElement;
  private spacer: HTMLElement;
  private rowsEl: HTMLElement;
  private rows: number[] = [];
  private tree: Tree | null = null;
  private folder = -1;
  private selected: number | null = null;
  private sort: SortKey = "size";
  private desc = true;
  private sortedFor = "";
  /** Rows already built, reused while only the selection changes (so a double-click hits the same row). */
  private built = new Map<number, HTMLElement>();
  private builtFor = "";
  private pal: Palette | null = null;
  private mode: ColorMode = "risk";

  constructor(host: HTMLElement, private readonly on: ExplorerHandlers) {
    host.classList.add("explorer");
    this.head = h("header", { class: "x-head" });
    this.cols = h("div", { class: "x-cols", attrs: { role: "row" } });
    this.scroller = h("div", { class: "x-scroll", attrs: { tabindex: "0", role: "grid", "aria-label": "Folder contents" } });
    this.spacer = h("div", { class: "x-spacer" });
    this.rowsEl = h("div", { class: "x-rows" });
    this.scroller.append(this.spacer, this.rowsEl);
    host.append(this.head, this.cols, this.scroller);
    this.scroller.addEventListener("scroll", () => this.drawRows());
    this.scroller.addEventListener("keydown", (e) => this.key(e));
    this.scroller.addEventListener("click", (e) => {
      if (e.target === this.scroller || e.target === this.spacer || e.target === this.rowsEl) this.on.select(null);
    });
    new ResizeObserver(() => this.drawRows()).observe(this.scroller);
  }

  /** Show `folder` of `tree`. Cheap to call often (re-sorts only when the data changed). */
  show(tree: Tree, folder: number, selected: number | null, pal: Palette, mode: ColorMode): void {
    const moved = folder !== this.folder || tree !== this.tree;
    this.tree = tree;
    this.folder = folder;
    this.selected = selected;
    this.pal = pal;
    this.mode = mode;
    this.sortRows();
    if (moved) this.scroller.scrollTop = 0;
    this.drawHead();
    this.drawCols();
    this.drawRows();
  }

  /** Scroll so `node` (a child of the folder) is visible. */
  reveal(node: number): void {
    const i = this.rows.indexOf(node);
    if (i < 0) return;
    const top = i * ROW_H;
    const view = this.scroller.clientHeight;
    if (top < this.scroller.scrollTop || top + ROW_H > this.scroller.scrollTop + view) this.scroller.scrollTop = Math.max(0, top - view / 3);
    this.drawRows();
  }

  focus(): void {
    this.scroller.focus({ preventScroll: true });
  }

  private sortRows(): void {
    const t = this.tree;
    if (!t || this.folder < 0) return;
    const key = `${this.folder}:${t.version}:${this.sort}:${this.desc}`;
    if (key === this.sortedFor) return;
    this.sortedFor = key;
    const rows: number[] = [];
    for (let c = t.firstChild[this.folder]; c >= 0; c = t.nextSibling[c]) rows.push(c);
    const dir = this.desc ? -1 : 1;
    const by: Record<SortKey, (a: number, b: number) => number> = {
      size: (a, b) => (t.agg[a] - t.agg[b]) * dir,
      name: (a, b) => t.names[a].localeCompare(t.names[b], undefined, { numeric: true, sensitivity: "base" }) * dir,
      changed: (a, b) => ((t.kind[a] === KIND.dir ? t.newest[a] : t.mtime[a]) - (t.kind[b] === KIND.dir ? t.newest[b] : t.mtime[b])) * dir,
      items: (a, b) => (t.files[a] - t.files[b]) * dir,
    };
    // Folders before files when sorting by name, as file managers do.
    rows.sort((a, b) => (this.sort === "name" ? (t.kind[b] === KIND.dir ? 1 : 0) - (t.kind[a] === KIND.dir ? 1 : 0) : 0) || by[this.sort](a, b) || a - b);
    this.rows = rows;
    this.spacer.style.height = `${rows.length * ROW_H}px`;
  }

  private drawHead(): void {
    const t = this.tree;
    if (!t) return;
    const f = this.folder;
    clear(this.head);
    const name = f === 0 ? "This PC" : t.names[f].replace(/[\\/]+$/, "") || t.names[f];
    const up = h("button", { class: "x-up", title: "Up one folder (Backspace)", onclick: () => this.on.up(), attrs: { "aria-label": "Up one folder" } }, "Up");
    if (f === 0) up.setAttribute("disabled", "");
    const total = t.agg[0] || 1;
    const bar = h("span", { class: "x-share" }, h("span", {}));
    (bar.firstElementChild as HTMLElement).style.width = `${Math.max(0.5, (t.agg[f] / total) * 100)}%`;
    this.head.append(
      up,
      h("div", { class: "x-title" }, h("h2", {}, name), h("span", { class: "path" }, f === 0 || t.path(f).replace(/[\\/]+$/, "") === name ? "" : t.path(f))),
      h("div", { class: "x-sum" }, h("b", {}, human(t.agg[f])), h("span", {}, `${count(t.files[f])} files, ${count(t.childCount[f])} items here`), bar),
    );
  }

  private drawCols(): void {
    clear(this.cols);
    const col = (k: SortKey | null, label: string, cls: string): HTMLElement => {
      if (!k) return h("span", { class: cls }, label);
      const on = this.sort === k;
      const b = h("button", { class: `${cls}${on ? " on" : ""}`, attrs: { "aria-sort": on ? (this.desc ? "descending" : "ascending") : "none" }, onclick: () => {
        if (this.sort === k) this.desc = !this.desc;
        else {
          this.sort = k;
          this.desc = k !== "name";
        }
        this.sortedFor = "";
        this.sortRows();
        this.drawCols();
        this.drawRows();
      } }, label, on ? (this.desc ? " ↓" : " ↑") : "");
      return b;
    };
    this.cols.append(col("name", "Name", "c-name"), col("size", "Size", "c-size"), h("span", { class: "c-share" }, "Of this folder"), col("items", "Files", "c-items"), col("changed", "Changed", "c-date"), h("span", { class: "c-risk" }, "Verdict"), h("span", { class: "c-owner" }, "Belongs to"));
  }

  private drawRows(): void {
    const t = this.tree;
    const p = this.pal;
    if (!t || !p) return;
    const top = this.scroller.scrollTop;
    const first = Math.max(0, Math.floor(top / ROW_H) - 4);
    const last = Math.min(this.rows.length, Math.ceil((top + this.scroller.clientHeight) / ROW_H) + 4);
    const stamp = `${this.sortedFor}:${t.version}:${p.accent}:${this.mode}`;
    if (stamp !== this.builtFor) {
      this.builtFor = stamp;
      this.built.clear();
      clear(this.rowsEl);
    }
    const folderSize = Math.max(1, t.agg[this.folder] - t.own[this.folder]);
    if (this.rows.length === 0) {
      clear(this.rowsEl);
      this.rowsEl.append(h("p", { class: "x-empty" }, t.kind[this.folder] === KIND.dir ? "This folder is empty, or its contents could not be read." : "Not a folder."));
      return;
    }
    const keep = new Set<number>();
    for (let k = first; k < last; k++) {
      const i = this.rows[k];
      keep.add(i);
      const old = this.built.get(i);
      if (old) {
        old.classList.toggle("on", i === this.selected);
        old.setAttribute("aria-selected", String(i === this.selected));
        continue;
      }
      const isDir = t.kind[i] === KIND.dir;
      const risk = t.riskOf(i);
      const owner = t.owner[i];
      const share = t.agg[i] / folderSize;
      const bar = h("span", { class: "x-bar" }, h("span", {}));
      (bar.firstElementChild as HTMLElement).style.width = `${Math.max(share > 0 ? 1 : 0, share * 100)}%`;
      const row = h("div", { class: `x-row${i === this.selected ? " on" : ""}${isDir ? " dir" : ""}`, attrs: { role: "row", "data-node": String(i), "aria-selected": String(i === this.selected) } },
        h("span", { class: "c-name" }, shapeIcon(lookOf(t, i, false, false, this.mode, p), 13), h("span", { class: "nm" }, t.names[i]), isDir && t.childCount[i] > 0 ? h("span", { class: "chev", attrs: { "aria-hidden": "true" } }, "›") : null),
        h("span", { class: "c-size" }, human(t.agg[i])),
        h("span", { class: "c-share" }, bar, h("span", { class: "pct" }, share >= 0.001 ? `${(share * 100).toFixed(share < 0.1 ? 1 : 0)}%` : "")),
        h("span", { class: "c-items" }, isDir ? count(t.files[i]) : ""),
        h("span", { class: "c-date" }, date(isDir ? t.newest[i] : t.mtime[i])),
        h("span", { class: `c-risk${risk ? ` r-${risk}` : ""}` }, risk ? RISK_LABELS[risk] : ""),
        h("span", { class: "c-owner" }, owner >= 0 ? t.owners[owner].name : ""),
      );
      row.style.transform = `translateY(${k * ROW_H}px)`;
      row.addEventListener("click", () => this.on.select(i));
      row.addEventListener("dblclick", () => {
        if (isDir && t.childCount[i] > 0) this.on.open(i);
      });
      row.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        this.on.context(i, e.clientX, e.clientY);
      });
      this.rowsEl.append(row);
      this.built.set(i, row);
    }
    for (const [i, el] of this.built) {
      if (keep.has(i)) continue;
      el.remove();
      this.built.delete(i);
    }
  }

  private key(e: KeyboardEvent): void {
    const t = this.tree;
    if (!t) return;
    const at = this.selected !== null ? this.rows.indexOf(this.selected) : -1;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(0, Math.min(this.rows.length - 1, at + (e.key === "ArrowDown" ? 1 : -1)));
      if (this.rows[next] !== undefined) {
        this.on.select(this.rows[next]);
        this.reveal(this.rows[next]);
      }
    } else if (e.key === "Enter" && this.selected !== null) {
      if (t.kind[this.selected] === KIND.dir && t.childCount[this.selected] > 0) this.on.open(this.selected);
    } else if (e.key === "Backspace") {
      e.preventDefault();
      this.on.up();
    }
  }
}

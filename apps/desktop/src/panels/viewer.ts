// The preview window (D143): plays videos and music, shows pictures, PDFs and text, with
// Previous / Next through a list (a folder's files, or a group's biggest files). Read-only:
// files come from the desktop app's `rzview` addresses, one file at a time.

import { viewKind } from "../core/view";
import type { Tree } from "../core/tree";
import { human } from "../core/util";
import { viewOpen } from "../platform";
import { clear, h } from "./dom";

const TEXT_MAX = 512 * 1024;

export class Viewer {
  private dlg: HTMLDialogElement;
  private stage: HTMLElement;
  private title: HTMLElement;
  private info: HTMLElement;
  private pos: HTMLElement;
  private prev: HTMLButtonElement;
  private next: HTMLButtonElement;
  private list: number[] = [];
  private at = 0;
  private tree: Tree | null = null;
  private seq = 0;

  /** `locate` shows a file in Files (the window closes). */
  constructor(private locate: (node: number) => void) {
    this.title = h("h2", {});
    this.info = h("p", { class: "quiet vw-info" });
    this.stage = h("div", { class: "vw-stage" });
    this.pos = h("span", { class: "quiet" });
    this.prev = h("button", { attrs: { "aria-label": "Previous" }, onclick: () => this.go(-1) }, "← Previous") as HTMLButtonElement;
    this.next = h("button", { attrs: { "aria-label": "Next" }, onclick: () => this.go(1) }, "Next →") as HTMLButtonElement;
    const close = h("button", { class: "vw-x", attrs: { "aria-label": "Close" }, onclick: () => this.close() }, "×");
    const where = h("button", { class: "textlink", onclick: () => { const n = this.list[this.at]; this.close(); if (n !== undefined) this.locate(n); } }, "Show in Files");
    this.dlg = h("dialog", { class: "vw", attrs: { "aria-label": "Preview" } },
      h("header", {}, h("div", { class: "vw-t" }, this.title, this.info), close),
      this.stage,
      h("footer", {}, this.prev, this.pos, this.next, h("span", { class: "vw-sp" }), where)) as HTMLDialogElement;
    this.dlg.addEventListener("keydown", (e) => {
      if (e.target instanceof HTMLInputElement) return;
      if (e.key === "ArrowLeft") { e.preventDefault(); this.go(-1); }
      if (e.key === "ArrowRight") { e.preventDefault(); this.go(1); }
    });
    this.dlg.addEventListener("close", () => this.stopMedia());
    document.body.append(this.dlg);
  }

  /** Show `list[at]` (files of `tree`), with Previous / Next through the list. */
  open(tree: Tree, list: number[], at: number): void {
    this.tree = tree;
    this.list = list.filter((n) => viewKind(tree.names[n]) !== null);
    this.at = Math.max(0, this.list.indexOf(list[at] ?? -1));
    if (!this.list.length) return;
    if (!this.dlg.open) this.dlg.showModal();
    void this.show();
  }

  close(): void {
    this.stopMedia();
    if (this.dlg.open) this.dlg.close();
  }

  private stopMedia(): void {
    this.stage.querySelectorAll("video, audio").forEach((m) => (m as HTMLMediaElement).pause());
    clear(this.stage);
  }

  private go(step: number): void {
    if (!this.list.length) return;
    const n = this.at + step;
    if (n < 0 || n >= this.list.length) return;
    this.at = n;
    void this.show();
  }

  private async show(): Promise<void> {
    const tree = this.tree;
    const node = this.list[this.at];
    if (!tree || node === undefined) return;
    const mine = ++this.seq;
    this.stopMedia();
    const path = tree.path(node);
    this.title.textContent = tree.names[node];
    this.info.textContent = `${human(tree.agg[node])} · ${path}`;
    this.pos.textContent = this.list.length > 1 ? `${this.at + 1} of ${this.list.length}` : "";
    this.prev.disabled = this.at === 0;
    this.next.disabled = this.at >= this.list.length - 1;
    this.prev.hidden = this.next.hidden = this.list.length < 2;
    this.stage.append(h("p", { class: "quiet vw-msg" }, "Opening…"));
    let o;
    try {
      o = await viewOpen(path);
    } catch (e) {
      if (mine !== this.seq) return;
      clear(this.stage);
      this.stage.append(h("p", { class: "vw-msg" }, String(e).replace(/^Error: /, "")));
      return;
    }
    if (mine !== this.seq) return;
    clear(this.stage);
    const cannot = (what: string): void => {
      clear(this.stage);
      this.stage.append(h("p", { class: "vw-msg" }, `Windows cannot ${what} this file inside Rhizome (some formats, like MKV, AVI or HEIC, need their own app). Use Show in Files, then open it from there.`));
    };
    if (o.kind === "video") {
      const v = h("video", { attrs: { controls: "", autoplay: "", preload: "metadata", src: o.url } }) as HTMLVideoElement;
      v.addEventListener("error", () => cannot("play"));
      this.stage.append(v);
    } else if (o.kind === "music") {
      const a = h("audio", { attrs: { controls: "", autoplay: "", src: o.url } }) as HTMLAudioElement;
      a.addEventListener("error", () => cannot("play"));
      this.stage.append(h("div", { class: "vw-music" }, h("span", { class: "vw-note", attrs: { "aria-hidden": "true" } }, "♪"), a));
    } else if (o.kind === "picture") {
      const img = h("img", { attrs: { src: o.url, alt: tree.names[node] } }) as HTMLImageElement;
      img.addEventListener("error", () => cannot("show"));
      this.stage.append(img);
    } else if (o.kind === "pdf") {
      this.stage.append(h("iframe", { attrs: { src: o.url, title: tree.names[node] } }));
    } else {
      try {
        const res = await fetch(o.url);
        const text = await res.text();
        if (mine !== this.seq) return;
        this.stage.append(h("pre", { class: "vw-text" }, text));
        if (o.size > TEXT_MAX) this.stage.append(h("p", { class: "quiet" }, `Showing the first ${human(TEXT_MAX)} of ${human(o.size)}.`));
      } catch {
        cannot("show");
      }
    }
  }
}

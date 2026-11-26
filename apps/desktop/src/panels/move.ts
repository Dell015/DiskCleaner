// Moving an app to another drive (B11, D141): the window that checks, moves and then asks
// "does it work?", and the list of moved apps. What may move, and how, is decided by the desktop
// app's Rust side from Windows' list right now; this page only shows it and asks.

import { count, human } from "../core/util";
import { listMoves, moveBack, moveCheck, moveConfirm, moveOptions, moveRecover, moveStart, type MoveAsk, type MoveInfo, type MoveOptions, type MoveTarget } from "../platform";
import { clear, h } from "./dom";

/** Waiting for the person's check, or stopped in the middle. */
export function needsAttention(m: MoveInfo): boolean {
  return m.state === "waiting" || m.state === "moving" || m.state === "moving_back";
}

const STATE_TEXT: Record<MoveInfo["state"], string> = {
  moving: "Rhizome stopped during the move",
  waiting: "Waiting for your check",
  done: "Moved",
  moving_back: "Rhizome stopped while moving it back",
  moved_back: "Moved back",
  failed: "Not moved",
};

export class MoveWindow {
  private dlg: HTMLDialogElement;
  private body: HTMLElement;
  private busy = false;

  /** `changed` is called after anything moved (to redraw Home and the Library). */
  constructor(private changed: () => void) {
    this.dlg = h("dialog", { class: "mv", attrs: { "aria-label": "Move to another drive" } }) as HTMLDialogElement;
    this.body = h("div", { class: "mv-body" });
    this.dlg.append(this.body);
    this.dlg.addEventListener("cancel", (e) => {
      if (this.busy) e.preventDefault();
    });
    document.body.append(this.dlg);
  }

  private frame(title: string, ...content: (HTMLElement | null)[]): void {
    clear(this.body);
    const close = h("button", { class: "mv-x", attrs: { "aria-label": "Close" }, onclick: () => this.close() }, "×");
    if (this.busy) close.setAttribute("disabled", "");
    this.body.append(h("header", {}, h("h2", {}, title), close), ...content.filter((c): c is HTMLElement => c !== null));
    if (!this.dlg.open) this.dlg.showModal();
  }

  close(): void {
    if (this.busy) return;
    this.dlg.close();
  }

  /** Start: ask the Rust side how this app moves and where it could go. */
  async open(ask: MoveAsk): Promise<void> {
    this.frame(`Move ${ask.name}`, h("p", { class: "quiet" }, "Checking how it can move…"));
    let opts: MoveOptions;
    try {
      opts = await moveOptions(ask);
    } catch (e) {
      this.frame(`Move ${ask.name}`, h("p", { class: "warn" }, String(e)), this.buttons(h("button", { onclick: () => this.close() }, "Close")));
      return;
    }
    const how = opts.how;
    if (how.how === "never") {
      this.frame(`Move ${ask.name}`, h("p", { class: "lead" }, "Rhizome does not move this app."), h("p", {}, `${how.why}.`), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
      return;
    }
    if (how.how === "guide") {
      this.frame(`Move ${ask.name}`,
        h("p", { class: "lead" }, `${how.by} moves it itself. Rhizome does not touch it, so ${how.by}'s records stay right:`),
        h("ol", { class: "mv-steps" }, ...how.steps.map((s) => h("li", {}, s))),
        this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
      return;
    }
    this.choose(ask, opts, how);
  }

  private buttons(...b: HTMLElement[]): HTMLElement {
    return h("div", { class: "mv-buttons" }, ...b);
  }

  /** Pick the drive and folder; every check runs again on each change. */
  private choose(ask: MoveAsk, opts: MoveOptions, how: Extract<MoveOptions["how"], { how: "move" }>): void {
    const usable = opts.targets.filter((t) => !t.why);
    if (!opts.targets.length) {
      this.frame(`Move ${ask.name}`, h("p", { class: "lead" }, "There is no other drive to move it to."), h("p", { class: "quiet" }, "Connect the drive (an internal or USB drive formatted NTFS), then try again."), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
      return;
    }
    let target: MoveTarget | undefined = usable.sort((a, b) => b.free - a.free)[0];
    const input = h("input", { class: "mv-dest", attrs: { type: "text", spellcheck: "false", "aria-label": "New folder" } }) as HTMLInputElement;
    input.value = target?.dest ?? "";
    const result = h("div", { class: "mv-check", attrs: { "aria-live": "polite" } });
    const go = h("button", { class: "primary", attrs: { disabled: "" } }, "Move");
    const drives = h("div", { class: "mv-drives", attrs: { role: "radiogroup", "aria-label": "Drive" } });
    for (const t of opts.targets) {
      const b = h("button", { class: `mv-drive${t === target ? " on" : ""}`, attrs: { role: "radio", "aria-checked": String(t === target), ...(t.why ? { disabled: "" } : {}) }, onclick: () => {
        target = t;
        input.value = t.dest;
        drives.querySelectorAll(".mv-drive").forEach((x) => { x.classList.toggle("on", x === b); x.setAttribute("aria-checked", String(x === b)); });
        void check();
      } },
      h("b", {}, `${t.drive}:`), h("span", {}, t.why || `${human(t.free)} free of ${human(t.total)}${t.removable ? " · USB / removable" : ""}`));
      drives.append(b);
    }
    let seq = 0;
    let checked = false;
    const check = async (): Promise<void> => {
      const mine = ++seq;
      checked = false;
      go.setAttribute("disabled", "");
      clear(result);
      result.append(h("p", { class: "quiet" }, "Checking…"));
      try {
        const c = await moveCheck({ ...ask, dest: input.value.trim() });
        if (mine !== seq) return;
        clear(result);
        if (!c.ok) {
          result.append(h("p", { class: "warn" }, `Cannot move it there: ${c.why}.`));
          return;
        }
        checked = true;
        go.removeAttribute("disabled");
        go.textContent = `Move ${human(c.bytes)}`;
        result.append(h("p", {}, `${human(c.bytes)} in ${count(c.files)} files will be copied. ${human(c.free)} is free there.`));
        if (c.removable) result.append(h("p", { class: "warn" }, `This is a USB or removable drive: if it is unplugged, ${ask.name} will not start until it is plugged in again.`));
      } catch (e) {
        if (mine === seq) {
          clear(result);
          result.append(h("p", { class: "warn" }, String(e)));
        }
      }
    };
    let timer = 0;
    input.addEventListener("input", () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => void check(), 350);
    });
    go.addEventListener("click", () => {
      if (checked) void this.run({ ...ask, dest: input.value.trim() }, how);
    });
    const next = how.link
      ? ["Rhizome copies it to the new folder and checks every byte.", `The old folder waits in Rhizome's holding folder, and a link takes its place, so shortcuts, updates and the uninstaller keep working.`, `You open ${ask.name} and check it works.`, "It works: the old copy is deleted and the space is free. If not: Move it back puts everything as it was."]
      : ["Rhizome copies it to the new folder and checks every byte.", "The old folder waits in Rhizome's holding folder.", "You tell the launcher the new place (the steps are shown next), and start the game.", "It works: the old copy is deleted and the space is free. If not: Move it back puts everything as it was."];
    this.frame(`Move ${ask.name}`,
      h("p", { class: "quiet path" }, how.folder),
      h("h3", {}, "To which drive?"), drives,
      h("label", { class: "mv-label" }, "New folder", input),
      result,
      how.warnings.length ? h("ul", { class: "mv-warn" }, ...how.warnings.map((w) => h("li", {}, w))) : null,
      h("details", { class: "mv-how" }, h("summary", {}, "What happens"), h("ol", {}, ...next.map((s) => h("li", {}, s)))),
      h("p", { class: "quiet" }, `Close ${ask.name} first. Rhizome never closes programs.`),
      this.buttons(h("button", { onclick: () => this.close() }, "Cancel"), go));
    if (target) void check();
  }

  /** Copy, check, hold, link; then ask for the check. */
  private async run(ask: MoveAsk, how: Extract<MoveOptions["how"], { how: "move" }>): Promise<void> {
    this.busy = true;
    const bar = h("div", { class: "mv-bar", attrs: { role: "progressbar", "aria-valuemin": "0", "aria-valuemax": "100" } }, h("span", {}));
    const label = h("p", {}, "Starting…");
    this.frame(`Moving ${ask.name}`, label, bar, h("p", { class: "quiet" }, "Keep the PC on until it finishes. The original is not touched until the copy is checked."));
    try {
      const info = await moveStart(ask, (step, done, total) => {
        const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0;
        (bar.firstElementChild as HTMLElement).style.width = `${pct}%`;
        bar.setAttribute("aria-valuenow", String(Math.round(pct)));
        label.textContent = step === "copy" ? `Copying: ${human(done)} of ${human(total)}` : `Checking every byte: ${human(done)} of ${human(total)}`;
      });
      this.busy = false;
      this.changed();
      this.waiting(info, how.steps);
    } catch (e) {
      this.busy = false;
      this.changed();
      this.frame(`${ask.name} was not moved`, h("p", {}, `${String(e).replace(/^Error: /, "")}.`), h("p", { class: "quiet" }, "Everything is where it was."), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
    }
  }

  /** After a move: open the app, then It works or Move it back. */
  waiting(info: MoveInfo, steps: string[] = []): void {
    this.frame(`Moved ${info.app}. Does it work?`,
      h("p", {}, `${info.app} is now at `, h("b", { class: "path" }, info.dest), info.link ? ". Its old folder is a link to it." : "."),
      steps.length ? h("div", {}, h("p", { class: "lead" }, "Now tell the launcher where it is:"), h("ol", { class: "mv-steps" }, ...steps.map((s) => h("li", {}, s)))) : h("p", {}, `Open ${info.app} the way you always do and check it works.`),
      h("p", { class: "quiet" }, `Until you answer, the old copy (${human(info.bytes)}) waits in Rhizome's holding folder. Home reminds you.`),
      this.buttons(
        h("button", { onclick: () => this.close() }, "Later"),
        h("button", { onclick: () => void this.back(info) }, "Move it back"),
        h("button", { class: "primary", onclick: () => void this.confirm(info) }, "It works")));
  }

  private async confirm(info: MoveInfo): Promise<void> {
    this.busy = true;
    this.frame(`Deleting the old copy of ${info.app}`, h("p", {}, h("span", { class: "spinner", attrs: { "aria-hidden": "true" } }), " Deleting…"));
    try {
      const done = await moveConfirm(info.id);
      this.busy = false;
      this.changed();
      this.frame(`${done.app} is moved`, h("p", {}, `${human(done.bytes)} is free on the old drive. ${done.app} runs from `, h("b", { class: "path" }, done.dest), "."), h("p", { class: "quiet" }, "You can still move it back later (Home → Moved apps)."), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "Done")));
    } catch (e) {
      this.busy = false;
      this.frame(`The old copy was not deleted`, h("p", { class: "warn" }, `${String(e).replace(/^Error: /, "")}.`), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
    }
  }

  async back(info: MoveInfo): Promise<void> {
    this.busy = true;
    const label = h("p", {}, "Moving it back…");
    const bar = h("div", { class: "mv-bar" }, h("span", {}));
    this.frame(`Moving ${info.app} back`, label, bar);
    try {
      const done = await moveBack(info.id, (step, d, t) => {
        (bar.firstElementChild as HTMLElement).style.width = `${t > 0 ? Math.min(100, (d / t) * 100) : 0}%`;
        label.textContent = step === "copy" ? `Copying back: ${human(d)} of ${human(t)}` : `Checking every byte: ${human(d)} of ${human(t)}`;
      });
      this.busy = false;
      this.changed();
      this.frame(`${done.app} is back`, h("p", {}, `${done.app} is at `, h("b", { class: "path" }, done.source), " again, as before. The copy on the other drive is deleted."),
        done.link ? null : h("p", { class: "quiet" }, "If you told the launcher the new place, tell it the old one again (Locate game)."),
        ...done.notes.map((n) => h("p", { class: "warn" }, n)),
        this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "Done")));
    } catch (e) {
      this.busy = false;
      this.changed();
      this.frame(`${info.app} was not moved back`, h("p", { class: "warn" }, `${String(e).replace(/^Error: /, "")}.`), h("p", { class: "quiet" }, `It still runs from ${info.dest}.`), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
    }
  }

  /** Every move, with what can be done now. */
  async list(): Promise<void> {
    let moves: MoveInfo[] = [];
    try {
      moves = await listMoves();
    } catch (e) {
      this.frame("Moved apps", h("p", { class: "warn" }, String(e)));
      return;
    }
    const ul = h("ul", { class: "mv-list" });
    for (const m of moves) {
      const when = new Date(m.created * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
      const acts = h("div", { class: "mv-acts" });
      if (m.state === "waiting") acts.append(h("button", { onclick: () => this.waiting(m) }, "Check it"));
      if (m.state === "done") acts.append(h("button", { onclick: () => void this.back(m) }, "Move it back"));
      if (m.state === "moving" || m.state === "moving_back") acts.append(h("button", { class: "primary", onclick: () => void this.recover(m) }, "Finish safely"));
      ul.append(h("li", {},
        h("div", {}, h("b", {}, m.app), h("span", { class: `mv-state s-${m.state}` }, STATE_TEXT[m.state])),
        h("p", { class: "quiet path" }, `${m.source} → ${m.dest}`),
        h("p", { class: "quiet" }, `${when} · ${human(m.bytes)}${m.why ? ` · ${m.why}` : ""}`),
        ...m.notes.map((n) => h("p", { class: "warn" }, n)),
        acts));
    }
    this.frame("Moved apps", moves.length ? ul : h("p", { class: "quiet" }, "No app has been moved yet. Open an app in the Library and press Move to another drive."), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "Close")));
  }

  private async recover(m: MoveInfo): Promise<void> {
    this.busy = true;
    this.frame(`Finishing ${m.app}`, h("p", {}, "Finishing safely…"));
    try {
      const said = await moveRecover(m.id);
      this.busy = false;
      this.changed();
      this.frame(`${m.app}`, h("p", {}, `Done: ${said}.`), this.buttons(h("button", { class: "primary", onclick: () => void this.list() }, "OK")));
    } catch (e) {
      this.busy = false;
      this.frame(`${m.app}`, h("p", { class: "warn" }, String(e)), this.buttons(h("button", { class: "primary", onclick: () => this.close() }, "OK")));
    }
  }
}

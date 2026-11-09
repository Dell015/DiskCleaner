// The graph view app: loads a view file, grows the graph like a scan, and wires the controls.
// Read-only: nothing here can delete, move or change a file.

import "@fontsource-variable/atkinson-hyperlegible-next";
import "@fontsource-variable/atkinson-hyperlegible-mono";
import { appendRange, emptyTree } from "./core/build";
import { KIND, type ViewData } from "./core/format";
import { DEFAULT_LOD, nodeKey, selectVisible, type Visible } from "./core/lod";
import { search } from "./core/search";
import type { Tree } from "./core/tree";
import { count, human } from "./core/util";
import type { LoaderReply, LoaderRequest } from "./loader.worker";
import { byId, clear, h } from "./panels/dom";
import { CATEGORY_LABELS, RISK_LABELS, renderApps, renderDetails, renderFindings, renderKey, type FindingFilter, type PanelActions } from "./panels/panels";
import { type ColorMode } from "./graph/encode";
import { GraphView, radiusFor } from "./graph/graph";
import { matchPreset, prefersReducedMotion, PRESET_NAMES, PRESETS, type Quality, type QualityId } from "./graph/quality";
import { paletteOf, STYLES, type StyleId } from "./graph/style";

/** A folder this big on screen (radius, px) opens when you zoom in; below CLOSE_PX it closes again. */
const OPEN_PX = 24;
const CLOSE_PX = 9;
const SYNC_MS = 140;
const REPLAY_SECONDS = [24, 14, 8, 4, 2];

type Tab = "findings" | "apps";
type Theme = "dark" | "light" | "system";

// ---------- settings (kept in this browser only) ----------

interface Settings { style: StyleId; theme: Theme; quality: Quality; chosen: boolean }
function loadSettings(): Settings {
  const base: Settings = { style: "deep", theme: "dark", quality: { ...PRESETS.high }, chosen: false };
  if (prefersReducedMotion()) base.quality.motion = "reduced";
  try {
    const raw = JSON.parse(localStorage.getItem("dc-settings") ?? "null") as Partial<Settings> | null;
    if (raw && typeof raw === "object") {
      if (raw.style && raw.style in STYLES) base.style = raw.style;
      if (raw.theme === "dark" || raw.theme === "light" || raw.theme === "system") base.theme = raw.theme;
      if (raw.quality && typeof raw.quality === "object") base.quality = { ...base.quality, ...raw.quality };
      base.chosen = Boolean(raw.chosen);
    }
  } catch {
    /* storage may be unavailable; defaults are used */
  }
  const params = new URLSearchParams(location.search);
  const st = params.get("style");
  if (st && st in STYLES) base.style = st as StyleId;
  const th = params.get("theme");
  if (th === "dark" || th === "light" || th === "system") base.theme = th;
  const q = params.get("quality");
  if (q && q in PRESETS) base.quality = { ...PRESETS[q as keyof typeof PRESETS] };
  return base;
}
const settings = loadSettings();
function saveSettings(): void {
  try {
    localStorage.setItem("dc-settings", JSON.stringify(settings));
  } catch {
    /* ignore */
  }
}
function isDark(): boolean {
  return settings.theme === "dark" || (settings.theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
}

// ---------- state ----------

const state = {
  data: null as ViewData | null,
  tree: null as Tree | null,
  vis: null as Visible | null,
  replay: { next: 0, running: false, paused: false, last: 0, lastSync: 0 },
  opened: new Set<number>(),
  zoomOpened: new Set<number>(),
  closed: new Set<number>(),
  wide: new Set<number>(),
  selected: null as string | null,
  focusRoot: null as number | null,
  tab: "findings" as Tab,
  filter: "offered" as FindingFilter,
  findingsShown: 60,
  owner: -1,
  hitIndex: -1,
};

const app = byId("app");
let userMoved = false;
const view = new GraphView(byId("graph"), {
  select: (key) => select(key),
  toggle: (key) => toggle(key),
  hover: (key, x, y) => tip(key, x, y),
  cameraMoved: () => {
    zoomDetail();
    sizeKey();
    drawMinimap();
  },
  context: (key, x, y) => contextMenu(key, x, y),
});
view.setStyle(STYLES[settings.style], isDark());
view.setQuality(settings.quality);
(window as unknown as { __dc: unknown }).__dc = { state, view, settings, refresh: () => refresh(false), reveal: (n: number) => reveal(n) };

// ---------- loading ----------

let worker: Worker | null = null;
function load(req: LoaderRequest): void {
  worker?.terminate();
  worker = new Worker(new URL("./loader.worker.ts", import.meta.url), { type: "module" });
  byId("loading").hidden = false;
  byId("loadingText").textContent = "Loading";
  hideError();
  worker.onmessage = (e: MessageEvent<LoaderReply>) => {
    const m = e.data;
    if (m.type === "progress") byId("loadingText").textContent = m.step;
    else if (m.type === "error") {
      byId("loading").hidden = true;
      showError(m.message);
      worker?.terminate();
      worker = null;
    } else {
      byId("loading").hidden = true;
      worker?.terminate();
      worker = null;
      start(m.data);
    }
  };
  worker.onerror = () => {
    byId("loading").hidden = true;
    showError("The file could not be loaded.");
  };
  worker.postMessage(req);
}

function start(data: ViewData): void {
  state.data = data;
  state.tree = emptyTree(data);
  state.vis = null;
  state.opened.clear();
  state.zoomOpened.clear();
  state.closed.clear();
  state.wide.clear();
  state.selected = null;
  state.focusRoot = null;
  state.owner = -1;
  state.findingsShown = 60;
  state.filter = data.marks.some((m) => m.offered) ? "offered" : data.marks.length ? "shown" : "offered";
  clearFocusTimers();
  closeDrawer();
  view.reset();
  view.setFocusOwner(-1);
  byId("empty").hidden = true;
  byId("minimap").hidden = false;
  userMoved = false;
  state.replay = { next: 0, running: true, paused: false, last: performance.now(), lastSync: 0 };
  frameTimes = [];
  renderSide();
  // The growth plays the first time a file opens; with motion off it is skipped.
  if (settings.quality.motion !== "full") {
    finishReplay();
    return;
  }
  byId("scan").hidden = false;
  byId("pause").textContent = "Pause";
  requestAnimationFrame(replayStep);
}

function replay(): void {
  if (state.data) {
    const d = state.data;
    const motion = settings.quality.motion;
    settings.quality.motion = "full";
    start(d);
    settings.quality.motion = motion;
  }
}

/** Frame times seen while the scan grew: they decide whether graphics should step down. */
let frameTimes: number[] = [];

function replayStep(now: number): void {
  const r = state.replay;
  const data = state.data;
  const tree = state.tree;
  if (!r.running || !data || !tree) return;
  const n = data.names.length;
  if (!r.paused) {
    const dt = Math.min(100, now - r.last);
    const secs = REPLAY_SECONDS[Number(byId<HTMLInputElement>("speed").value) - 1] ?? 8;
    const per = Math.max(1, Math.ceil((n / (secs * 1000)) * dt));
    const to = Math.min(n, r.next + per);
    appendRange(tree, data, r.next, to);
    r.next = to;
  }
  if (!r.paused) frameTimes.push(now - r.last);
  r.last = now;
  if (r.next >= n) {
    finishReplay();
    return;
  }
  if (now - r.lastSync > SYNC_MS && !r.paused) {
    r.lastSync = now;
    refresh(true);
    let p = r.next - 1;
    while (p > 0 && !state.vis?.byKey.has(nodeKey(p))) p = tree.parent[p];
    view.setHead(nodeKey(Math.max(0, p)));
    byId("scanText").textContent = `Scanning: ${count(r.next)} of ${count(n)} entries, ${human(tree.agg[0])}`;
  }
  requestAnimationFrame(replayStep);
}

function finishReplay(): void {
  const r = state.replay;
  const data = state.data;
  const tree = state.tree;
  if (!data || !tree) return;
  if (r.next < data.names.length) appendRange(tree, data, r.next, data.names.length);
  r.next = data.names.length;
  r.running = false;
  byId("scan").hidden = true;
  view.setHead(null);
  refresh(true);
  renderSide();
  settle();
}

/** Once the layout is calm: frame everything (unless the user moved), then hold still. */
function settle(): void {
  const wait = (): void => {
    if (view.layoutRunning() && !userMoved) setTimeout(wait, 300);
    else {
      if (!userMoved) view.fit();
      view.freeze(true);
      view.stable = true;
      autoQuality();
    }
  };
  setTimeout(wait, 600);
}

// ---------- visible set ----------

function pictureRoot(): number {
  const data = state.data;
  const tree = state.tree;
  if (state.focusRoot !== null && tree && state.focusRoot < tree.n) return state.focusRoot;
  if (data && tree && data.roots.length === 1 && tree.n > 1) return 1;
  return 0;
}

function selectedNode(): number | null {
  const k = state.selected;
  return k && k.startsWith("n") ? Number(k.slice(1)) : null;
}

function refresh(animate: boolean): void {
  const tree = state.tree;
  if (!tree || tree.n === 0) return;
  const mustShow = new Set<number>();
  const sel = selectedNode();
  const root = pictureRoot();
  if (sel !== null && (sel === root || tree.isAncestor(root, sel))) mustShow.add(sel);
  const opened = new Set([...state.opened, ...state.zoomOpened]);
  view.rootNode = root;
  state.vis = selectVisible(tree, { ...DEFAULT_LOD, budget: settings.quality.budget }, { opened, closed: state.closed, wide: state.wide, mustShow, root });
  view.sync(tree, state.vis, animate);
  status();
  key();
  sizeKey();
  crumbs();
  drawMinimap();
}

function zoomDetail(): void {
  const tree = state.tree;
  const vis = state.vis;
  if (!tree || !vis || state.replay.running) return;
  let changed = false;
  for (const n of [...state.zoomOpened]) {
    const r = view.pixelRadius(nodeKey(n));
    if (!vis.byKey.has(nodeKey(n)) || r < CLOSE_PX) {
      state.zoomOpened.delete(n);
      changed = true;
    }
  }
  const want: [number, number][] = [];
  for (const item of vis.items) {
    const i = item.node;
    if (i < 0 || item.open || tree.kind[i] !== KIND.dir || tree.childCount[i] === 0 || state.closed.has(i)) continue;
    const r = view.pixelRadius(item.key);
    if (r >= OPEN_PX && view.onScreen(item.key)) want.push([r, i]);
  }
  want.sort((a, b) => b[0] - a[0]);
  for (const [, i] of want.slice(0, 12)) {
    state.zoomOpened.add(i);
    changed = true;
  }
  if (changed) {
    refresh(true);
    if (want.length > 0) setTimeout(zoomDetail, 450);
  }
}

// ---------- selection, drawer, focus ----------

function select(key: string | null): void {
  hideMenu();
  state.selected = key;
  view.setSelected(key);
  if (key) openDrawer();
  else closeDrawer();
  crumbs();
}

function openDrawer(): void {
  renderDrawer();
  const d = byId("drawer");
  d.classList.add("open");
  d.setAttribute("aria-hidden", "false");
  if (window.innerWidth < 900) app.classList.remove("no-side");
}

function closeDrawer(): void {
  const d = byId("drawer");
  d.classList.remove("open");
  d.setAttribute("aria-hidden", "true");
}

function renderDrawer(): void {
  const tree = state.tree;
  if (!tree) return;
  const k = state.selected;
  const item = k ? state.vis?.byKey.get(k) : undefined;
  renderDetails(byId("drawer"), tree, view.pal, view.mode, pictureRoot(), item?.group ? null : selectedNode(), item?.group ?? null, actions);
}

function toggle(key: string): void {
  const vis = state.vis;
  if (!vis) return;
  const item = vis.byKey.get(key);
  if (!item) return;
  if (item.group) {
    state.wide.add(item.group.parent);
    refresh(true);
    return;
  }
  toggleNode(item.node);
}

function toggleNode(i: number): void {
  const tree = state.tree;
  if (!tree || tree.kind[i] !== KIND.dir) return;
  if (state.vis?.open.has(i)) {
    state.closed.add(i);
    state.opened.delete(i);
    state.zoomOpened.delete(i);
    state.wide.delete(i);
    for (const z of [...state.zoomOpened]) if (tree.isAncestor(i, z)) state.zoomOpened.delete(z);
  } else {
    state.closed.delete(i);
    state.opened.add(i);
  }
  refresh(true);
  if (state.selected) renderDrawer();
}

let focusTimers: number[] = [];
function clearFocusTimers(): void {
  for (const t of focusTimers) clearTimeout(t);
  focusTimers = [];
}

/** Make an entry visible (opening the way to it), select it and fly there. */
function reveal(i: number): void {
  if (state.replay.running && state.tree && i >= state.tree.n) finishReplay();
  const tree = state.tree;
  if (!tree || i < 0 || i >= tree.n) return;
  const root = pictureRoot();
  if (i !== root && !tree.isAncestor(root, i)) state.focusRoot = null;
  for (const a of tree.ancestors(i)) state.closed.delete(a);
  state.selected = nodeKey(i);
  refresh(true);
  view.setSelected(state.selected);
  openDrawer();
  crumbs();
  clearFocusTimers();
  focusTimers.push(window.setTimeout(() => view.focus(nodeKey(i)), 120), window.setTimeout(() => view.focus(nodeKey(i)), 700));
}

/** Make a folder the centre of the picture ("local graph"). null goes back to everything. */
function focusOn(i: number | null): void {
  const tree = state.tree;
  if (!tree) return;
  state.focusRoot = i;
  state.zoomOpened.clear();
  state.wide.clear();
  state.selected = i === null ? null : nodeKey(i);
  clearFocusTimers();
  view.reset();
  view.setSelected(null);
  userMoved = false;
  refresh(true);
  if (state.selected) {
    view.setSelected(state.selected);
    openDrawer();
  } else closeDrawer();
  settle();
}

const actions: PanelActions = {
  reveal,
  toggle: toggleNode,
  focusHere: (i) => focusOn(i),
  showMore: (p) => {
    state.wide.add(p);
    state.opened.add(p);
    refresh(true);
  },
  unpin: (i) => {
    view.unpin(nodeKey(i));
    renderDrawer();
  },
  isPinned: (i) => view.isPinned(nodeKey(i)),
  isOpen: (i) => Boolean(state.vis?.open.has(i)),
  focusOwner: (o) => {
    state.owner = o;
    view.setFocusOwner(o);
    renderSide();
  },
  close: () => select(null),
};

// ---------- header: where you are ----------

function crumbs(): void {
  const el = byId("crumbs");
  const tree = state.tree;
  clear(el);
  if (!tree) return;
  const target = selectedNode() ?? pictureRoot();
  const chain = target > 0 ? [...tree.ancestors(target), target].filter((a) => a !== 0) : [];
  const all = h("button", { onclick: () => (state.focusRoot !== null ? focusOn(null) : select(null)) }, "This PC");
  if (chain.length === 0) all.setAttribute("aria-current", "page");
  el.append(all);
  for (const a of chain) {
    el.append(h("span", { class: "sep", attrs: { "aria-hidden": "true" } }, "/"));
    const b = h("button", { onclick: () => reveal(a), title: tree.path(a) }, tree.names[a].replace(/[\\/]+$/, "") || tree.names[a]);
    if (a === target) b.setAttribute("aria-current", "page");
    el.append(b);
  }
  // Long paths: keep the end in view.
  el.scrollLeft = el.scrollWidth;
}

// ---------- side column ----------

function setTab(t: Tab): void {
  state.tab = t;
  document.querySelectorAll<HTMLButtonElement>(".tabs button").forEach((b) => {
    const on = b.dataset.tab === t;
    b.classList.toggle("on", on);
    b.setAttribute("aria-selected", String(on));
  });
  byId("tab-findings").hidden = t !== "findings";
  byId("tab-apps").hidden = t !== "apps";
  renderSide();
}

function renderSide(): void {
  const tree = state.tree;
  if (!tree) return;
  if (state.tab === "findings") {
    renderFindings(byId("tab-findings"), tree, view.pal, state.filter, state.findingsShown, actions, (f) => {
      state.filter = f;
      state.findingsShown = 60;
      renderSide();
    }, () => {
      state.findingsShown += 60;
      renderSide();
    });
  } else renderApps(byId("tab-apps"), tree, view.pal, state.owner, actions);
  if (state.selected) renderDrawer();
}

// ---------- graph overlays: key, size scale, status, minimap, tooltip, menu ----------

function key(): void {
  const tree = state.tree;
  if (!tree) return;
  const p = view.pal;
  const apps: { name: string; color: string }[] = [];
  if (view.mode === "app") {
    const seen = new Map<number, number>();
    for (const item of state.vis?.items ?? []) {
      if (item.node < 0) continue;
      const o = tree.owner[item.node];
      if (o >= 0 && tree.ownerHere[item.node] === o) seen.set(o, (seen.get(o) ?? 0) + tree.agg[item.node]);
    }
    for (const [o] of [...seen.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
      apps.push({ name: tree.owners[o].name, color: tree.owners[o].kind === "windows" ? p.windows : p.apps[o % p.apps.length] });
    }
  }
  renderKey(byId("key"), p, view.mode, CATEGORY_LABELS, apps);
}

/** Two reference dots that say what a size on screen means right now. */
function sizeKey(): void {
  const el = byId("sizekey");
  const tree = state.tree;
  clear(el);
  if (!tree || !state.vis) return;
  const rootBytes = tree.agg[pictureRoot()] || tree.agg[0];
  const upp = view.pixelsPerUnit();
  const M = 1024 * 1024;
  const steps = [M, 10 * M, 100 * M, 1024 * M, 10240 * M, 102400 * M, 1048576 * M];
  const fit = steps.filter((b) => {
    const r = radiusFor(b, rootBytes) * upp;
    return r >= 2.5 && r <= 22 && b <= rootBytes;
  });
  for (const b of fit.slice(-2)) {
    const r = radiusFor(b, rootBytes) * upp;
    const i = h("i");
    i.style.width = i.style.height = `${(r * 2).toFixed(1)}px`;
    el.append(h("span", {}, i, human(b).replace(/\.0+ /, " ")));
  }
}

function status(): void {
  const tree = state.tree;
  const vis = state.vis;
  const el = byId("status");
  clear(el);
  if (!tree || !vis) return;
  el.append(`Showing ${count(vis.items.length)} of ${count(tree.n - 1)} entries, ${human(tree.agg[0])} in all`);
  if (vis.truncated) el.append(". ", h("span", { class: "warn" }, "Detail limit reached: raise Detail in Settings to see more."));
  if (!state.replay.running) {
    el.append(" ");
    const b = h("button", { class: "textlink", onclick: replay }, "Replay scan");
    el.append(b);
  }
}

let miniTimer = 0;
function drawMinimap(): void {
  if (miniTimer) return;
  miniTimer = window.setTimeout(() => {
    miniTimer = 0;
    const c = byId<HTMLCanvasElement>("minimap");
    if (!c.hidden) view.drawMinimap(c);
  }, 120);
}

function tip(k: string | null, x: number, y: number): void {
  const el = byId("tip");
  const tree = state.tree;
  const item = k ? state.vis?.byKey.get(k) : undefined;
  if (!tree || !item) {
    el.hidden = true;
    return;
  }
  clear(el);
  if (item.group) {
    el.append(h("b", {}, `${count(item.group.count)} more items`), h("span", { class: "m" }, `${human(item.group.bytes)}. Double-click to show them.`));
  } else {
    const i = item.node;
    const risk = tree.riskOf(i);
    const o = tree.owner[i];
    const bits = [human(tree.agg[i])];
    if (tree.kind[i] === KIND.dir) bits.push(`${count(tree.files[i])} files`);
    if (o >= 0) bits.push(tree.owners[o].name);
    if (risk) bits.push(RISK_LABELS[risk]);
    el.append(h("b", {}, i === 0 ? "This PC" : tree.names[i]), h("span", { class: "m" }, bits.join(", ")));
    if (tree.kind[i] === KIND.dir && tree.childCount[i] > 0) el.append(h("span", { class: "m" }, item.open ? "Double-click to close." : "Double-click or zoom in to open."));
  }
  const rect = byId("stage").getBoundingClientRect();
  el.hidden = false;
  el.style.left = `${Math.min(rect.left + x + 16, window.innerWidth - 380)}px`;
  el.style.top = `${rect.top + y + 16}px`;
}

function contextMenu(k: string, x: number, y: number): void {
  const tree = state.tree;
  const item = state.vis?.byKey.get(k);
  const menu = byId("menu");
  if (!tree || !item) return;
  clear(menu);
  const add = (label: string, run: () => void): void => {
    menu.append(h("button", { attrs: { role: "menuitem" }, onclick: () => { hideMenu(); run(); } }, label));
  };
  if (item.group) add("Show these items", () => toggle(k));
  else {
    const i = item.node;
    add("Show details", () => select(k));
    if (tree.kind[i] === KIND.dir && tree.childCount[i] > 0) {
      add(item.open ? "Close folder" : "Open folder", () => toggleNode(i));
      if (i !== pictureRoot()) add("Focus here", () => focusOn(i));
    }
    add("Copy path", () => void navigator.clipboard?.writeText(tree.path(i)));
    if (view.isPinned(k)) add("Unpin", () => view.unpin(k));
  }
  const rect = byId("stage").getBoundingClientRect();
  menu.hidden = false;
  menu.style.left = `${Math.min(rect.left + x, window.innerWidth - 200)}px`;
  menu.style.top = `${Math.min(rect.top + y, window.innerHeight - 200)}px`;
  (menu.firstElementChild as HTMLElement | null)?.focus();
}
function hideMenu(): void {
  byId("menu").hidden = true;
}

function showError(message: string): void {
  byId("errorText").textContent = message;
  byId("error").hidden = false;
}
function hideError(): void {
  byId("error").hidden = true;
}
let toastTimer = 0;
function toast(message: string): void {
  const t = byId("toast");
  t.textContent = message;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (t.hidden = true), 6000);
}

// ---------- graphics: automatic step down when frames are slow ----------

function autoQuality(): void {
  if (settings.chosen || frameTimes.length < 60) return;
  const sorted = frameTimes.slice(10).sort((x, y) => x - y);
  frameTimes = [];
  const median = sorted[Math.floor(sorted.length / 2)];
  const order: QualityId[] = ["ultra", "high", "medium", "low"];
  const idx = order.indexOf(matchPreset(settings.quality));
  if (median > 34 && idx >= 0 && idx < order.length - 1) {
    const next = order[idx + 1] as keyof typeof PRESETS;
    settings.quality = { ...PRESETS[next] };
    saveSettings();
    view.setQuality(settings.quality);
    refresh(false);
    toast(`Graphics set to ${PRESET_NAMES[next]} to keep movement smooth. Change it in Settings.`);
  }
}

// ---------- settings dialog ----------

function renderSettings(): void {
  const styles = byId("styles");
  clear(styles);
  for (const st of Object.values(STYLES)) {
    const p = paletteOf(st, isDark());
    const sw = h("span", { class: "swatch" });
    sw.style.background = `radial-gradient(circle at 30% 45%, ${p.folder} 0 5px, transparent 6px), radial-gradient(circle at 55% 35%, ${p.risk.probably_safe} 0 3px, transparent 4px), radial-gradient(circle at 70% 65%, ${p.risk.safe} 0 4px, transparent 5px), radial-gradient(circle at 45% 70%, ${p.accent} 0 2px, transparent 3px), ${p.bg}`;
    styles.append(h("button", { class: settings.style === st.id ? "on" : "", attrs: { type: "button", "aria-pressed": String(settings.style === st.id) }, onclick: () => {
      settings.style = st.id;
      settings.chosen = true;
      applyLook();
    } }, sw, h("span", {}, h("b", {}, st.name), h("span", { class: "s" }, st.summary))));
  }
  document.querySelectorAll<HTMLButtonElement>("#themes button").forEach((b) => b.classList.toggle("on", b.dataset.theme === settings.theme));
  const presets = byId("presets");
  clear(presets);
  const cur = matchPreset(settings.quality);
  for (const id of ["low", "medium", "high", "ultra"] as const) {
    presets.append(h("button", { class: cur === id ? "on" : "", attrs: { type: "button" }, onclick: () => {
      settings.quality = { ...PRESETS[id] };
      settings.chosen = true;
      applyQuality();
    } }, PRESET_NAMES[id]));
  }
  byId("presetNote").textContent = cur === "custom" ? "Custom: your own mix of the options below." : "Pick a preset, or change any option below.";
  const q = settings.quality;
  byId<HTMLInputElement>("q-glow").checked = q.glow;
  byId<HTMLInputElement>("q-blur").checked = q.focusBlur;
  byId<HTMLInputElement>("q-curved").checked = q.curved;
  byId<HTMLInputElement>("q-pulses").checked = q.pulses;
  byId<HTMLSelectElement>("q-motion").value = q.motion;
  byId<HTMLInputElement>("q-budget").value = String(q.budget);
  byId("q-budget-out").textContent = `${count(q.budget)} nodes`;
  byId<HTMLInputElement>("q-labels").value = String(q.labels);
}

function applyLook(): void {
  saveSettings();
  view.setStyle(STYLES[settings.style], isDark());
  document.documentElement.dataset.theme = isDark() ? "dark" : "light";
  renderSettings();
  renderSide();
  key();
  drawMinimap();
}

function applyQuality(): void {
  saveSettings();
  view.setQuality(settings.quality);
  renderSettings();
  refresh(false);
}

function bindSettings(): void {
  const dlg = byId<HTMLDialogElement>("settings");
  byId("settingsBtn").addEventListener("click", () => {
    renderSettings();
    dlg.showModal();
  });
  document.querySelectorAll<HTMLButtonElement>("#themes button").forEach((b) => b.addEventListener("click", () => {
    settings.theme = (b.dataset.theme ?? "dark") as Theme;
    applyLook();
  }));
  const bool = (id: string, k: "glow" | "focusBlur" | "curved" | "pulses"): void => {
    byId<HTMLInputElement>(id).addEventListener("change", (e) => {
      settings.quality = { ...settings.quality, [k]: (e.target as HTMLInputElement).checked };
      settings.chosen = true;
      applyQuality();
    });
  };
  bool("q-glow", "glow");
  bool("q-blur", "focusBlur");
  bool("q-curved", "curved");
  bool("q-pulses", "pulses");
  byId<HTMLSelectElement>("q-motion").addEventListener("change", (e) => {
    settings.quality = { ...settings.quality, motion: (e.target as HTMLSelectElement).value as Quality["motion"] };
    settings.chosen = true;
    applyQuality();
  });
  byId<HTMLInputElement>("q-budget").addEventListener("change", (e) => {
    settings.quality = { ...settings.quality, budget: Number((e.target as HTMLInputElement).value) };
    settings.chosen = true;
    applyQuality();
  });
  byId<HTMLInputElement>("q-labels").addEventListener("change", (e) => {
    settings.quality = { ...settings.quality, labels: Number((e.target as HTMLInputElement).value) };
    settings.chosen = true;
    applyQuality();
  });
  byId("savePic").addEventListener("click", () => {
    const url = view.snapshot();
    if (!url) return;
    const a = h("a", { attrs: { href: url, download: "disk-graph.png" } });
    a.click();
  });
}

// ---------- search ----------

let searchTimer = 0;
function runSearch(): void {
  const tree = state.tree;
  const q = byId<HTMLInputElement>("q").value;
  const box = byId("hits");
  clear(box);
  state.hitIndex = -1;
  if (!tree || q.trim().length < 2) {
    box.hidden = true;
    return;
  }
  const hits = search(tree, q, 30);
  box.hidden = false;
  if (hits.length === 0) {
    box.append(h("div", { class: "none" }, "Nothing found"));
    return;
  }
  for (const hit of hits) {
    box.append(h("button", { attrs: { role: "option" }, onclick: () => pick(hit.node) },
      h("span", { class: "title" }, tree.names[hit.node]), h("span", { class: "size" }, human(tree.agg[hit.node])), h("span", { class: "path" }, hit.path)));
  }
}
function pick(node: number): void {
  byId("hits").hidden = true;
  reveal(node);
}

// ---------- controls ----------

function bind(): void {
  const file = byId<HTMLInputElement>("file");
  const openFile = (): void => file.click();
  byId("open").addEventListener("click", openFile);
  byId("emptyOpen").addEventListener("click", openFile);
  file.addEventListener("change", () => {
    const f = file.files?.[0];
    if (f) load({ type: "file", file: f });
    file.value = "";
  });
  const demo = (): void => load({ type: "demo", size: 150_000 });
  byId("demo").addEventListener("click", demo);
  byId("emptyDemo").addEventListener("click", demo);
  byId("errorClose").addEventListener("click", hideError);
  byId("pause").addEventListener("click", () => {
    state.replay.paused = !state.replay.paused;
    byId("pause").textContent = state.replay.paused ? "Resume" : "Pause";
  });
  byId("skip").addEventListener("click", finishReplay);
  byId("panelToggle").addEventListener("click", () => app.classList.toggle("no-side"));
  if (window.innerWidth < 900) app.classList.add("no-side");

  document.querySelectorAll<HTMLButtonElement>("#controls .seg button").forEach((b) =>
    b.addEventListener("click", () => {
      view.setMode((b.dataset.mode ?? "risk") as ColorMode);
      document.querySelectorAll("#controls .seg button").forEach((x) => x.classList.toggle("on", x === b));
      key();
      if (state.selected) renderDrawer();
      drawMinimap();
    }));
  document.querySelectorAll<HTMLButtonElement>(".tabs button").forEach((b) => b.addEventListener("click", () => {
    // Picking a list closes the details drawer that covers it.
    if (state.selected) select(null);
    setTab((b.dataset.tab ?? "findings") as Tab);
  }));

  const mini = byId<HTMLCanvasElement>("minimap");
  mini.addEventListener("click", (e) => {
    const r = mini.getBoundingClientRect();
    userMoved = true;
    view.minimapGo(e.clientX - r.left, e.clientY - r.top);
  });

  const q = byId<HTMLInputElement>("q");
  q.addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = window.setTimeout(runSearch, 160);
  });
  q.addEventListener("keydown", (e) => {
    const items = [...byId("hits").querySelectorAll<HTMLButtonElement>("button")];
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length === 0) return;
      state.hitIndex = (state.hitIndex + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length;
      items.forEach((it, i) => it.classList.toggle("on", i === state.hitIndex));
    } else if (e.key === "Enter") {
      (items[Math.max(0, state.hitIndex)] ?? null)?.click();
    } else if (e.key === "Escape") {
      q.value = "";
      byId("hits").hidden = true;
      q.blur();
    }
  });
  document.addEventListener("click", (e) => {
    const t = e.target as HTMLElement;
    if (!t.closest(".search")) byId("hits").hidden = true;
    if (!t.closest("#menu")) hideMenu();
  });
  document.addEventListener("keydown", (e) => {
    if ((e.target as HTMLElement).closest("input, textarea, select, dialog")) return;
    if (e.key === "/") {
      e.preventDefault();
      q.focus();
    } else if (e.key === "Escape") {
      hideMenu();
      if (state.selected) select(null);
      else if (state.owner >= 0) actions.focusOwner(-1);
      else app.classList.remove("cine");
    } else if (e.key === "f" || e.key === "F") view.fit();
    else if (e.key === "h" || e.key === "H") app.classList.toggle("cine");
    else if (e.key === " " && state.replay.running) {
      e.preventDefault();
      byId("pause").click();
    } else if (e.key === "Enter") {
      const s = selectedNode();
      if (s !== null && state.tree?.kind[s] === KIND.dir) focusOn(s);
    }
  });

  let depth = 0;
  window.addEventListener("dragenter", (e) => {
    if (!e.dataTransfer?.types.includes("Files")) return;
    depth++;
    byId("drop").hidden = false;
  });
  window.addEventListener("dragleave", () => {
    depth = Math.max(0, depth - 1);
    if (depth === 0) byId("drop").hidden = true;
  });
  window.addEventListener("dragover", (e) => e.preventDefault());
  window.addEventListener("drop", (e) => {
    e.preventDefault();
    depth = 0;
    byId("drop").hidden = true;
    const f = e.dataTransfer?.files?.[0];
    if (f) load({ type: "file", file: f });
  });

  const moved = (): void => {
    userMoved = true;
    view.freeze(true);
  };
  byId("graph").addEventListener("wheel", moved, { passive: true });
  byId("graph").addEventListener("mousedown", moved);
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (settings.theme === "system") applyLook();
  });
}

bind();
bindSettings();
document.documentElement.dataset.theme = isDark() ? "dark" : "light";
// Labels are drawn on a canvas: redraw once the fonts have arrived.
void document.fonts?.ready.then(() => view.relook());
const params = new URLSearchParams(location.search);
if (params.has("demo")) load({ type: "demo", size: Number(params.get("demo")) || 150_000 });

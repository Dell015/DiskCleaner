// The graph view app: loads a view file, grows the graph like a scan, and wires the controls.
// Read-only: nothing here can delete, move or change a file.

import "@fontsource-variable/atkinson-hyperlegible-next";
import "@fontsource-variable/atkinson-hyperlegible-mono";
import { appendRange, emptyTree } from "./core/build";
import { KIND, type ViewData } from "./core/format";
import { DEFAULT_LOD, nodeKey, selectVisible, type Visible, type VisItem } from "./core/lod";
import { search } from "./core/search";
import type { Tree } from "./core/tree";
import { count, human } from "./core/util";
import type { LoaderReply, LoaderRequest } from "./loader.worker";
import { LiveTree, simulateLive, type LiveEvent, type LiveSummary } from "./live";
import { cancelScan, confirmDrive, isDesktop, isDriveRoot, pickFolder, startScan, suggestedFolders } from "./platform";
import { makeDemo } from "./core/demo";
import { checkView } from "./core/format";
import { byId, clear, h } from "./panels/dom";
import { CATEGORY_LABELS, RISK_LABELS, renderApps, renderDetails, renderFindings, renderKey, type FindingFilter, type PanelActions } from "./panels/panels";
import { type ColorMode } from "./graph/encode";
import { GraphView, radiusFor } from "./graph/graph";
import { matchPreset, prefersReducedMotion, PRESET_NAMES, PRESETS, type Quality, type QualityId } from "./graph/quality";
import { paletteOf, STYLES, type StyleId } from "./graph/style";
import { ExplorerView } from "./panels/explorer";

/** A folder this big on screen (radius, px) opens when you zoom in; below CLOSE_PX it closes again. */
const OPEN_PX = 24;
const CLOSE_PX = 9;
/** How long the picture takes to build, per speed setting (slowest first). */
const REPLAY_SECONDS = [18, 11, 7, 4, 2];

type Tab = "findings" | "apps";

/** A live scan in progress (or just finished). */
interface LiveSession {
  id: number;
  lt: LiveTree | null;
  active: boolean;
  finished: boolean;
  summary: LiveSummary | null;
  queue: VisItem[];
  queued: Set<string>;
  partial: Visible;
  lastTarget: number;
  lastSync: number;
  last: number;
  credit: number;
  text: string;
  cancel: () => void;
  root: string;
  driveRoot: boolean;
  volumeUsed: number | null;
  files: number;
  dirs: number;
  current: string;
  phase: "reading" | "checking" | "placing";
  /** The user moved around the map during the scan: stop the paced reveal and show their view. */
  manual: boolean;
  lastList: number;
}
type Theme = "dark" | "light" | "system";

// ---------- settings (kept in this browser only) ----------

/** "files" is the simple list (the default); "map" is the node graph. */
type ViewMode = "files" | "map";
interface Settings { style: StyleId; theme: Theme; quality: Quality; chosen: boolean; view: ViewMode }
function loadSettings(): Settings {
  const base: Settings = { style: "deep", theme: "dark", quality: { ...PRESETS.high }, chosen: false, view: "files" };
  if (prefersReducedMotion()) base.quality.motion = "reduced";
  try {
    const raw = JSON.parse(localStorage.getItem("dc-settings") ?? "null") as Partial<Settings> | null;
    if (raw && typeof raw === "object") {
      if (raw.style && raw.style in STYLES) base.style = raw.style;
      if (raw.theme === "dark" || raw.theme === "light" || raw.theme === "system") base.theme = raw.theme;
      if (raw.quality && typeof raw.quality === "object") base.quality = { ...base.quality, ...raw.quality };
      base.chosen = Boolean(raw.chosen);
      if (raw.view === "files" || raw.view === "map") base.view = raw.view;
    }
  } catch {
    /* storage may be unavailable; defaults are used */
  }
  const st = params.get("style");
  if (st && st in STYLES) base.style = st as StyleId;
  const th = params.get("theme");
  if (th === "dark" || th === "light" || th === "system") base.theme = th;
  const vw = params.get("view");
  if (vw === "files" || vw === "map") base.view = vw;
  const q = params.get("quality");
  if (q && q in PRESETS) base.quality = { ...PRESETS[q as keyof typeof PRESETS] };
  return base;
}
const params = new URLSearchParams(location.search);
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
  replay: { running: false, paused: false, last: 0, shown: 0, order: [] as VisItem[], partial: null as Visible | null },
  live: null as LiveSession | null,
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
  /** The folder the list shows (-1: the top of what was scanned). */
  folder: -1,
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
const explorer = new ExplorerView(byId("explorer"), {
  select: (i) => select(i === null ? null : nodeKey(i)),
  open: (i) => goFolder(i),
  up: () => {
    const tree = state.tree;
    const f = curFolder();
    if (tree && f > 0) goFolder(tree.parent[f], f);
  },
  context: (i, x, y) => {
    const r = byId("stage").getBoundingClientRect();
    contextMenu(nodeKey(i), x - r.left, y - r.top);
  },
});
(window as unknown as { __dc: unknown }).__dc = { state, view, settings, refresh: () => refresh(false), reveal: (n: number) => reveal(n), scanFolder: (p?: string) => scanFolder(p), setViewMode: (m: ViewMode) => setViewMode(m), goFolder: (i: number) => goFolder(i), curFolder: () => curFolder() };

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
  endLive();
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
  state.folder = -1;
  state.findingsShown = 60;
  state.filter = data.marks.some((m) => m.offered) ? "offered" : data.marks.length ? "shown" : "offered";
  clearFocusTimers();
  closeDrawer();
  view.reset();
  view.setFocusOwner(-1);
  byId("empty").hidden = true;
  byId("minimap").hidden = false;
  userMoved = false;
  state.replay = { running: true, paused: false, last: 0, shown: 0, order: [], partial: null };
  frameTimes = [];
  prepare(data, 0);
}

/**
 * Step 1: put every entry in the tree, a slice per frame so the page stays responsive.
 * The data is already scanned; nothing here is shown yet.
 */
function prepare(data: ViewData, from: number): void {
  const tree = state.tree;
  if (!tree || state.data !== data) return;
  const n = data.names.length;
  const to = Math.min(n, from + 150_000);
  appendRange(tree, data, from, to);
  if (to < n) {
    byId("loading").hidden = false;
    byId("loadingText").textContent = `Preparing: ${Math.round((to / n) * 100)}%`;
    requestAnimationFrame(() => prepare(data, to));
    return;
  }
  byId("loading").hidden = true;
  renderSide();
  // Step 2: decide the final picture once, then reveal it at a steady pace.
  const root = pictureRoot();
  view.rootNode = root;
  const target = selectVisible(tree, { ...DEFAULT_LOD, budget: settings.quality.budget }, { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set(), root });
  const depthOf = (it: VisItem): number => (it.node >= 0 ? tree.depth[it.node] : tree.depth[it.group?.parent ?? 0] + 1);
  const rank = new Map(target.items.map((it, i) => [it.key, i]));
  // Outward in rings from the root (folders first in each ring, biggest first): the graph reaches out.
  state.replay.order = target.items.slice().sort((a, b) => depthOf(a) - depthOf(b) || (rank.get(a.key) ?? 0) - (rank.get(b.key) ?? 0));
  state.replay.partial = { items: [], byKey: new Map(), open: new Set(), truncated: false };
  // The list has nothing to animate; the map grows ring by ring.
  if (settings.quality.motion !== "full" || settings.view === "files") {
    finishReplay();
    return;
  }
  byId("scan").hidden = false;
  byId("pause").textContent = "Pause";
  key();
  state.replay.last = performance.now();
  requestAnimationFrame(revealStep);
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

/** Frame times seen while the picture grew: they decide whether graphics should step down. */
let frameTimes: number[] = [];

/** Step 3: a few more nodes every frame, at the same rate whatever the size of the disk. */
function revealStep(now: number): void {
  try {
    revealFrame(now);
  } catch (err) {
    // Never leave the picture half built: show the rest at once.
    console.error("building the picture failed; showing it all at once", err);
    finishReplay();
  }
}

function revealFrame(now: number): void {
  const r = state.replay;
  const tree = state.tree;
  const part = r.partial;
  if (!r.running || !tree || !part) return;
  const dt = Math.min(64, now - r.last);
  r.last = now;
  if (!r.paused) {
    frameTimes.push(dt);
    const secs = REPLAY_SECONDS[Number(byId<HTMLInputElement>("speed").value) - 1] ?? 7;
    const before = Math.floor(r.shown);
    // The whole build takes `secs`, but never slower than 60 nodes a second (small trees are quick).
    const rate = Math.max(r.order.length / secs, 60);
    r.shown = Math.min(r.order.length, r.shown + rate * (dt / 1000));
    const upto = Math.floor(r.shown);
    let head: string | null = null;
    for (let i = before; i < upto; i++) {
      const it = { ...r.order[i], open: false };
      part.items.push(it);
      part.byKey.set(it.key, it);
      if (it.parentKey) {
        const parent = part.byKey.get(it.parentKey);
        if (parent && parent.node >= 0 && !parent.open) {
          parent.open = true;
          part.open.add(parent.node);
        }
        head = it.parentKey;
      }
    }
    if (upto > before) {
      view.sync(tree, part, true);
      if (head) view.setHead(head);
      setScan("Building the picture", (upto / r.order.length) * 100, `${count(upto)} of ${count(r.order.length)} nodes`, `${count(tree.files[0])} files, ${human(tree.agg[0])}`);
      drawMinimap();
    }
  }
  if (r.shown >= r.order.length) {
    finishReplay();
    return;
  }
  requestAnimationFrame(revealStep);
}

function finishReplay(): void {
  const r = state.replay;
  const tree = state.tree;
  if (!tree || !r.running) return;
  r.running = false;
  r.shown = r.order.length;
  byId("scan").hidden = true;
  view.setHead(null);
  refresh(true);
  renderSide();
  settle();
}

/** Once the layout is calm: frame everything (unless the user moved), then hold still. */
function settle(): void {
  const started = performance.now();
  const wait = (): void => {
    // Wait for the layout to calm down, but never more than 5 s.
    if (view.layoutRunning() && !userMoved && performance.now() - started < 5000) setTimeout(wait, 300);
    else {
      if (!userMoved) view.fit();
      view.freeze(true);
      view.stable = true;
      autoQuality();
    }
  };
  setTimeout(wait, 600);
}


// ---------- progress readout ----------

/** Update the progress readout. `pct` null means "how much is left is not known". */
function setScan(title: string, pct: number | null, meta: string, countText: string, path = ""): void {
  byId("scanTitle").textContent = title;
  byId("scanPct").textContent = pct === null ? "" : `${Math.floor(pct)}%`;
  const bar = byId("scanFill").parentElement as HTMLElement;
  bar.classList.toggle("unknown", pct === null);
  byId("scanFill").style.width = pct === null ? "" : `${Math.min(100, Math.max(0, pct))}%`;
  if (pct !== null) bar.setAttribute("aria-valuenow", String(Math.floor(pct)));
  else bar.removeAttribute("aria-valuenow");
  byId("scanText").textContent = meta;
  byId("scanCount").textContent = countText;
  byId("scanPath").textContent = path;
}

// ---------- live scan (desktop app, or a simulated one with ?live=demo) ----------

let liveCounter = 0;

/** Stop any live scan and forget it (a file or the demo is being opened instead). */
function endLive(): void {
  const s = state.live;
  if (!s) return;
  if (s.active) s.cancel();
  s.active = false;
  state.live = null;
  byId("stop").hidden = true;
}

/** Ask for a folder (or take one) and scan it live. */
async function scanFolder(path?: string): Promise<void> {
  try {
    const desktop = isDesktop();
    if (!desktop && !params.has("live")) return;
    const target = path ?? (desktop ? await pickFolder() : "C:\\");
    if (!target) return;
    let allow = false;
    if (desktop && (await isDriveRoot(target))) {
      if (!(await confirmDrive(target))) return;
      allow = true;
    }
    beginLive((on) => {
      if (desktop) {
        startScan(target, allow, on).catch((e) => on({ type: "failed", message: String(e) }));
        return () => void cancelScan();
      }
      // Browser checks: pretend to scan the demo disk.
      const sim = simulateLive(checkView(makeDemo(Number(params.get("live")) || 150_000)), on);
      return () => sim.cancel();
    });
  } catch (e) {
    showError(`The scan could not start: ${String(e)}`);
  }
}

function beginLive(starter: (on: (e: LiveEvent) => void) => () => void): void {
  endLive();
  state.replay.running = false;
  worker?.terminate();
  state.data = null;
  state.tree = null;
  state.vis = null;
  state.opened.clear();
  state.zoomOpened.clear();
  state.closed.clear();
  state.wide.clear();
  state.selected = null;
  state.focusRoot = null;
  state.owner = -1;
  state.folder = -1;
  clearFocusTimers();
  closeDrawer();
  view.reset();
  view.setFocusOwner(-1);
  userMoved = false;
  frameTimes = [];
  byId("empty").hidden = true;
  byId("minimap").hidden = false;
  byId("scan").hidden = false;
  byId("scan").classList.add("live");
  byId("stop").hidden = false;
  setScan("Starting the scan", null, "", "");
  const id = ++liveCounter;
  const session: LiveSession = {
    id, lt: null, active: true, finished: false, summary: null, queue: [], queued: new Set(),
    partial: { items: [], byKey: new Map(), open: new Set(), truncated: false },
    lastTarget: 0, lastSync: 0, last: performance.now(), credit: 0, text: "Starting", cancel: () => undefined,
    root: "", driveRoot: false, volumeUsed: null, files: 0, dirs: 0, current: "", phase: "reading", manual: false, lastList: 0,
  };
  state.live = session;
  session.cancel = starter((e) => onLiveEvent(id, e));
}

function onLiveEvent(id: number, e: LiveEvent): void {
  const s = state.live;
  if (!s || s.id !== id) return;
  switch (e.type) {
    case "started":
      s.root = e.root;
      s.driveRoot = Boolean(e.drive_root);
      s.volumeUsed = e.drive_root && e.volume_used ? e.volume_used : null;
      s.lt = new LiveTree(e.root);
      state.tree = s.lt.tree;
      view.rootNode = 1;
      renderSide();
      key();
      crumbs();
      s.last = performance.now();
      requestAnimationFrame(liveStep);
      break;
    case "batch":
      s.lt?.add(e.dirs);
      break;
    case "progress":
      s.files = e.files;
      s.dirs = e.dirs;
      s.current = e.current;
      break;
    case "analysing":
      s.phase = "checking";
      break;
    case "finished": {
      s.summary = e;
      s.lt?.finish(e);
      s.finished = true;
      s.phase = "placing";
      if (e.status === "cancelled") toast("Scan stopped. The picture shows what was read; no checks were run.");
      else if (e.status === "partial") toast(`Done. ${count(e.errors)} entries could not be read (usually folders Windows keeps private).`);
      else if (!e.apps_known) toast("Done. Installed apps could not be read, so owners and leftovers are missing.");
      break;
    }
    case "failed":
      s.active = false;
      byId("scan").hidden = true;
      byId("stop").hidden = true;
      showError(e.message);
      break;
  }
}

/** Every frame: new nodes join at a steady pace; every 400 ms the picture's target is worked out again. */
function liveStep(now: number): void {
  const s = state.live;
  const tree = state.tree;
  if (!s || !s.active || !tree) return;
  try {
    // Slow frames still drain the queue at the intended pace (up to a quarter second per frame).
    const dt = Math.min(250, now - s.last);
    s.last = now;
    frameTimes.push(Math.min(64, dt));
    if (settings.view === "files" || s.manual) {
      // No paced reveal: the list (or the user's own map) updates a few times a second.
      if (now - s.lastList > 450 || s.finished) {
        s.lastList = now;
        liveReadout(s, tree);
        if (settings.view === "files") renderList();
        else refresh(true);
      }
      if (s.finished) {
        finishLive();
        return;
      }
      requestAnimationFrame(liveStep);
      return;
    }
    if (now - s.lastTarget > 400 || (s.finished && s.queue.length === 0 && now - s.lastTarget > 100)) {
      s.lastTarget = now;
      liveTarget(s, tree);
    }
    const rate = Math.max(45, s.queue.length / 1.4);
    s.credit += (rate * dt) / 1000;
    let k = Math.floor(s.credit);
    s.credit -= k;
    const wanted = k;
    let added = 0;
    let head: string | null = null;
    const part = s.partial;
    const cap = DEFAULT_LOD.hardCap;
    let tries = s.queue.length;
    while (k > 0 && s.queue.length > 0 && part.items.length < cap && tries-- > 0) {
      const it = s.queue.shift()!;
      if (it.parentKey && !part.byKey.has(it.parentKey)) {
        s.queue.push(it); // its folder is not on screen yet
        continue;
      }
      s.queued.delete(it.key);
      const c = { ...it, open: false };
      part.items.push(c);
      part.byKey.set(c.key, c);
      if (c.parentKey) {
        const parent = part.byKey.get(c.parentKey);
        if (parent && parent.node >= 0 && !parent.open) {
          parent.open = true;
          part.open.add(parent.node);
        }
        head = c.parentKey;
      }
      k--;
      added++;
    }
    if (added > 0 || now - s.lastSync > 250) {
      s.lastSync = now;
      view.sync(tree, part, true);
      if (head) view.setHead(head);
      liveReadout(s, tree);
      drawMinimap();
      status();
    }
    // Done, or nothing left that can join (never wait forever on entries whose folder is not shown).
    if (s.finished && (s.queue.length === 0 || (wanted > 0 && added === 0) || part.items.length >= cap)) {
      finishLive();
      return;
    }
  } catch (err) {
    console.error("the live picture failed; showing the result directly", err);
    finishLive();
    return;
  }
  requestAnimationFrame(liveStep);
}

/** The readout while a live scan runs. */
function liveReadout(s: LiveSession, tree: Tree): void {
  const found = tree.agg[0];
  // Once reading is done there is nothing left to stop.
  byId("stop").hidden = s.phase !== "reading";
  const counts = `${count(Math.max(s.files, tree.files[0]))} files, ${count(Math.max(s.dirs, 0))} folders`;
  if (s.phase === "checking") {
    setScan("Checking what was found", null, "Cleanup rules, app owners and leftovers", counts);
  } else if (s.phase === "placing") {
    const total = s.partial.items.length + s.queue.length;
    setScan("Placing the last folders", total ? (s.partial.items.length / total) * 100 : 100, `${count(s.partial.items.length)} of ${count(total)} nodes`, counts);
  } else if (s.volumeUsed) {
    // A whole drive: compare what was found with the space in use (never shows 100% before the end).
    setScan(`Reading ${s.root}`, Math.min(99, (found / s.volumeUsed) * 100), `${human(found)} of ${human(s.volumeUsed)} in use`, counts, s.current);
  } else {
    setScan(`Reading ${s.root}`, null, `${human(found)} found`, counts, s.current);
  }
}

/** Work out what the picture should hold now and queue what is new (nothing shown is removed). */
function liveTarget(s: LiveSession, tree: Tree): void {
  if (tree.n < 2) return;
  const target = selectVisible(tree, { ...DEFAULT_LOD, budget: settings.quality.budget }, { opened: new Set(), closed: new Set(), wide: new Set(), mustShow: new Set(), root: 1 });
  const depthOf = (it: VisItem): number => (it.node >= 0 ? tree.depth[it.node] : tree.depth[it.group?.parent ?? 0] + 1);
  const rank = new Map(target.items.map((it, i) => [it.key, i]));
  const fresh = target.items.filter((it) => !s.partial.byKey.has(it.key) && !s.queued.has(it.key));
  fresh.sort((a, b) => depthOf(a) - depthOf(b) || (rank.get(a.key) ?? 0) - (rank.get(b.key) ?? 0));
  for (const it of fresh) {
    s.queue.push(it);
    s.queued.add(it.key);
  }
  // "More items" nodes already on screen keep counting up.
  for (const it of target.items) {
    if (!it.group) continue;
    const shown = s.partial.byKey.get(it.key);
    if (shown) shown.group = it.group;
  }
}

function finishLive(): void {
  const s = state.live;
  if (!s) return;
  s.active = false;
  s.finished = true;
  byId("scan").hidden = true;
  byId("scan").classList.remove("live");
  byId("stop").hidden = true;
  view.setHead(null);
  refresh(true);
  renderSide();
  settle();
}

// ---------- visible set ----------

function pictureRoot(): number {
  const data = state.data;
  const tree = state.tree;
  if (state.focusRoot !== null && tree && state.focusRoot < tree.n) return state.focusRoot;
  if (state.live && tree && tree.n > 1) return 1;
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
  if (settings.view === "files") {
    renderList();
    crumbs();
    return;
  }
  // While a live scan grows the picture, its own loop owns the graph (until the user takes over).
  if (state.live?.active && !state.live.manual) return;
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
  if (!tree || !vis || state.replay.running || settings.view === "files" || (state.live?.active && !state.live.manual)) return;
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
  if (settings.view === "files") renderList();
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
  if (settings.view === "files") {
    goFolder(i);
    return;
  }
  takeOver();
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
  if (state.replay.running) finishReplay();
  const tree = state.tree;
  if (!tree || i < 0 || i >= tree.n) return;
  if (settings.view === "files") {
    goFolder(i === 0 ? 0 : tree.parent[i], i);
    return;
  }
  takeOver();
  const root = pictureRoot();
  if (i !== root && !tree.isAncestor(root, i)) state.focusRoot = null;
  // Stop following the growing frame before flying: the target must not move under the camera.
  userMoved = true;
  view.freeze(true);
  for (const a of tree.ancestors(i)) state.closed.delete(a);
  state.selected = nodeKey(i);
  refresh(true);
  view.setSelected(state.selected);
  openDrawer();
  crumbs();
  clearFocusTimers();
  // Fly once the new nodes have a place (they start at their folder and move out).
  focusTimers.push(window.setTimeout(() => view.focus(nodeKey(i)), 450));
}

/** Make a folder the centre of the picture ("local graph"). null goes back to everything. */
function focusOn(i: number | null): void {
  const tree = state.tree;
  if (!tree) return;
  if (settings.view === "files") {
    goFolder(i ?? 0);
    return;
  }
  takeOver();
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

// ---------- the two ways to look: the list (simple) and the map (graph) ----------

/** The top of what was scanned: a single drive or folder opens straight into it. */
function listRoot(): number {
  const tree = state.tree;
  if (!tree || tree.n < 2) return 0;
  if (state.live || (state.data && state.data.roots.length === 1)) return 1;
  return 0;
}

function curFolder(): number {
  const tree = state.tree;
  const f = state.folder;
  if (tree && f >= 0 && f < tree.n && tree.kind[f] === KIND.dir) return f;
  return listRoot();
}

/** Show a folder in the list; `pick` selects one of its entries (for search and findings). */
function goFolder(i: number, pick: number | null = null): void {
  const tree = state.tree;
  if (!tree || i < 0 || i >= tree.n) return;
  hideMenu();
  state.folder = i;
  state.selected = pick === null ? null : nodeKey(pick);
  view.setSelected(null);
  if (pick !== null) openDrawer();
  else closeDrawer();
  renderList();
  crumbs();
  if (pick !== null) explorer.reveal(pick);
  explorer.focus();
}

function renderList(): void {
  const tree = state.tree;
  if (settings.view !== "files" || !tree || tree.n === 0) return;
  explorer.show(tree, curFolder(), selectedNode(), view.pal, view.mode);
}

/** The user moved around the map while a scan runs: from now on show their view, not the paced reveal. */
function takeOver(): void {
  const s = state.live;
  if (s?.active && !s.manual) s.manual = true;
}

function applyViewMode(): void {
  const files = settings.view === "files";
  app.classList.toggle("files", files);
  byId("explorer").hidden = !files;
  document.querySelectorAll<HTMLButtonElement>("#viewSeg button").forEach((b) => {
    const on = b.dataset.view === settings.view;
    b.classList.toggle("on", on);
    b.setAttribute("aria-pressed", String(on));
  });
}

function setViewMode(m: ViewMode): void {
  if (settings.view === m) return;
  settings.view = m;
  saveSettings();
  hideMenu();
  byId("tip").hidden = true;
  applyViewMode();
  const tree = state.tree;
  if (!tree) return;
  if (m === "files") {
    // Open the list where the map was looking.
    const sel = selectedNode();
    const at = state.focusRoot ?? (sel !== null ? (tree.kind[sel] === KIND.dir ? sel : tree.parent[sel]) : -1);
    if (at >= 0) state.folder = at;
    renderList();
    crumbs();
    explorer.focus();
    if (sel !== null) explorer.reveal(sel);
    return;
  }
  // The map: centre it on the folder the list was showing.
  if (state.replay.running) finishReplay();
  const f = curFolder();
  state.focusRoot = f > listRoot() ? f : null;
  const keep = state.selected;
  clearFocusTimers();
  view.reset();
  userMoved = false;
  view.freeze(false);
  if (state.live?.active) state.live.manual = true;
  refresh(false);
  if (keep) {
    state.selected = keep;
    view.setSelected(keep);
    openDrawer();
  }
  crumbs();
  settle();
}

/** The bar that says what is highlighted on the map and how to clear it. */
function selbar(): void {
  const bar = byId("selbar");
  const tree = state.tree;
  const sel = selectedNode();
  const focus = state.focusRoot;
  if (!tree || settings.view !== "map" || (sel === null && focus === null)) {
    bar.hidden = true;
    return;
  }
  const name = (i: number): string => (i === 0 ? "This PC" : tree.names[i].replace(/[\\/]+$/, "") || tree.names[i]);
  const text = byId("selText");
  clear(text);
  if (sel !== null) text.append("Highlighted: ", h("b", {}, name(sel)), h("span", { class: "quiet" }, " and the way to it; the rest is dimmed."));
  else if (focus !== null) text.append("Showing only ", h("b", {}, name(focus)), ".");
  byId("selClear").textContent = sel !== null ? "Clear selection" : "Show everything";
  byId("selAll").hidden = !(sel !== null && focus !== null);
  bar.hidden = false;
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
  selbar();
  if (settings.view === "files") {
    const f = curFolder();
    const chain = f > 0 ? [...tree.ancestors(f), f].filter((a) => a !== 0) : [];
    const all = h("button", { onclick: () => goFolder(0) }, "This PC");
    if (chain.length === 0) all.setAttribute("aria-current", "page");
    el.append(all);
    for (const a of chain) {
      el.append(h("span", { class: "sep", attrs: { "aria-hidden": "true" } }, "/"));
      const b = h("button", { onclick: () => goFolder(a), title: tree.path(a) }, tree.names[a].replace(/[\\/]+$/, "") || tree.names[a]);
      if (a === f) b.setAttribute("aria-current", "page");
      el.append(b);
    }
    el.scrollLeft = el.scrollWidth;
    return;
  }
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
  if (state.live && !state.live.finished) {
    const el = byId(state.tab === "findings" ? "tab-findings" : "tab-apps");
    clear(el);
    el.append(h("p", { class: "lead" }, "Reading the disk"), h("p", { class: "quiet" }, "Findings and apps appear when the scan is done. Nothing is changed while it runs."));
    return;
  }
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
  if (!state.replay.running && state.data && !state.live) {
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
  const menu = byId("menu");
  if (!tree) return;
  if (settings.view === "files") {
    const i = Number(k.slice(1));
    if (!(i >= 0 && i < tree.n)) return;
    clear(menu);
    const add = (label: string, run: () => void): void => {
      menu.append(h("button", { attrs: { role: "menuitem" }, onclick: () => { hideMenu(); run(); } }, label));
    };
    add("Show details", () => select(k));
    if (tree.kind[i] === KIND.dir && tree.childCount[i] > 0) add("Open folder", () => goFolder(i));
    add("Show on the map", () => {
      setViewMode("map");
      reveal(i);
    });
    add("Copy path", () => void navigator.clipboard?.writeText(tree.path(i)));
    const rect = byId("stage").getBoundingClientRect();
    menu.hidden = false;
    menu.style.left = `${Math.min(rect.left + x, window.innerWidth - 200)}px`;
    menu.style.top = `${Math.min(rect.top + y, window.innerHeight - 200)}px`;
    (menu.firstElementChild as HTMLElement | null)?.focus();
    return;
  }
  const item = state.vis?.byKey.get(k);
  if (!item) return;
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

/** Scanning needs the desktop app (or ?live for the browser checks). */
function bindScan(): void {
  const can = isDesktop() || params.has("live");
  for (const id of ["scanBtn", "emptyScan"]) byId(id).hidden = !can;
  if (!can) {
    byId("emptyOpen").classList.add("primary");
    return;
  }
  byId("scanBtn").addEventListener("click", () => void scanFolder());
  byId("emptyScan").addEventListener("click", () => void scanFolder());
  byId("stop").addEventListener("click", () => {
    const s = state.live;
    if (s?.active) {
      byId("scanTitle").textContent = "Stopping";
      s.cancel();
    }
  });
  if (isDesktop()) {
    void suggestedFolders().then((list) => {
      const box = byId("suggest");
      clear(box);
      for (const sug of list) box.append(h("button", { title: sug.path, onclick: () => void scanFolder(sug.path) }, `Scan ${sug.label}`));
      box.hidden = list.length === 0;
    }).catch(() => undefined);
  }
}

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
  document.querySelectorAll<HTMLButtonElement>("#viewSeg button").forEach((b) =>
    b.addEventListener("click", () => setViewMode((b.dataset.view ?? "files") as ViewMode)));
  byId("selClear").addEventListener("click", () => {
    if (state.selected) select(null);
    else if (state.focusRoot !== null) focusOn(null);
  });
  byId("selAll").addEventListener("click", () => focusOn(null));
  if (window.innerWidth < 900) app.classList.add("no-side");

  document.querySelectorAll<HTMLButtonElement>("#controls .seg button").forEach((b) =>
    b.addEventListener("click", () => {
      view.setMode((b.dataset.mode ?? "risk") as ColorMode);
      document.querySelectorAll("#controls .seg button").forEach((x) => x.classList.toggle("on", x === b));
      key();
      if (state.selected) renderDrawer();
      renderList();
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
      else if (state.focusRoot !== null && settings.view === "map") focusOn(null);
      else if (state.owner >= 0) actions.focusOwner(-1);
      else app.classList.remove("cine");
    } else if (settings.view === "files" && (e.key === "Backspace" || (e.altKey && e.key === "ArrowUp"))) {
      if ((e.target as HTMLElement).closest("#explorer")) return;
      e.preventDefault();
      const tree = state.tree;
      const f = curFolder();
      if (tree && f > 0) goFolder(tree.parent[f], f);
    } else if (e.key === "f" || e.key === "F") view.fit();
    else if (e.key === "h" || e.key === "H") app.classList.toggle("cine");
    else if (e.key === " " && state.replay.running) {
      e.preventDefault();
      byId("pause").click();
    } else if (e.key === "Enter") {
      if ((e.target as HTMLElement).closest("#explorer, button, a")) return;
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
bindScan();
applyViewMode();
document.documentElement.dataset.theme = isDark() ? "dark" : "light";
// Labels are drawn on a canvas: redraw once the fonts have arrived.
void document.fonts?.ready.then(() => view.relook());
if (params.has("demo")) load({ type: "demo", size: Number(params.get("demo")) || 150_000 });

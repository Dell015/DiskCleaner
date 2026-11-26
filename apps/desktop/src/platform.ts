// The desktop app's commands (Tauri). In a plain browser these are not available and the page
// works with view files and the demo only.

import { Channel, invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type { LiveEvent } from "./live";
import type { DupEvent } from "./core/dupes";

export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export interface Suggestion { label: string; path: string }

export async function suggestedFolders(): Promise<Suggestion[]> {
  return invoke<Suggestion[]>("suggested_folders");
}

/** Ask the user for a folder. null if they cancelled. */
export async function pickFolder(): Promise<string | null> {
  const r = await open({ directory: true, multiple: false, title: "Pick a folder to scan" });
  return typeof r === "string" ? r : null;
}

export async function isDriveRoot(path: string): Promise<boolean> {
  return invoke<boolean>("is_drive_root", { path });
}

export async function confirmDrive(path: string): Promise<boolean> {
  return ask(`${path} is a whole drive. Reading every folder on it can take a few minutes. Nothing is changed or deleted. Scan it?`, { title: "Scan a whole drive?", kind: "warning", okLabel: "Scan the drive", cancelLabel: "Cancel" });
}

export interface ScanChoices { withUsage: boolean; keepReport: boolean }

export async function startScan(path: string, allowDriveRoot: boolean, choices: ScanChoices, onEvent: (e: LiveEvent) => void): Promise<void> {
  const channel = new Channel<LiveEvent>();
  channel.onmessage = onEvent;
  await invoke("start_scan", { options: { path, allowDriveRoot, withApps: true, ...choices }, onEvent: channel });
}

export interface AiStatus {
  enabled: boolean;
  folder: string;
  report_time: number | null;
  program: string;
  log: { time: number; tool: string; args: Record<string, unknown>; ok: boolean }[];
}

export async function aiStatus(): Promise<AiStatus> {
  return invoke<AiStatus>("ai_status");
}

export async function setAiAccess(enabled: boolean): Promise<void> {
  await invoke("set_ai_access", { enabled });
}

/** The latest scan report (Markdown), to paste into any AI chat. */
export async function latestReport(): Promise<string> {
  return invoke<string>("latest_report");
}

/** Look for identical files in the last finished scan (below `within`, if given). Reads only. */
export async function findDupes(within: string | null, onEvent: (e: DupEvent) => void): Promise<void> {
  const channel = new Channel<DupEvent>();
  channel.onmessage = onEvent;
  await invoke("find_dupes", { within, onEvent: channel });
}

export async function cancelDupes(): Promise<void> {
  await invoke("cancel_dupes");
}

export interface Saved { path: string; entries: number; bytes: number }

/** Ask where to save the finished scan, then save it. null if the user cancelled. */
export async function saveScan(suggestedName: string): Promise<Saved | null> {
  const path = await save({ title: "Save this scan", defaultPath: suggestedName, filters: [{ name: "Rhizome scan", extensions: ["json"] }] });
  if (!path) return null;
  return invoke<Saved>("save_scan", { path });
}

export async function cancelScan(): Promise<void> {
  await invoke("cancel_scan");
}

// ---------- removing (desktop builds with the "cleaning" feature; a stand-in for browser checks) ----------

export interface PickRequest { path: string; checked: boolean; understood: boolean; /** A folder this app left behind after being uninstalled (D133). */ leftBehindOf?: string }
export interface Removal { id: string; held: [string, number][]; skipped: [string, string][] }
export interface CleanupItem {
  root: string;
  n: number;
  original: string;
  held: string;
  is_dir: boolean;
  bytes: number;
  files: number;
  group: string;
  title: string;
  reason: string;
  state: "held" | "restored" | "emptied";
  to?: string;
}
export interface CleanupInfo { id: string; created: number; items: CleanupItem[] }
export interface RestoreResult { back: [string, string][]; problems: { path: string; why: string }[] }

/** A pretend engine for the browser: nothing on a disk, everything in memory. */
class Pretend {
  cleanups: CleanupInfo[] = [];
  remove(picks: PickRequest[], verdict: (path: string) => { group: string; why: string; bytes: number; isDir: boolean } | null): Removal {
    const id = `${Math.floor(Date.now() / 1000)}-sim`;
    const out: Removal = { id: "", held: [], skipped: [] };
    const items: CleanupItem[] = [];
    for (const p of picks) {
      const found = verdict(p.path);
      // Left behind by an uninstalled app: Probably safe in the stand-in.
      const v = found && p.leftBehindOf ? { ...found, group: "probably_safe", why: `Left behind by ${p.leftBehindOf}` } : found;
      if (!v) { out.skipped.push([p.path, "it is not in the scan"]); continue; }
      if (v.group === "never") { out.skipped.push([p.path, v.why]); continue; }
      if (v.group === "probably_safe" && !p.checked) { out.skipped.push([p.path, "it is Probably safe and was not ticked as checked"]); continue; }
      if (v.group === "your_call" && !p.understood) { out.skipped.push([p.path, "removing your own files was not confirmed"]); continue; }
      out.held.push([p.path, v.bytes]);
      items.push({ root: "(pretend)", n: items.length + 1, original: p.path, held: `(pretend)\\${p.path}`, is_dir: v.isDir, bytes: v.bytes, files: 1, group: v.group, title: v.why, reason: "", state: "held" });
    }
    if (items.length) {
      this.cleanups.unshift({ id, created: Date.now() / 1000, items });
      out.id = id;
    }
    return out;
  }
}
let pretend: Pretend | null = null;
let pretendVerdict: ((path: string) => { group: string; why: string; bytes: number; isDir: boolean } | null) | null = null;

/** Browser checks only: use the pretend engine with this way of judging paths. */
export function usePretendCleaning(verdict: (path: string) => { group: string; why: string; bytes: number; isDir: boolean } | null): void {
  pretend = new Pretend();
  pretendVerdict = verdict;
}

export async function cleaningAvailable(): Promise<boolean> {
  if (pretend) return true;
  if (!isDesktop()) return false;
  return invoke<boolean>("cleaning_available");
}

export async function removeItems(picks: PickRequest[], onProgress: (done: number, total: number) => void): Promise<Removal> {
  if (pretend && pretendVerdict) {
    onProgress(picks.length, picks.length);
    return pretend.remove(picks, pretendVerdict);
  }
  const channel = new Channel<[number, number]>();
  channel.onmessage = ([d, t]) => onProgress(d, t);
  return invoke<Removal>("remove_items", { picks, onProgress: channel });
}

export async function listCleanups(): Promise<CleanupInfo[]> {
  if (pretend) return pretend.cleanups;
  return invoke<CleanupInfo[]>("list_cleanups");
}

export async function restoreItems(id: string, only: [string, number][] | null): Promise<RestoreResult> {
  if (pretend) {
    const c = pretend.cleanups.find((x) => x.id === id);
    const back: [string, string][] = [];
    for (const it of c?.items ?? []) {
      if (it.state !== "held" || (only && !only.some(([r, n]) => r === it.root && n === it.n))) continue;
      it.state = "restored";
      it.to = it.original;
      back.push([it.original, it.original]);
    }
    return { back, problems: [] };
  }
  return invoke<RestoreResult>("restore_items", { id, only });
}

export async function emptyCleanup(id: string): Promise<number> {
  if (pretend) {
    let bytes = 0;
    for (const it of pretend.cleanups.find((x) => x.id === id)?.items ?? []) {
      if (it.state === "held") { bytes += it.bytes; it.state = "emptied"; }
    }
    return bytes;
  }
  return invoke<number>("empty_cleanup", { id });
}

/** App icons as picture addresses, one per icon location (desktop app only; null when there is none). */
export async function appIcons(locations: string[]): Promise<(string | null)[]> {
  if (!isDesktop() || locations.length === 0) return locations.map(() => null);
  return invoke<(string | null)[]>("app_icons", { locations });
}

/** A cleanup whose retention time is over (nothing is deleted by asking). */
export interface ExpiredInfo { id: string; created: number; bytes: number }

export async function expiredCleanups(days: number): Promise<ExpiredInfo[]> {
  if (pretend) {
    const limit = Date.now() / 1000 - days * 86_400;
    return pretend.cleanups
      .filter((c) => c.created > 0 && c.created <= limit && c.items.some((i) => i.state === "held"))
      .map((c) => ({ id: c.id, created: c.created, bytes: c.items.filter((i) => i.state === "held").reduce((a, i) => a + i.bytes, 0) }));
  }
  return invoke<ExpiredInfo[]>("expired_cleanups", { days });
}

/** What emptying old cleanups did: emptied (id, bytes freed) and failed (id, why). */
export interface EmptiedInfo { emptied: [string, number][]; failed: [string, string][] }

/** Empty the cleanups `ids`, each only if it is still older than `days`. */
export async function emptyExpired(days: number, ids: string[]): Promise<EmptiedInfo> {
  if (pretend) {
    const old = new Set((await expiredCleanups(days)).map((c) => c.id));
    const out: EmptiedInfo = { emptied: [], failed: [] };
    for (const id of ids) {
      if (!old.has(id)) continue;
      try {
        out.emptied.push([id, await emptyCleanup(id)]);
      } catch (e) {
        out.failed.push([id, String(e)]);
      }
    }
    return out;
  }
  return invoke<EmptiedInfo>("empty_expired", { days, ids });
}

/** Holding folders that cannot be found now (a drive not connected, or a changed letter). */
export async function missingHolding(): Promise<string[]> {
  if (pretend || !isDesktop()) return [];
  return invoke<string[]>("missing_holding");
}

// ---------- uninstalling an app with its own uninstaller (D133; builds with "cleaning") ----------

export interface Uninstalling { how: "uninstaller" | "settings"; why: string | null }

/** Browser checks: apps the pretend uninstaller has removed. */
const pretendGone = new Set<string>();

/** Start an app's own uninstaller (or Windows' Installed apps page when that is safer). */
export async function uninstallApp(name: string, publisher?: string): Promise<Uninstalling> {
  if (pretend) {
    // The pretend uninstaller finishes a moment later.
    window.setTimeout(() => pretendGone.add(name), 300);
    return { how: "uninstaller", why: null };
  }
  return invoke<Uninstalling>("uninstall_app", { name, publisher: publisher ?? null });
}

/** Is each of these apps still installed (Windows' list right now)? */
export async function appsInstalled(names: string[]): Promise<boolean[]> {
  if (pretend) return names.map((n) => !pretendGone.has(n));
  return invoke<boolean[]>("apps_installed", { names });
}

/** Is each of these folders still there? */
export async function pathsExist(paths: string[]): Promise<boolean[]> {
  if (pretend) return paths.map(() => true);
  return invoke<boolean[]>("paths_exist", { paths });
}

/** Size [bytes, files] of each folder now, or null when it is gone (read-only). */
export async function folderSizes(paths: string[]): Promise<([number, number] | null)[]> {
  if (pretend) return paths.map(() => [123_456, 3]);
  return invoke<([number, number] | null)[]>("folder_sizes", { paths });
}

// ---------- Moving an app to another drive (B11, D141) ----------

export type MoveHow =
  | { how: "move"; link: boolean; folder: string; warnings: string[]; steps: string[] }
  | { how: "guide"; by: string; steps: string[] }
  | { how: "never"; why: string };
export interface MoveTarget { drive: string; filesystem: string; free: number; total: number; removable: boolean; dest: string; why: string }
export interface MoveOptions { how: MoveHow; targets: MoveTarget[] }
export interface MoveAsk { name: string; publisher?: string; folder: string; dest?: string; game?: boolean }
export interface MoveChecked { ok: boolean; why: string; bytes: number; files: number; free: number; removable: boolean }
export type MoveStateId = "moving" | "waiting" | "done" | "moving_back" | "moved_back" | "failed";
export interface MoveInfo {
  id: string; app: string; source: string; dest: string; link: boolean; bytes: number; files: number; created: number; held: string;
  state: MoveStateId; step?: string; why?: string; notes: string[];
}

/** A pretend mover for the browser checks: nothing on a disk. */
class PretendMover {
  moves: MoveInfo[] = [];
  launcher: (name: string) => string | undefined = () => undefined;
}
let pretendMover: PretendMover | null = null;

/** Browser checks only: pretend to move apps (`launcher` says which apps a launcher installed). */
export function usePretendMoving(launcher: (name: string) => string | undefined): void {
  pretendMover = new PretendMover();
  pretendMover.launcher = launcher;
}

export async function movingAvailable(): Promise<boolean> {
  if (pretendMover) return true;
  if (!isDesktop()) return false;
  return invoke<boolean>("moving_available");
}

export async function moveOptions(ask: MoveAsk): Promise<MoveOptions> {
  if (pretendMover) {
    const l = pretendMover.launcher(ask.name);
    const leaf = ask.folder.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? ask.name;
    const base = ask.game ? "Games" : "Apps";
    const how: MoveHow = l === "steam"
      ? { how: "guide", by: "Steam", steps: ["Open Steam and go to your Library.", `Right-click ${ask.name}, then Properties.`, "Open Installed Files, then Move install folder."] }
      : { how: "move", link: !l, folder: ask.folder, warnings: [l ? `${ask.name}'s launcher must be told the new place after the move (the steps are shown).` : `Rhizome has not seen ${ask.name} moved before. Most apps keep working through the link; a few updaters do not. If it does not start, press Move it back.`], steps: l ? ["Open the launcher and pick the game.", "Use Locate game and pick the new folder.", "Start it. If it runs, come back and press It works."] : [] };
    return { how, targets: [
      { drive: "D", filesystem: "NTFS", free: 600e9, total: 1000e9, removable: false, dest: `D:\\${base}\\${leaf}`, why: "" },
      { drive: "E", filesystem: "exFAT", free: 60e9, total: 64e9, removable: true, dest: `E:\\${base}\\${leaf}`, why: "formatted exFAT; apps can only go to an NTFS drive" },
    ] };
  }
  return invoke<MoveOptions>("move_options", { ask });
}

export async function moveCheck(ask: MoveAsk): Promise<MoveChecked> {
  if (pretendMover) {
    const taken = pretendMover.moves.some((m) => m.dest.toLowerCase() === (ask.dest ?? "").toLowerCase() && m.state !== "moved_back" && m.state !== "failed");
    return taken ? { ok: false, why: `${ask.dest} already exists; pick another place`, bytes: 0, files: 0, free: 0, removable: false } : { ok: true, why: "", bytes: 1.2e9, files: 840, free: 600e9, removable: false };
  }
  return invoke<MoveChecked>("move_check", { ask });
}

export async function moveStart(ask: MoveAsk, onProgress: (step: string, done: number, total: number) => void): Promise<MoveInfo> {
  if (pretendMover) {
    const total = 1.2e9;
    for (const step of ["copy", "check"]) for (let k = 0; k <= 4; k++) { onProgress(step, (total * k) / 4, total); await new Promise((r) => setTimeout(r, 40)); }
    const info: MoveInfo = { id: `m${pretendMover.moves.length + 1}`, app: ask.name, source: ask.folder, dest: ask.dest ?? "", link: !pretendMover.launcher(ask.name), bytes: total, files: 840, created: Date.now() / 1000, held: "(pretend)", state: "waiting", notes: [] };
    pretendMover.moves.unshift(info);
    return { ...info };
  }
  const ch = new Channel<[string, number, number]>();
  ch.onmessage = ([s, d, t]) => onProgress(s, d, t);
  return invoke<MoveInfo>("move_start", { ask, onProgress: ch });
}

export async function listMoves(): Promise<MoveInfo[]> {
  if (pretendMover) return pretendMover.moves.map((m) => ({ ...m }));
  if (!isDesktop()) return [];
  return invoke<MoveInfo[]>("list_moves");
}

export async function moveConfirm(id: string): Promise<MoveInfo> {
  if (pretendMover) {
    const m = pretendMover.moves.find((x) => x.id === id);
    if (!m || m.state !== "waiting") throw new Error("this move is not waiting for a check");
    m.state = "done";
    return { ...m };
  }
  return invoke<MoveInfo>("move_confirm", { id });
}

export async function moveBack(id: string, onProgress: (step: string, done: number, total: number) => void): Promise<MoveInfo> {
  if (pretendMover) {
    const m = pretendMover.moves.find((x) => x.id === id);
    if (!m || (m.state !== "waiting" && m.state !== "done")) throw new Error("this move cannot be moved back now");
    if (m.state === "done") for (let k = 0; k <= 4; k++) { onProgress("copy", (m.bytes * k) / 4, m.bytes); await new Promise((r) => setTimeout(r, 30)); }
    m.state = "moved_back";
    return { ...m };
  }
  const ch = new Channel<[string, number, number]>();
  ch.onmessage = ([s, d, t]) => onProgress(s, d, t);
  return invoke<MoveInfo>("move_back", { id, onProgress: ch });
}

export async function moveRecover(id: string): Promise<string> {
  if (pretendMover) return "nothing needed finishing";
  return invoke<string>("move_recover", { id });
}

// ---------- Looking at your own files (D143) ----------

export interface ViewOpened { url: string; kind: "video" | "picture" | "music" | "pdf" | "text"; mime: string; size: number }

/** Ask the desktop app to open a file of the latest scan for viewing (read-only). */
export async function viewOpen(path: string): Promise<ViewOpened> {
  if (!isDesktop()) throw new Error("Previews of your own files work in the desktop app (this page cannot read files).");
  return invoke<ViewOpened>("view_open", { path });
}

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
  const path = await save({ title: "Save this scan", defaultPath: suggestedName, filters: [{ name: "DiskCleaner scan", extensions: ["json"] }] });
  if (!path) return null;
  return invoke<Saved>("save_scan", { path });
}

export async function cancelScan(): Promise<void> {
  await invoke("cancel_scan");
}

// ---------- removing (desktop builds with the "cleaning" feature; a stand-in for browser checks) ----------

export interface PickRequest { path: string; checked: boolean; understood: boolean }
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
      const v = verdict(p.path);
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

export async function emptyExpired(days: number): Promise<[string, number][]> {
  if (pretend) return [];
  return invoke<[string, number][]>("empty_expired", { days });
}

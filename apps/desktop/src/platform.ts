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

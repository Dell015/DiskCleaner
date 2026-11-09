// The desktop app's commands (Tauri). In a plain browser these are not available and the page
// works with view files and the demo only.

import { Channel, invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type { LiveEvent } from "./live";

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

export async function startScan(path: string, allowDriveRoot: boolean, onEvent: (e: LiveEvent) => void): Promise<void> {
  const channel = new Channel<LiveEvent>();
  channel.onmessage = onEvent;
  await invoke("start_scan", { path, allowDriveRoot, withApps: true, onEvent: channel });
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

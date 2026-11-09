// Graphics settings, like in a game: presets from Low to Ultra, or each effect on its own.

export type QualityId = "low" | "medium" | "high" | "ultra" | "custom";
export type MotionLevel = "full" | "reduced" | "off";

export interface Quality {
  /** Soft light behind the biggest folders and the selection. */
  glow: boolean;
  /** Blur what is not related to the selected node (otherwise it is only greyed). */
  focusBlur: boolean;
  /** Curved links (if the style uses them). */
  curved: boolean;
  /** A light travels along each new link while scanning. */
  pulses: boolean;
  /** How many nodes before you zoom in. */
  budget: number;
  /** Growth and transitions. */
  motion: MotionLevel;
  /** How many names are drawn (0.2 sparse to 1 dense). */
  labels: number;
}

export const PRESETS: Record<Exclude<QualityId, "custom">, Quality> = {
  low: { glow: false, focusBlur: false, curved: false, pulses: false, budget: 700, motion: "reduced", labels: 0.35 },
  medium: { glow: false, focusBlur: false, curved: true, pulses: true, budget: 1300, motion: "full", labels: 0.5 },
  high: { glow: true, focusBlur: true, curved: true, pulses: true, budget: 2000, motion: "full", labels: 0.6 },
  ultra: { glow: true, focusBlur: true, curved: true, pulses: true, budget: 3500, motion: "full", labels: 0.75 },
};

export const PRESET_NAMES: Record<QualityId, string> = { low: "Low", medium: "Medium", high: "High", ultra: "Ultra", custom: "Custom" };

export function matchPreset(q: Quality): QualityId {
  for (const [id, p] of Object.entries(PRESETS)) {
    if (JSON.stringify(p) === JSON.stringify(q)) return id as QualityId;
  }
  return "custom";
}

/** The system asked for less motion. */
export function prefersReducedMotion(): boolean {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

/**
 * How sizes are written:
 * - "windows": counted in 1024s and labelled GB, MB (what File Explorer shows; the default),
 * - "binary": counted in 1024s and labelled GiB, MiB (the precise names),
 * - "decimal": counted in 1000s and labelled GB, MB (how drives are sold).
 */
export type SizeUnits = "windows" | "binary" | "decimal";
export const SIZE_UNITS: SizeUnits[] = ["windows", "binary", "decimal"];
let sizeUnits: SizeUnits = "windows";

export function setSizeUnits(u: SizeUnits): void {
  sizeUnits = u;
}

export function getSizeUnits(): SizeUnits {
  return sizeUnits;
}

/** Bytes as text in the chosen units ("1.50 GB"). */
export function human(bytes: number, units: SizeUnits = sizeUnits): string {
  const names = units === "binary" ? ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] : ["B", "KB", "MB", "GB", "TB", "PB"];
  const step = units === "decimal" ? 1000 : 1024;
  let n = bytes;
  let i = 0;
  while (n >= step && i < names.length - 1) {
    n /= step;
    i++;
  }
  // Keep three significant digits without ever showing "1000 MB" or "1024 MB".
  if (i > 0 && i < names.length - 1 && Number(n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)) >= step) {
    n /= step;
    i++;
  }
  const units_ = names;
  return i === 0 ? `${n} B` : `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${units_[i]}`;
}

/** Whole numbers with thousands separators. */
export function count(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

/** A Unix time as a short local date, or "unknown". */
export function date(secs: number): string {
  if (!secs) return "unknown";
  const d = new Date(secs * 1000);
  return d.toLocaleDateString("en-CA");
}

/** Small seeded random numbers (same seed, same sequence). */
export function rng(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 4294967296;
  };
}

/** Bytes as text, binary units ("1.50 GiB"). */
export function human(bytes: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return i === 0 ? `${n} B` : `${n.toFixed(n >= 100 ? 0 : n >= 10 ? 1 : 2)} ${units[i]}`;
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

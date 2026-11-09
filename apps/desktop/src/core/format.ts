// The `dc-view` file written by `dc view-data`, and its checks. Fail closed: anything odd rejects
// the whole file, so nothing half-loaded or contradictory is ever shown.

export const SCHEMA = "dc-view";
export const SCHEMA_VERSION = 1;

export type RiskId = "safe" | "probably_safe" | "dont_touch";
export type MarkSource = "rule" | "leftover" | "refused";

export interface ViewEvidence { points: number; text: string }

export interface Mark {
  node: number;
  risk: RiskId;
  source: MarkSource;
  title: string;
  reason: string;
  detail: string;
  confidence?: "low" | "medium" | "high";
  evidence: ViewEvidence[];
  offered: boolean;
  bytes: number;
}

export interface Owner { name: string; kind: "app" | "vendor" | "windows"; publisher?: string }

/** The file as written (columns of plain arrays). */
export interface ViewFile {
  schema: string;
  schema_version: number;
  tool_version?: string;
  created_unix?: number;
  roots: string[];
  categories: string[];
  nodes: {
    name: string[];
    parent: number[];
    kind: number[];
    size: number[];
    mtime: number[];
    flags: number[];
    cat: number[];
  };
  owners: Owner[];
  assignments: [number, number, number][];
  marks: Mark[];
}

/** Checked data in compact arrays (transferable from a worker). */
export interface ViewData {
  roots: string[];
  categories: string[];
  names: string[];
  parent: Int32Array;
  kind: Uint8Array;
  size: Float64Array;
  mtime: Float64Array;
  flags: Uint32Array;
  cat: Int8Array;
  owners: Owner[];
  /** Flat triples: node, owner, confidence. */
  assignments: Int32Array;
  marks: Mark[];
}

export const KIND = { file: 0, dir: 1, link: 2, other: 3 } as const;
export const FLAG = {
  link: 1, hardlink: 2, cloud: 4, system: 8, hidden: 16, notCounted: 32, compressed: 64, sparse: 128,
} as const;

export class ViewError extends Error {}

const RISKS: readonly string[] = ["safe", "probably_safe", "dont_touch"];
const SOURCES: readonly string[] = ["rule", "leftover", "refused"];
const OWNER_KINDS: readonly string[] = ["app", "vendor", "windows"];
const CONFIDENCES: readonly string[] = ["low", "medium", "high"];

function fail(why: string): never {
  throw new ViewError(why);
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function array(v: unknown, what: string): unknown[] {
  if (!Array.isArray(v)) fail(`${what} is missing or is not a list`);
  return v;
}

function int(v: unknown, what: string, min: number, max: number): number {
  if (typeof v !== "number" || !Number.isInteger(v) || v < min || v > max) {
    fail(`${what} has a bad value (${String(v).slice(0, 40)})`);
  }
  return v;
}

function text(v: unknown, what: string): string {
  if (typeof v !== "string") fail(`${what} is not text`);
  return v;
}

/** Check a parsed file and convert it. Throws `ViewError` with a plain message. */
export function checkView(raw: unknown): ViewData {
  if (!isObject(raw)) fail("This is not a DiskCleaner view file.");
  if (raw.schema !== SCHEMA) {
    fail("This is not a DiskCleaner view file. Make one with: dc view-data <scan.json> --out view.json");
  }
  if (raw.schema_version !== SCHEMA_VERSION) {
    fail(`This view file has version ${String(raw.schema_version)}; this viewer reads version ${SCHEMA_VERSION}. Make the file again with the same version of dc.`);
  }
  const roots = array(raw.roots, "roots").map((r, i) => text(r, `roots[${i}]`));
  const categories = array(raw.categories, "categories").map((c, i) => text(c, `categories[${i}]`));
  if (!isObject(raw.nodes)) fail("nodes are missing");
  const cols = raw.nodes;
  const names = array(cols.name, "nodes.name");
  const n = names.length;
  if (n < 1) fail("The file has no entries.");
  if (n > 50_000_000) fail("The file has too many entries for the viewer.");
  for (const c of ["parent", "kind", "size", "mtime", "flags", "cat"]) {
    if (array(cols[c], `nodes.${c}`).length !== n) fail(`nodes.${c} has a different length than nodes.name`);
  }
  const P = cols.parent as unknown[];
  const K = cols.kind as unknown[];
  const S = cols.size as unknown[];
  const M = cols.mtime as unknown[];
  const F = cols.flags as unknown[];
  const C = cols.cat as unknown[];
  const parent = new Int32Array(n);
  const kind = new Uint8Array(n);
  const size = new Float64Array(n);
  const mtime = new Float64Array(n);
  const flags = new Uint32Array(n);
  const cat = new Int8Array(n);
  const nameList: string[] = new Array(n);
  for (let i = 0; i < n; i++) {
    nameList[i] = text(names[i], `name of entry ${i}`);
    kind[i] = int(K[i], `kind of entry ${i}`, 0, 3);
    if (i === 0) {
      if (P[0] !== -1) fail("The first entry must be the root (parent -1).");
      if (kind[0] !== KIND.dir) fail("The root must be a folder.");
      parent[0] = -1;
    } else {
      const p = int(P[i], `parent of entry ${i}`, 0, i - 1);
      if (kind[p] !== KIND.dir) fail(`Entry ${i} is inside something that is not a folder.`);
      parent[i] = p;
    }
    size[i] = int(S[i], `size of entry ${i}`, 0, Number.MAX_SAFE_INTEGER);
    if (size[i] > 0 && kind[i] !== KIND.file) fail(`Entry ${i} is not a file but has its own size.`);
    mtime[i] = int(M[i], `time of entry ${i}`, -1e11, 1e11);
    flags[i] = int(F[i], `flags of entry ${i}`, 0, 255);
    cat[i] = int(C[i], `category of entry ${i}`, -1, categories.length - 1);
  }
  const owners = array(raw.owners, "owners").map((o, i): Owner => {
    if (!isObject(o)) fail(`owner ${i} is damaged`);
    const kindText = text(o.kind, `owner ${i} kind`);
    if (!OWNER_KINDS.includes(kindText)) fail(`owner ${i} has an unknown kind`);
    const out: Owner = { name: text(o.name, `owner ${i} name`), kind: kindText as Owner["kind"] };
    if (o.publisher !== undefined) out.publisher = text(o.publisher, `owner ${i} publisher`);
    return out;
  });
  const rawAssign = array(raw.assignments, "assignments");
  const assignments = new Int32Array(rawAssign.length * 3);
  rawAssign.forEach((a, i) => {
    const t = array(a, `assignment ${i}`);
    if (t.length !== 3) fail(`assignment ${i} is damaged`);
    assignments[i * 3] = int(t[0], `assignment ${i} entry`, 0, n - 1);
    assignments[i * 3 + 1] = int(t[1], `assignment ${i} owner`, 0, owners.length - 1);
    assignments[i * 3 + 2] = int(t[2], `assignment ${i} confidence`, 0, 2);
  });
  const marks = array(raw.marks, "marks").map((m, i): Mark => {
    if (!isObject(m)) fail(`mark ${i} is damaged`);
    const risk = text(m.risk, `mark ${i} risk`);
    if (!RISKS.includes(risk)) fail(`mark ${i} has an unknown risk`);
    const source = text(m.source, `mark ${i} source`);
    if (!SOURCES.includes(source)) fail(`mark ${i} has an unknown source`);
    if (typeof m.offered !== "boolean") fail(`mark ${i} does not say whether it is offered`);
    // Something marked "don't touch" can never be offered. A file that says otherwise is wrong.
    if (risk === "dont_touch" && m.offered) fail(`mark ${i} offers something marked "don't touch"`);
    if (source === "refused" && m.offered) fail(`mark ${i} offers something that was refused`);
    const out: Mark = {
      node: int(m.node, `mark ${i} entry`, 0, n - 1),
      risk: risk as RiskId,
      source: source as MarkSource,
      title: text(m.title, `mark ${i} title`),
      reason: text(m.reason, `mark ${i} reason`),
      detail: text(m.detail, `mark ${i} detail`),
      evidence: array(m.evidence, `mark ${i} evidence`).map((e, j) => {
        if (!isObject(e)) fail(`mark ${i} evidence ${j} is damaged`);
        return { points: int(e.points, `mark ${i} points`, 0, 1000), text: text(e.text, `mark ${i} evidence text`) };
      }),
      offered: m.offered,
      bytes: int(m.bytes, `mark ${i} size`, 0, Number.MAX_SAFE_INTEGER),
    };
    if (m.confidence !== undefined) {
      const c = text(m.confidence, `mark ${i} confidence`);
      if (!CONFIDENCES.includes(c)) fail(`mark ${i} has an unknown confidence`);
      out.confidence = c as Mark["confidence"];
    }
    return out;
  });
  return { roots, categories, names: nameList, parent, kind, size, mtime, flags, cat, owners, assignments, marks };
}

/** Parse text and check it. */
export function parseView(json: string): ViewData {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    fail("The file is not valid JSON (it may be cut off or damaged).");
  }
  return checkView(raw);
}

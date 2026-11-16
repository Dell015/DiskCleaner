// "Your files" for the Library view: the files no app owns, outside Windows and program folders,
// grouped by what they are (videos, pictures, music, documents, code projects, downloads, ...)
// and then by kind (screenshots, camera photos, recordings, PDFs, ...). Each file is counted in
// exactly one group. Read-only: this only describes the scan.

import { FLAG, KIND } from "./format";
import type { Tree } from "./tree";

export type GroupId = "videos" | "pictures" | "music" | "documents" | "projects" | "downloads" | "archives" | "other";

export const GROUP_TEXT: Record<GroupId, { title: string; about: string; mark: string }> = {
  videos: { title: "Videos", about: "Recordings, phone videos and other videos.", mark: "▶" },
  pictures: { title: "Pictures", about: "Screenshots, camera photos, wallpapers and other pictures.", mark: "◩" },
  music: { title: "Music & audio", about: "Songs, voice recordings and other sound files.", mark: "♪" },
  documents: { title: "Documents", about: "PDFs, Word, Excel, PowerPoint and text files.", mark: "≡" },
  projects: { title: "Code projects", about: "Folders with a project file (.git, package.json, Cargo.toml, ...). Counted whole, whatever is inside.", mark: "{}" },
  downloads: { title: "Downloads", about: "Everything in your Downloads folders.", mark: "↓" },
  archives: { title: "Archives & installers", about: "Zip, rar and 7z files, disk images and setup programs outside Downloads.", mark: "▣" },
  other: { title: "Other files", about: "Your files of other kinds: game files, programs and things Rhizome cannot sort.", mark: "…" },
};

export const GROUPS: GroupId[] = ["videos", "pictures", "music", "documents", "projects", "downloads", "archives", "other"];

export interface Kind {
  label: string;
  bytes: number;
  files: number;
  /** Folders holding the most of it (direct parent of the files), biggest first. */
  folders: { node: number; bytes: number; files: number }[];
  /** Its biggest files (projects: the project folders), biggest first. */
  biggest: number[];
}

export interface Group {
  id: GroupId;
  bytes: number;
  files: number;
  kinds: Kind[];
  /** Code projects only: bytes in folders that are rebuilt by the project's tools (node_modules, target, ...). */
  rebuildable?: number;
}

/** Folders at a drive's top level that hold Windows and programs, not your files. */
const SYSTEM_TOP = new Set(["windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information", "recovery", "$winreagent", "$windows.~bt", "$windows.~ws", "windows.old", "msocache", "perflogs", "intel", "amd", "nvidia", "drivers", "onedrivetemp", "$sysreset", "$getcurrent"]);

/** Files that mark a folder as a code project. */
const PROJECT_FILE = /^(\.git|package\.json|cargo\.toml|pyproject\.toml|requirements\.txt|setup\.py|go\.mod|pom\.xml|build\.gradle(\.kts)?|pubspec\.yaml|composer\.json|gemfile|cmakelists\.txt|makefile|.+\.sln|.+\.csproj|.+\.uproject|deno\.json|project\.godot)$/i;
/** Folders inside a project that its tools make again (listed, never offered from here). */
const REBUILT = new Set(["node_modules", "target", ".venv", "venv", "__pycache__", ".next", ".nuxt", ".svelte-kit", "dist", "build", "obj", ".gradle", ".dart_tool", "library", "intermediate", "binaries", "deriveddatacache", ".parcel-cache", ".turbo", ".angular", "coverage"]);

const EXT: Record<string, string> = {};
for (const [kind, list] of Object.entries({
  video: "mp4 mkv mov avi wmv webm flv m4v mpg mpeg ts m2ts 3gp vob",
  picture: "jpg jpeg png gif bmp webp heic heif tif tiff raw cr2 cr3 nef arw dng psd svg ico avif jfif",
  audio: "mp3 wav flac m4a aac ogg opus wma aiff mid midi flp als",
  pdf: "pdf",
  word: "doc docx odt rtf pages",
  sheet: "xls xlsx xlsm ods csv numbers",
  slides: "ppt pptx odp key",
  text: "txt md log json xml yaml yml ini epub mobi",
  archive: "zip rar 7z tar gz bz2 xz tgz zst cab",
  image: "iso img vhd vhdx vmdk dmg",
  installer: "exe msi msix msixbundle appx appxbundle apk",
})) for (const e of list.split(" ")) EXT[e] = kind;

function ext(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** What a folder's path says about the files in it (worked out once per folder). */
interface Where { recordings: boolean; camera: boolean; movies: boolean; screenshots: boolean; wallpapers: boolean; voice: boolean }

function where(path: string): Where {
  const p = path.toLowerCase();
  return {
    recordings: /\\(captures|recordings|obs|nvidia|radeon replay|medal|outplayed)\\/.test(p),
    camera: /\\dcim\\|\\camera roll\\|\\camera\\/.test(p),
    movies: /\\(movies|films|tv|shows|series|anime)\\/.test(p),
    screenshots: /\\screenshots?\\/.test(p),
    wallpapers: /\\wallpapers?\\/.test(p),
    voice: /\\(sound recordings|recordings|voice)\\/.test(p),
  };
}

/** Which group and kind a file goes to (outside projects and Downloads). Exported for tests. */
export function sortFile(name: string, path: string, category = "other"): [GroupId, string] {
  return sortIn(name, where(path), category);
}

function sortIn(name: string, w: Where, category: string): [GroupId, string] {
  const e = EXT[ext(name)];
  switch (e) {
    case "video": {
      const n = name.toLowerCase();
      if (w.recordings || /^\d{4}-\d{2}-\d{2}[ _]\d{2}-\d{2}-\d{2}/.test(n)) return ["videos", "Recordings & clips"];
      if (w.camera || /^(vid|pxl|mov)_?\d/.test(n)) return ["videos", "Phone & camera videos"];
      if (w.movies) return ["videos", "Movies & shows"];
      return ["videos", "Other videos"];
    }
    case "picture": {
      const n = name.toLowerCase();
      if (w.screenshots || /^(screenshot|screen shot|capture|snip)/.test(n)) return ["pictures", "Screenshots"];
      if (w.camera || /^(img|pxl|dsc|dscn|dji|gopr)_?\d/.test(n)) return ["pictures", "Camera photos"];
      if (w.wallpapers) return ["pictures", "Wallpapers"];
      if (/\.(psd|raw|cr2|cr3|nef|arw|dng)$/.test(n)) return ["pictures", "Photo & design originals"];
      return ["pictures", "Other pictures"];
    }
    case "audio":
      if (w.voice) return ["music", "Voice recordings"];
      if (/\.(flp|als|mid|midi)$/i.test(name)) return ["music", "Music projects"];
      return ["music", "Songs & sounds"];
    case "pdf": return ["documents", "PDFs"];
    case "word": return ["documents", "Word documents"];
    case "sheet": return ["documents", "Spreadsheets"];
    case "slides": return ["documents", "Presentations"];
    case "text": return ["documents", "Text & notes"];
    case "archive": return ["archives", "Archives (zip, rar, 7z)"];
    case "image": return ["archives", "Disk images (iso, vhd)"];
    case "installer": return ["archives", "Installers & programs"];
    default:
      // What the scan's own sorting says about the rest.
      switch (category) {
        case "code": return ["other", "Loose code files"];
        case "game_asset": return ["other", "Game files"];
        case "executable": return ["other", "Programs"];
        case "temporary": return ["other", "Temporary files"];
        default: return ["other", "Other files"];
      }
  }
}

/** The kind of a file inside Downloads. */
function downloadKind(name: string): string {
  switch (EXT[ext(name)]) {
    case "installer": return "Installers";
    case "archive": case "image": return "Archives & disk images";
    case "video": return "Videos";
    case "picture": return "Pictures";
    case "audio": return "Music & audio";
    case "pdf": case "word": case "sheet": case "slides": case "text": return "Documents";
    default: return "Other downloads";
  }
}

/** The kind of a code project, from the project files at its top. */
function projectKind(names: string[]): string {
  const has = (re: RegExp): boolean => names.some((n) => re.test(n));
  if (has(/\.uproject$/i)) return "Unreal projects";
  if (names.some((n) => n.toLowerCase() === "projectsettings") && names.some((n) => n.toLowerCase() === "assets")) return "Unity projects";
  if (has(/^project\.godot$/i)) return "Godot projects";
  if (has(/^cargo\.toml$/i)) return "Rust projects";
  if (has(/^(package\.json|deno\.json)$/i)) return "JavaScript & TypeScript projects";
  if (has(/^(pyproject\.toml|requirements\.txt|setup\.py)$/i)) return "Python projects";
  if (has(/\.(sln|csproj)$/i)) return ".NET projects";
  if (has(/^(pom\.xml|build\.gradle(\.kts)?)$/i)) return "Java & Android projects";
  if (has(/^pubspec\.yaml$/i)) return "Flutter projects";
  if (has(/^go\.mod$/i)) return "Go projects";
  if (has(/^composer\.json$/i)) return "PHP projects";
  return "Other projects";
}

/** Keep the `max` biggest of a list of (item, bytes). */
function top<T>(m: Map<T, number>, max: number): [T, number][] {
  return [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, max);
}

const cache = new WeakMap<Tree, { version: number; groups: Group[] }>();

/** Your files, grouped (biggest group first; empty groups left out). Cached per scan. */
export function yourFiles(tree: Tree): Group[] {
  const hit = cache.get(tree);
  if (hit && hit.version === tree.version) return hit.groups;
  const groups = build(tree);
  cache.set(tree, { version: tree.version, groups });
  return groups;
}

interface Acc { bytes: number; files: number; folders: Map<number, number>; folderFiles: Map<number, number>; big: Map<number, number>; /** Files this size or smaller cannot be among the biggest any more. */ floor: number }

function build(tree: Tree): Group[] {
  const acc = new Map<GroupId, Map<string, Acc>>();
  const rebuildable = { bytes: 0 };
  const add = (g: GroupId, kind: string, node: number, bytes: number, files: number, folder: number): void => {
    let kinds = acc.get(g);
    if (!kinds) acc.set(g, (kinds = new Map()));
    let a = kinds.get(kind);
    if (!a) kinds.set(kind, (a = { bytes: 0, files: 0, folders: new Map(), folderFiles: new Map(), big: new Map(), floor: -1 }));
    a.bytes += bytes;
    a.files += files;
    if (folder >= 0) {
      a.folders.set(folder, (a.folders.get(folder) ?? 0) + bytes);
      a.folderFiles.set(folder, (a.folderFiles.get(folder) ?? 0) + files);
    }
    // Keep the map small: pruned to the 200 biggest, so the 12 biggest are always in it.
    if (bytes > a.floor) {
      a.big.set(node, bytes);
      if (a.big.size > 800) {
        const kept = top(a.big, 200);
        a.big = new Map(kept);
        a.floor = kept[kept.length - 1][1];
      }
    }
  };

  // Walk the tree once from the top, carrying "in Downloads" down; projects are counted whole.
  const isDrive = (i: number): boolean => /^[a-z]:[\\/]?$/i.test(tree.names[i]);
  // Files of one folder come one after another: their folder's path is made once.
  let lastUp = -1;
  let lastWhere: Where = where("");
  const stack: [number, boolean][] = [];
  for (let c = tree.firstChild[0]; c >= 0; c = tree.nextSibling[c]) stack.push([c, false]);
  while (stack.length) {
    const [i, inDownloads] = stack.pop() as [number, boolean];
    // Anything an app owns is the app's (shown on its page), not yours.
    if (tree.owner[i] >= 0) continue;
    const name = tree.names[i];
    if (tree.kind[i] === KIND.dir) {
      const lower = name.toLowerCase().replace(/[\\/]+$/, "");
      const up = tree.parent[i];
      if (up >= 0 && isDrive(up) && SYSTEM_TOP.has(lower)) continue;
      if (lower === "appdata" || lower === "$recycle.bin") continue;
      if (!inDownloads) {
        // A code project: counted whole, with what its tools rebuild.
        let marker = false;
        let unity = 0;
        for (let c = tree.firstChild[i]; c >= 0 && !marker; c = tree.nextSibling[c]) {
          const k = tree.names[c];
          if (PROJECT_FILE.test(k)) marker = true;
          else if (k.length === 15 || k.length === 6) {
            const l = k.toLowerCase();
            if (l === "projectsettings") unity |= 1;
            else if (l === "assets") unity |= 2;
          }
        }
        if (marker || unity === 3) {
          const kids: string[] = [];
          for (let c = tree.firstChild[i]; c >= 0; c = tree.nextSibling[c]) kids.push(tree.names[c]);
          add("projects", projectKind(kids), i, tree.agg[i], tree.files[i], up);
          for (let c = tree.firstChild[i]; c >= 0; c = tree.nextSibling[c]) {
            if (tree.kind[c] === KIND.dir && REBUILT.has(tree.names[c].toLowerCase())) rebuildable.bytes += tree.agg[c];
          }
          continue;
        }
      }
      const dl = inDownloads || (lower === "downloads" && up >= 0 && tree.parent[up] >= 0 && tree.names[tree.parent[up]].toLowerCase().replace(/[\\/]+$/, "") === "users");
      for (let c = tree.firstChild[i]; c >= 0; c = tree.nextSibling[c]) stack.push([c, dl]);
      continue;
    }
    if (tree.kind[i] !== KIND.file) continue;
    const f = tree.flags[i];
    // Second names of a file are counted once; cloud placeholders are not on this disk.
    if (f & (FLAG.notCounted | FLAG.cloud)) continue;
    const bytes = tree.agg[i];
    const up = tree.parent[i];
    if (inDownloads) add("downloads", downloadKind(name), i, bytes, 1, up);
    else {
      if (up !== lastUp) {
        lastUp = up;
        lastWhere = where(`${tree.path(up)}\\`);
      }
      const [g, kind] = sortIn(name, lastWhere, tree.categories[tree.cat[i]] ?? "other");
      add(g, kind, i, bytes, 1, up);
    }
  }

  const out: Group[] = [];
  for (const [id, kinds] of acc) {
    const list: Kind[] = [...kinds.entries()].map(([label, a]) => ({
      label,
      bytes: a.bytes,
      files: a.files,
      folders: top(a.folders, 12).map(([node, bytes]) => ({ node, bytes, files: a.folderFiles.get(node) ?? 0 })),
      biggest: top(a.big, 12).map(([n]) => n),
    })).sort((a, b) => b.bytes - a.bytes);
    const g: Group = { id, bytes: list.reduce((s, k) => s + k.bytes, 0), files: list.reduce((s, k) => s + k.files, 0), kinds: list };
    if (id === "projects") g.rebuildable = rebuildable.bytes;
    out.push(g);
  }
  return out.filter((g) => g.files > 0).sort((a, b) => b.bytes - a.bytes);
}

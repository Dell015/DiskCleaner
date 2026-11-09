# F1: Graph view (frontend, read-only)

Status: built (prototype quality reached; see "As built"). Read-only: the view never deletes, moves or changes anything.

## Goal

An "Obsidian-style" picture of the disk: nodes and links that grow while a scan runs, that you
can drag and zoom, from the whole disk down to the smallest file. Findings (rules and leftovers)
are coloured by risk and explained with their evidence.

## Requirements

| id | requirement |
|----|-------------|
| V1 | Show a real scan of 1.5 million entries smoothly. The graph never holds more than a fixed budget of nodes (default 2,500, hard cap 6,000). |
| V2 | Zooming in opens folders that become big on screen; zooming out closes them again (with a gap between the two thresholds so nothing flickers). |
| V3 | Small children of a folder are grouped into one "N more items" node, so sizes always add up. |
| V4 | Live growth: nodes and links appear as the scan finds them; new links flash. The same code path is used for a replay now and for a live scan (Tauri) later. |
| V5 | Drag empty space to pan, scroll to zoom, drag a node to move and pin it, double-click a folder to open or close it. |
| V6 | Colour by risk (default), by app, or by file type. Legend always visible. |
| V7 | Details panel for the selected node: path with clickable parents, size, file count, last change, owner app, flags (link, cloud, hard link, system), finding with evidence, biggest items inside. |
| V8 | Findings panel: leftovers and rule candidates, split into "offered" and "shown only", with totals; clicking one flies to it (opening its parents). |
| V9 | Search by name or path; picking a result reveals and selects it. |
| V10 | Load a file exported by `dc view-data` (drag and drop or file picker), or a built-in demo. Loading runs in a worker; the page stays responsive. Nothing is uploaded. |
| V11 | A damaged or wrong file is rejected with a plain message (fail closed); nothing half-loaded is shown. |
| V12 | File names are shown as text only (never as HTML). |
| V13 | Light and dark themes follow the system. Usable at 1280x720 and up. Keyboard: `/` search, `Esc` clear, `F` fit, `Space` pause replay. |

## Data: `dc view-data`

`dc view-data <scan.json>... [--apps apps.json] --out view.json` writes one compact, columnar
JSON file (schema `dc-view`, version 1). Several scans are joined under a virtual root.
Per node: name, parent index (always smaller than the node's own index), kind, own size on disk,
last change, flags, file category, owner app. Plus a list of owners and a list of marks (rule
candidates, leftover findings, refused folders) with risk, evidence and whether they are offered.
It never overwrites a file and changes nothing on disk.

## Design

- **Tree** (`apps/desktop/src/core/tree.ts`): typed arrays that can grow (so a live scan can append), child
  lists as linked lists, sizes added up the chain as nodes arrive.
- **Level of detail** (`apps/desktop/src/core/lod.ts`): a pure function picks the visible nodes: start at
  the root, always open the biggest open-able node next (a priority queue), until the budget is
  used. Folders you opened, and folders the zoom asked for, go first. Children too small to get a
  node are summed into one "more items" node.
- **Graph sync** (`apps/desktop/src/graph/`): the visible set is compared with what sigma shows and only the
  difference is applied. New nodes start next to their parent, pointing away from the grandparent.
  d3-force lays it out (D86) and stops once things settle.
- **Semantic zoom**: after the camera moves, folders drawn with a radius of 24 px or more are
  opened, and auto-opened folders below 9 px are closed (D85).

## QA plan

Unit tests (vitest) for format validation, tree, level of detail, search and the demo generator.
Browser checks (Playwright) on the built page: demo loads, replay completes, budget respected,
zoom in opens and zoom out closes, drag moves and pins, clicking a finding selects it, search,
a file exported by the real `dc view-data` loads, a broken file is refused, no console errors,
screenshots light and dark. Timing at 1.5 million nodes. Rust tests for the export.

Done when every check passes and the screenshots look right at overview and close-up.

## As built

- Files: `apps/desktop/src/core/` (format, tree, lod, search, demo, build), `apps/desktop/src/graph/` (graph, layout,
  colours), `apps/desktop/src/panels/` (panels, dom), `apps/desktop/src/main.ts`, `apps/desktop/src/loader.worker.ts`.
  Backend: `crates/dc-cli/src/view_cmd.rs`.
- Iterations driven by screenshots: (1) giant overlapping circles: switched node size from area
  to size^0.42 and added a minimum dot size; (2) overview full of fans of small files: added a
  share-of-everything limit for automatic opening; (3) ForceAtlas2 clumps: switched to d3-force;
  (4) zooming in made the target drift away and edges grow thick: local layout after the first
  growth and edges in pixels; (5) forced labels overlapped: labels left to sigma's label grid;
  (6) a new file could inherit a camera move: camera reset and pending moves cancelled.
- Checks: 18 unit tests; browser checks all pass (empty state, growth, budget, zoom opens and
  closes, finding click, search, open/close, drag pins, apps tab, broken file refused and the old
  graph kept, HTML in names shown as text, a real `dc view-data` file loads, no page errors, CSP on).
- Known limits: N66-N68.

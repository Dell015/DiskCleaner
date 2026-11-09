# F2: Graph view polish (plan)

Status: plan, waiting for the owner's answers (end of this file). Nothing built yet.

## 1. What is wrong today (honest review of F1)

The graph works; the chrome around it looks like any generated tool. Specific tells found in our own UI:

| Where | Tell | Why it matters |
|---|---|---|
| Whole page | System font only, default blue accent (#0f8bd6), grey pill buttons | Looks like every other dev tool; no identity |
| Findings | Identical rounded cards with the same shadow and a coloured left stripe | The "SaaS card kit"; findings are a ranked list, so they want rows, not cards |
| Everywhere | Meta strings joined by middle dots ("14 · 22.1 GiB", "Showing … · 577 GiB") | Template chrome |
| Panel | All-caps tracked heading ("BIGGEST INSIDE"), pill badges on every card | Template chrome |
| Graph | Every node is the same flat disc; an open folder only gets lighter | Nothing says "folder you can open" vs "file" vs "more items" |
| Graph | Risk only by colour, red/green-ish | Colour-blind users cannot tell Safe from Don't touch |
| Motion | Nodes pop in; folders open without showing where children came from | Motion does not explain what changed |
| Layout | Header is a row of equal buttons; the most important control (where am I) is missing | No sense of place on a 1.5M-entry disk |

## 2. Principles

1. **The disk is the hero.** The graph gets the boldness; the chrome is quiet, flat and small.
2. **Readable paths beat pretty ones.** This tool asks people to judge real folders, so `l`, `I`
   and `1`, `0` and `O` must never be confused. Legibility drives the type choice.
3. **Risk is never colour alone.** Every risk has a colour *and* a shape.
4. **Motion explains, never decorates.** One orchestrated moment (the scan growing); everything
   else answers an action (open, close, select, fly to) and shows where things came from.
5. **Rows for lists, structure for data.** No card grids. Borders and rules only where they
   separate real groups.
6. **Plain words.** Sentence case, no eyebrows, no "→" on buttons, no invented numbers.

## 3. Identity (proposal: "survey")

The picture is a survey of a drive: a map you walk, measured exactly. Cool mineral paper in
light mode, blue-slate ink in dark mode, colours taken from colour-blind-safe sets.

### Colour tokens

| Token | Light | Dark | Role |
|---|---|---|---|
| `paper` | #E9EDEA | #121A21 | page and graph background (never pure white or #111) |
| `sheet` | #F6F8F6 | #18222B | panel surface (one step from paper, no shadow) |
| `ink` | #1B252D | #DCE3E0 | text |
| `ink-2` | #56636C | #8E9DA6 | secondary text |
| `rule` | #C9D2CE | #2A3843 | lines, edges |
| `sky` | #1F7FB8 | #56B4E9 | focus, selection, the scan "head" (Okabe-Ito sky) |
| `safe` | #00876A | #2CC5A0 | Safe (bluish green), drawn as a **solid** dot |
| `maybe` | #B87800 | #E6A532 | Probably safe (orange), drawn as a **ring** |
| `stop` | #C24A00 | #F07A3A | Don't touch (vermillion), drawn as a dot with a **notch/cross** |

Folders are drawn in `ink-2`, files a lighter tint of it, links as a dashed ring. App and type
modes keep a 10-colour categorical set checked for colour-blind distance (validator in QA).

### Type

- **Atkinson Hyperlegible Next** for the interface and **Atkinson Hyperlegible Mono** for paths
  and sizes (Braille Institute, open licence, made so similar characters never look alike;
  shipped inside the app, no font download). Tabular numbers for sizes so columns line up.
- Scale (1.2 ratio): 12 / 13 / 15 / 18 / 22 px. Weights 400 and 600 only.
- Labels in the graph: 12 px, 600 for folders, 400 for files, with a 2 px paper-coloured halo so
  they stay readable over edges.

## 4. Layout and navigation

```
+-------------------------------------------------------------------+
| C: > Users > Randel > AppData            [search……………]   ◐  ☰   |  <- path bar = where you are
+-------------------------------------------+-----------------------+
|                                           | Inspector             |
|                                           |  name, size bar       |
|                GRAPH                      |  what it is, owner    |
|                                           |  finding + evidence   |
|                                           |  biggest inside (rows)|
|   [risk|app|type]  detail ───○───         +-----------------------+
|   scale: ● 1 GB  ● 10 GB                  | Findings (rows)       |
|   minimap                                 |  offered / shown only |
+-------------------------------------------+-----------------------+
```

- **Path bar** replaces the brand row as the main header: clickable folders of the current
  focus; typing in it searches. The app name moves to the window title.
- **Graph controls** sit on the graph (bottom left, small): colour mode, detail, a size legend
  that shows what dot size means in GB, and a **minimap** of the whole picture with the view box.
- **Inspector and Findings** share the right column as two resizable parts, not tabs: you can
  see a finding list and the selected item at once.
- **Findings as rows**: risk shape, name, size (right-aligned, tabular), one line of evidence;
  click to expand all evidence. Sort by size or confidence; filter offered / shown only / refused.
- **Focus mode ("local graph")**: double-click a folder's name in the path bar or press Enter on
  a selected folder to make it the root of the picture; the path bar is the way back. Obsidian's
  most useful graph feature is the local graph; this is ours.
- **Filters**: by risk, by age (untouched for 6/12 months), by type, by minimum size. Filtered-out
  nodes fade to `rule` colour instead of vanishing, so the shape of the disk stays.
- **List alternative**: a keyboard-friendly tree list of the same visible set, for screen readers
  and for people who prefer rows (toggle).
- **Right-click menu** on nodes: Open folder, Focus here, Copy path, Show in Explorer (Tauri later).
- **History**: Back / Forward (Alt+Left / Alt+Right) through focus and selection.

## 5. The graph itself

- **Node shapes** (custom sigma node programs): folder = disc with a thin outer ring; open folder
  = hollow ring (its contents are around it); file = small disc; "N more" = dashed ring with the
  count inside when big enough; link = dashed ring, never filled.
- **Risk halo**: risk shape drawn as a second ring around the node so the folder colour stays.
- **Edges**: straight, 1 px, `rule` colour, opacity falling with depth from the focus; the path
  from focus to selection drawn in `sky`.
- **Labels**: biggest node per area (already), plus always the focus, the selection and the path to
  it. Folder labels show size on hover only.
- **Size legend**: two reference dots ("1 GB", "10 GB") that follow the current zoom.

## 6. Motion system

| Token | Duration | Easing | Use |
|---|---|---|---|
| `quick` | 120 ms | ease-out | hover, press, tooltip in |
| `standard` | 220 ms | cubic-bezier(0.2, 0, 0, 1) | panels, rows expanding, colour mode change (cross-fade) |
| `travel` | 480 ms | cubic-bezier(0.3, 0, 0, 1) | camera fly-to, fit |
| `grow` | 360 ms | ease-out, 8 ms stagger by size rank | children emerging |

- **The one orchestrated moment: the scan.** Edges extend from parent to child (drawn as a line
  growing over `grow`), the child dot appears at the tip; the scan head pulses softly in `sky`.
  Status line counts entries and size. Nothing else animates on its own.
- **Open a folder**: children start *at the parent's position* and spread out along their edges;
  the parent eases from disc to ring. **Close**: the reverse, children fold back into the parent.
- **Fly to**: camera eases with `travel`; if far, zoom out a little first, then in (a short arc),
  so you see where you are going.
- **Colour mode / theme change**: colours cross-fade over `standard`, nodes do not move.
- **Layout settling** is physics, not easing: keep d3-force, but cap velocity so nothing jumps.
- **Reduced motion** (system setting): no growth animation (nodes appear), camera cuts instead of
  flies, no pulse.

## 7. Words

- Empty state: one sentence of what it is, two actions: "Open a view file" and "Try the demo",
  plus the command to make a file. No headline slogan.
- Errors say what happened and what to do ("This file was made by a newer dc. Make it again with
  this version."). They never apologise.
- Same verb all the way: the button "Open folder" leads to the state "open"; "Focus here" leads to
  the path bar showing the focus.

## 8. Quality floor

- Contrast: text 4.5:1, graph labels with halo 4.5:1, risk shapes 3:1 against paper in both themes
  (checked by a script in QA).
- Keyboard: everything in panels reachable with Tab; arrow keys move through the tree list;
  visible focus ring in `sky`.
- Performance budget: 60 fps on the owner's PC with 1,500 nodes and 30 fps with 6,000; first
  picture of a 1.5M-entry file in under 3 s.
- Window sizes: 1280x720 to 4K; narrow (under 900 px) puts the right column in a drawer.

## 9. Things we will not do (slop list)

Purple or blue-to-purple gradients, glass blur panels, glowing neon nodes, particle backgrounds,
cards for list items, pill badges everywhere, all-caps eyebrows, middle-dot meta strings,
"→" on buttons, count-up numbers, fade-and-slide on every panel, bounce easing, emoji icons,
a slogan headline on the empty state, Inter/Geist/Space Grotesk by default.

## 10. Order of work (each step ends with screenshots in both themes and the browser checks)

1. Tokens: colours, type (self-hosted fonts), spacing; remove the tells in section 1.
2. Layout: path bar, inspector + findings split, graph controls, rows.
3. Node programs: shapes, rings, risk halos, dashed "more" and links; size legend.
4. Motion: tokens, open/close from the parent, edge growth, fly-to arc, reduced motion.
5. Focus mode, filters, minimap, history, right-click menu, tree list.
6. QA: contrast script, colour-blind simulation screenshots, keyboard pass, timing at 1.5M,
   compare before/after screenshots; repeat until it passes.

## 11. Questions for the owner

1. Identity: go with "survey" (cool mineral paper / blue-slate ink, section 3), or would you rather
   see two other directions first as screenshots?
2. Default theme: follow the system (current), or dark first?
3. Fonts: Atkinson Hyperlegible Next + Mono (legibility first), or something with more character?
4. Inspector + findings side by side in one column (proposal), or keep tabs?
5. Focus mode on double-click (proposal: double-click opens/closes, Enter/path bar focuses), or
   should double-click focus?
6. Risk shapes: solid / ring / notched dot as proposed?
7. Should the scan growth play every time a file is opened, or only for a live scan (with a
   "replay" button for files)?

## Sources consulted

- AI-slop tells and fixes: capitalandcompute.net "How to Fix AI Slop in Web Design, Tell by Tell";
  noqta.tn "AI design slop: overused UI patterns"; superdesign.dev "How to make AI UI look less generic".
- Graph usefulness: codeculture.store "Obsidian's Graph View Is Beautiful and Almost Completely
  Useless" (local graph, hierarchy, hairball past ~200 nodes); eleanorkonik.com "In Defense of
  Obsidian's Graph View" (filters, orphans at the edges, zoom and hover to explore).
- Motion: Fluent 2 motion guidance (duration by size and distance, short staggers, fades for top
  level changes); Atlassian design motion foundations.
- Colour-blind safe sets: Okabe and Ito palette.

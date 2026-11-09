# Node design system

How an entry on the disk becomes a node on screen. The rules live in code in
`apps/desktop/src/graph/encode.ts` (shape and colour), `apps/desktop/src/graph/graph.ts` (size, states, effects) and
`apps/desktop/src/graph/style.ts` (palettes). The key under the graph is drawn from the same rules, so it can
never disagree with the picture. Reference sheet: `node-design-system.png` (sent with this file).

## 1. Size: how much space it takes

- Radius = 34 × (bytes ÷ bytes of the picture's root) ^ 0.42, in graph units.
  Bigger is bigger, but softer than true area, so a 1 GB folder is still visible next to 400 GB.
- Never drawn smaller than a 2.2 px dot at any zoom (so everything stays clickable).
- The size scale under the graph shows two reference dots ("1 GiB", "10 GiB") at the current zoom.
- Zooming is semantic: a closed folder that becomes 24 px (radius) or more opens; a folder the
  zoom opened closes again below 9 px.

## 2. Shape: what it is, and what the checks say

| Shape | Meaning |
|---|---|
| Dot with a lighter rim | Folder (closed) |
| Hollow ring | Open folder: its contents are the nodes around it |
| Plain dot | File |
| Solid dot in the safe colour | Safe: regenerates or is junk |
| Normal dot inside a ring in the probably-safe colour | Probably safe: look first |
| Bullseye in the don't-touch colour | Don't touch (also: refused, shown only) |
| Thin hollow ring in the link colour | Link or junction, never followed |
| Ring with a pale centre | Cloud placeholder, not on this disk |
| Faint hollow ring | "N more items" too small to show at this detail |
| Accent ring | Selected (or hovered: a softer accent) |

Risk is always colour **and** shape, so it works for colour-blind eyes. Risk only goes up: an entry
shows the riskiest verdict of itself and the folders above it.

## 3. Colour: depends on "Colour by"

| Mode | Folders | Files |
|---|---|---|
| Risk (default) | folder colour of the look; risk colours replace it where a check spoke | a hint of the file type colour (55% toward the file grey) |
| App | owning app's colour (12 distinct hues; Windows in grey-blue); "no app" grey | same, slightly darker |
| Type | the folder's biggest file type, mixed 30% toward the folder colour | the file type colour |

File types (dc-core categories): Video, Pictures, Audio, Archives, Code, Documents, Game files,
Installers, Programs, Temporary, Other. Each has a fixed colour per theme (style.ts `cats`).

## 4. Labels

- Atkinson Hyperlegible Next: 700 for the picture's root, 600 for folders, 400 for files.
- A halo in the background colour keeps names readable over links.
- The biggest node in each area gets its name (sigma's label grid); names of the selected node and
  the folders above it are always shown. "More items" nodes are named on hover only.

## 5. States

| State | Look |
|---|---|
| New (scan or folder opened) | grows from 10% to full size over 360 ms; its link flashes in the accent colour; with pulses on, a light travels along the link |
| Opening a folder | children start at the folder's rim and spread out (existing nodes hold still) |
| Closing a folder | children fold back into it over 240 ms |
| Hovered | accent-tinted ring; the path above and its contents stay bright, the rest dims |
| Selected | accent ring, accent links on the path to it, glow (if on); the details drawer opens; everything unrelated is greyed and, with "focus blur" on, blurred |
| Scan head | accent ring on the folder being scanned |
| Pinned (dragged) | stays where it was dropped; "Unpin" in its details |
| Filtered by app | other apps fade to the background |

## 6. Links

Parent to child only. 1 px whatever the zoom; curved in the Deep field and Mycelium looks (when the
graphics setting allows), straight in Instrument. The path to the selection is drawn in the accent.

## 7. Looks and graphics

Three looks (palettes and drawing choices): Deep field, Instrument, Mycelium. Light and dark
versions of each. Graphics presets Low / Medium / High / Ultra, or each option on its own: glow,
focus blur, curved links, scan pulses, motion (full / reduced / off), detail (nodes before zooming),
and how many names are drawn. If the first scan replay runs slower than about 30 frames a second,
the preset steps down once and says so (only until you choose one yourself).

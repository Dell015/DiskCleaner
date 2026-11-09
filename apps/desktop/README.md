# DiskCleaner graph view

The disk as a graph: every folder is a node sized by the space it takes. It grows like a scan,
you can drag it and zoom from the whole disk down to single files, and findings are coloured by
risk with their evidence. **Read-only:** the page cannot delete, move or send anything.

## Run it

Needs Node.js 20.19 or newer.

```powershell
cd apps/desktop
npm install
npm run dev          # opens a local page; click "Try the demo"
```

## Look at your own disk

```powershell
cargo run -q -p dc-cli -- view-data data/me.json data/scan-pd.json data/scan-pf.json --apps data/apps.json --out data/view.json
```

Then open the page and drop `data/view.json` on it (or use "Open…"). The file stays on your computer.

## Use it

- Drag the background to move, scroll to zoom. Zooming in opens folders; zooming out closes them.
- Click a node for details; double-click a folder to open or close it; drag a node to pin it.
- `/` search, `Esc` clear, `F` fit, `Space` pause the scan replay.
- Colour by Risk, App or Type. "Detail" sets how many nodes show before you zoom in.

## Checks

```powershell
npm test             # unit tests
npm run build        # type check + production build (with a Content-Security-Policy)
npm run preview      # serve the build, then in another terminal:
npm run e2e -- http://127.0.0.1:4173/ ..\..\data\view.json   # browser checks (needs Playwright)
```

## Folders

```
apps/desktop/
  index.html          the page
  src/
    main.ts           wiring: loading, replay, selection, settings
    loader.worker.ts  reads and checks a view file off the main thread
    core/             data only, no drawing: format check, tree, level of detail, search, demo disk
    graph/            drawing: sigma layers, layout, node design system (encode.ts), looks, graphics settings
    panels/           side column, details drawer, key, small DOM helpers
    style.css
  tests/
    unit/             vitest: format, tree, level of detail, search, demo
    e2e/              browser checks and timing (Playwright)
```

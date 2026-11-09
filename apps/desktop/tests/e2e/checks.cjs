// Browser checks for the graph view. Run against `npm run preview`:
//   node tests/e2e/checks.cjs [base-url] [view-file.json]
// Needs Playwright (npm i -D playwright, or a global install). Screenshots go to tests/e2e/out/.
const path = require("path");
const fs = require("fs");
const { chromium } = require("playwright");

const base = process.argv[2] || "http://127.0.0.1:4173/";
const viewFile = process.argv[3];
const out = path.join(__dirname, "out");
fs.mkdirSync(out, { recursive: true });
let failures = 0;
const check = (ok, what) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${what}`);
  if (!ok) failures++;
};

(async () => {
  const browser = await chromium.launch({ args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e.stack || e)));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  const st = () => page.evaluate(() => {
    const s = window.__dc.state;
    return { items: s.vis ? s.vis.items.length : 0, open: s.vis ? s.vis.open.size : 0, zoom: s.zoomOpened.size, running: s.replay.running, selected: s.selected, n: s.tree ? s.tree.n : 0, truncated: s.vis ? s.vis.truncated : false };
  });
  const shot = (name) => page.screenshot({ path: path.join(out, `${name}.png`) });

  try {
    await page.goto(base);
    check(await page.isVisible("#empty"), "empty state is shown first");
    await shot("00-empty");

    // Demo: grows, then settles.
    await page.click("#emptyDemo");
    await page.waitForFunction(() => window.__dc.state.replay.running, null, { timeout: 30000 });
    await page.waitForTimeout(1500);
    await shot("01-growing");
    const mid = await st();
    check(mid.running && mid.items > 1, `graph grows during the scan (${mid.items} nodes so far)`);
    await page.waitForFunction(() => !window.__dc.state.replay.running, null, { timeout: 90000 });
    await page.waitForTimeout(7000);
    await shot("02-overview");
    const done = await st();
    check(done.items > 200 && done.items <= 1500, `overview respects the detail budget (${done.items} nodes)`);
    check(!(await page.isVisible("#replay")), "scan bar hides when done");

    // Zoom in on a closed folder, a few steps at a time like a person: folders open.
    const box = await page.locator("#graph").boundingBox();
    const aimAt = () => page.evaluate(() => {
      const v = window.__dc.view, s = window.__dc.state, t = s.tree;
      let best = null, bestR = -1;
      for (const it of s.vis.items) {
        if (it.node < 0 || it.open || t.kind[it.node] !== 1 || t.childCount[it.node] === 0 || !v.onScreen(it.key, -20)) continue;
        const r = v.pixelRadius(it.key);
        if (r > bestR) { bestR = r; best = it.key; }
      }
      if (!best) return null;
      const a = v.graph.getNodeAttributes(best);
      return v.sigma.graphToViewport({ x: a.x, y: a.y });
    });
    for (let round = 0; round < 5; round++) {
      const aim = await aimAt();
      await page.mouse.move(box.x + (aim ? aim.x : box.width / 2), box.y + (aim ? aim.y : box.height / 2));
      for (let i = 0; i < 3; i++) { await page.mouse.wheel(0, -260); await page.waitForTimeout(120); }
      await page.waitForTimeout(900);
      if ((await st()).zoom > 0) break;
    }
    await page.waitForTimeout(1500);
    const zin = await st();
    await shot("03-zoomed-in");
    check(zin.zoom > 0, `zooming in opens folders (${zin.zoom} opened by zoom, ${zin.items} nodes)`);
    const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
    await page.mouse.move(cx, cy);
    for (let i = 0; i < 20; i++) { await page.mouse.wheel(0, 400); await page.waitForTimeout(60); }
    await page.waitForTimeout(2500);
    const zout = await st();
    check(zout.zoom < zin.zoom || zin.zoom === 0, `zooming out closes them again (${zout.zoom} left)`);
    check(zout.items <= 6000, "never more than the hard cap");

    // Findings: click the first card, it is selected and shown.
    await page.click("#tab-findings .row");
    await page.waitForTimeout(1500);
    const sel = await st();
    check(sel.selected !== null, `clicking a finding selects it (${sel.selected})`);
    check(await page.isVisible("#drawer.open h2"), "details drawer opens");
    await shot("04-finding-selected");

    // Search.
    await page.fill("#q", "roaming\\norton");
    await page.waitForTimeout(500);
    const hits = await page.locator("#hits button").count();
    check(hits >= 1, `search finds by path (${hits} hits)`);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(1500);
    const title = await page.textContent("#drawer h2");
    check(title === "Norton", `search result is selected (${title})`);
    await shot("05-search");

    // Double-click opens a folder (via the details button, which uses the same path).
    const before = await st();
    const toggle = page.locator("#drawer .actions button", { hasText: /Open folder|Close folder/ });
    if (await toggle.count()) {
      await toggle.first().click();
      await page.waitForTimeout(800);
      const after = await st();
      check(after.open !== before.open, `open/close folder changes what is shown (${before.open} -> ${after.open} open)`);
    }

    // Drag a node: it moves and is pinned (with nothing selected, every node can be grabbed).
    await page.keyboard.press("Escape");
    await page.keyboard.press("f");
    await page.waitForTimeout(1200);
    // The layout may still be settling, so aim again right before grabbing, and try a few nodes.
    let pinned = false;
    let found = false;
    for (let attempt = 0; attempt < 4 && !pinned; attempt++) {
      const drag = await page.evaluate((n) => {
        const v = window.__dc.view;
        const keys = v.graph.nodes().filter((k) => v.onScreen(k, -40) && v.pixelRadius(k) > 3);
        const k = keys[(Math.floor(keys.length / 2) + n * 7) % Math.max(1, keys.length)];
        if (!k) return null;
        const a = v.graph.getNodeAttributes(k);
        const p = v.sigma.graphToViewport({ x: a.x, y: a.y });
        return { k, x: p.x, y: p.y };
      }, attempt);
      if (!drag) continue;
      found = true;
      await page.mouse.move(box.x + drag.x, box.y + drag.y);
      await page.mouse.down();
      for (let i = 1; i <= 10; i++) await page.mouse.move(box.x + drag.x + i * 12, box.y + drag.y + i * 6);
      await page.mouse.up();
      await page.waitForTimeout(400);
      pinned = await page.evaluate((k) => window.__dc.view.isPinned(k), drag.k);
    }
    if (found) {
      check(pinned, "dragging a node pins it");
    } else check(false, "found a node to drag");

    // Colour modes and themes.
    await page.click('#controls .seg button[data-mode="app"]');
    await page.waitForTimeout(600);
    await shot("06-by-app");
    await page.click('#controls .seg button[data-mode="type"]');
    await page.waitForTimeout(600);
    await shot("07-by-type");
    await page.click('#controls .seg button[data-mode="risk"]');
    await page.click(".tabs button[data-tab=apps]");
    check((await page.locator("#tab-apps .rows li").count()) > 3, "apps tab lists apps");
    await page.click("#settingsBtn");
    await page.click('#themes button[data-theme="light"]');
    await page.waitForTimeout(400);
    await shot("08a-light");
    await page.click('#themes button[data-theme="dark"]');
    await page.keyboard.press("Escape");
    await page.keyboard.press("Escape");
    await page.keyboard.press("f");
    await page.waitForTimeout(900);
    await shot("08-dark");

    // A damaged file is refused with a message, and the current graph stays.
    const bad = path.join(out, "broken.json");
    fs.writeFileSync(bad, '{"schema":"dc-view","schema_version":1,"roots":[]');
    await page.setInputFiles("#file", bad);
    await page.waitForSelector("#error:not([hidden])", { timeout: 10000 });
    const msg = await page.textContent("#errorText");
    check(/not valid JSON/.test(msg), `broken file refused: "${msg}"`);
    check((await st()).n > 1000, "previous graph is kept after a refused file");
    await page.click("#errorClose");

    // A file name with HTML in it is shown as text.
    const xss = path.join(out, "names.json");
    fs.writeFileSync(xss, JSON.stringify({ schema: "dc-view", schema_version: 1, roots: ["C:\\"], categories: [],
      nodes: { name: ["This PC", "C:\\", "<img src=x onerror=window.__pwned=1>", "a.txt"], parent: [-1, 0, 1, 2], kind: [1, 1, 1, 0], size: [0, 0, 0, 10], mtime: [0, 0, 0, 0], flags: [0, 0, 0, 0], cat: [-1, -1, -1, -1] },
      owners: [], assignments: [], marks: [{ node: 2, risk: "safe", source: "rule", title: "<b>x</b>", reason: "r", detail: "d", evidence: [], offered: true, bytes: 10 }] }));
    await page.setInputFiles("#file", xss);
    await page.waitForFunction(() => window.__dc.state.tree && window.__dc.state.tree.n === 4 && !window.__dc.state.replay.running, null, { timeout: 15000 });
    await page.click(".tabs button[data-tab=findings]");
    await page.click("#tab-findings .row");
    await page.waitForTimeout(500);
    check(await page.evaluate(() => window.__pwned === undefined), "file names are never run as HTML");
    check((await page.textContent("#drawer h2")).includes("<img"), "odd names are shown as plain text");

    if (viewFile) {
      await page.setInputFiles("#file", viewFile);
      await page.waitForFunction(() => window.__dc.state.tree && !window.__dc.state.replay.running && window.__dc.state.tree.n > 4, null, { timeout: 60000 });
      await page.waitForTimeout(4000);
      await shot("09-real-export");
      const s = await st();
      check(s.n > 4 && s.items > 1, `file from dc view-data loads (${s.n} entries, ${s.items} nodes)`);
    }
  } catch (e) {
    check(false, `script error: ${e.message.split("\n").slice(0, 3).join(" ")}`);
  } finally {
    check(errors.length === 0, `no errors in the page${errors.length ? ": " + errors.slice(0, 3).join(" | ") : ""}`);
    await browser.close();
    console.log(failures === 0 ? "ALL PASSED" : `${failures} FAILED`);
    process.exit(failures === 0 ? 0 : 1);
  }
})();

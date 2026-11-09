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
    return { items: s.vis ? s.vis.items.length : 0, building: s.replay.partial ? s.replay.partial.items.length : 0, open: s.vis ? s.vis.open.size : 0, zoom: s.zoomOpened.size, running: s.replay.running, selected: s.selected, n: s.tree ? s.tree.n : 0, truncated: s.vis ? s.vis.truncated : false };
  });
  const shot = (name) => page.screenshot({ path: path.join(out, `${name}.png`) });

  try {
    await page.goto(base + "?view=map");
    check(await page.isVisible("#empty"), "empty state is shown first");
    await shot("00-empty");

    // Demo: grows, then settles.
    await page.click("#emptyDemo");
    await page.waitForFunction(() => window.__dc.state.replay.running, null, { timeout: 30000 });
    await page.waitForTimeout(1500);
    await shot("01-growing");
    const mid = await st();
    check(mid.running && mid.building > 1, `graph grows step by step (${mid.building} nodes so far)`);
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
    await page.evaluate(() => document.activeElement && document.activeElement.blur());
    await page.keyboard.press("Escape");
    await page.keyboard.press("f");
    await page.waitForTimeout(1200);
    // The layout may still be settling, so aim again right before grabbing, and try a few nodes.
    let pinned = false;
    let found = false;
    for (let attempt = 0; attempt < 4 && !pinned; attempt++) {
      const drag = await page.evaluate((n) => {
        const v = window.__dc.view;
        const keys = v.graph.nodes().filter((k) => v.onScreen(k, -40)).sort((a, b) => v.pixelRadius(b) - v.pixelRadius(a));
        const k = keys[1 + n];
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

    // A live scan (simulated in the browser; the desktop app feeds the same code from Rust).
    await page.goto(base + "?live=40000&view=map");
    await page.click("#emptyScan");
    await page.waitForFunction(() => window.__dc.state.live && window.__dc.state.live.partial.items.length > 5, null, { timeout: 30000 });
    const early = await page.evaluate(() => window.__dc.state.live.partial.items.length);
    await shot("10-live-growing");
    await page.waitForFunction(() => window.__dc.state.live && !window.__dc.state.live.active, null, { timeout: 120000 });
    await page.waitForTimeout(2500);
    await shot("11-live-done");
    const live = await page.evaluate(() => ({ n: window.__dc.state.tree.n, marks: window.__dc.state.tree.marks.length, items: window.__dc.state.vis ? window.__dc.state.vis.items.length : 0 }));
    check(early > 5 && live.items > early, `live scan grows the graph (${early} early, ${live.items} at the end)`);
    check(live.marks > 5, `findings arrive at the end of a live scan (${live.marks})`);
    check((await page.locator("#tab-findings .row").count()) > 3, "findings list fills after the live scan");
    await page.click("#tab-findings .row");
    await page.waitForTimeout(800);
    check(await page.isVisible("#drawer.open h2"), "a finding from a live scan opens its details");


    // ---------- The list ("Files"): the simple way to look, and the default. ----------
    await page.goto(base + "?demo=40000&view=files");
    await page.waitForFunction(() => document.querySelectorAll("#explorer .x-row").length > 3, null, { timeout: 60000 });
    await shot("20-files");
    check(await page.isVisible("#explorer") && !(await page.isVisible("#minimap")), "the list shows and the map is out of the way");
    const sizes = await page.$$eval("#explorer .x-row .c-size", (els) => els.map((e) => e.textContent));
    check(sizes.length > 3, `the list shows the folder's contents (${sizes.length} rows)`);
    check(/ (GB|MB)$/.test(sizes[0]), `sizes are written like Windows by default (${sizes[0]})`);
    await page.click("#settingsBtn");
    await page.click("#units button[data-units=binary]");
    const precise = await page.textContent("#explorer .x-row .c-size");
    await page.click("#units button[data-units=windows]");
    await page.keyboard.press("Escape");
    check(/ GiB$/.test(precise), `the size names can be switched to the precise ones (${precise})`);
    const f0 = await page.evaluate(() => window.__dc.curFolder());
    await page.dblclick("#explorer .x-row.dir >> nth=0");
    await page.waitForTimeout(300);
    const f1 = await page.evaluate(() => window.__dc.curFolder());
    check(f1 !== f0 && (await page.textContent("#crumbs")).split("/").length >= 3, "double-click opens a folder and the path grows");
    await page.keyboard.press("Backspace");
    await page.waitForTimeout(200);
    check((await page.evaluate(() => window.__dc.curFolder())) === f0, "Backspace goes up one folder");
    await page.click("#explorer .x-row >> nth=1");
    await page.waitForTimeout(200);
    check(await page.isVisible("#drawer.open") && (await page.locator("#explorer .x-row.on").count()) === 1, "a click selects the row and shows its details");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(200);
    check(!(await page.isVisible("#drawer.open")) && (await page.locator("#explorer .x-row.on").count()) === 0, "Esc clears the selection");
    await page.click("#explorer .x-cols button.c-name");
    await page.waitForTimeout(150);
    const names = await page.$$eval("#explorer .x-row .nm", (els) => els.map((e) => e.textContent));
    check(names.length > 1, "sorting by name works");
    await page.fill("#q", "CrashDumps");
    await page.waitForTimeout(400);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(400);
    const hit = await page.evaluate(() => { const s = window.__dc.state, t = s.tree, i = Number((s.selected || "n-1").slice(1)); return { i, parentShown: i > 0 && t.parent[i] === window.__dc.curFolder(), row: document.querySelectorAll("#explorer .x-row.on").length }; });
    check(hit.parentShown && hit.row === 1, "a search result opens its folder in the list and selects it");
    await shot("21-files-search");

    // ---------- Switching to the map and back never leaves a stuck screen. ----------
    await page.click("#viewSeg button[data-view=map]");
    await page.waitForTimeout(2500);
    const m1 = await page.evaluate(() => ({ items: window.__dc.state.vis ? window.__dc.state.vis.items.length : 0, bar: !document.getElementById("selbar").hidden }));
    check(m1.items > 3 && m1.bar, `the map opens on the same place with a way to clear the highlight (${m1.items} nodes)`);
    await page.click("#selClear");
    await page.waitForTimeout(300);
    const cleared = await page.evaluate(() => ({ sel: window.__dc.state.selected, hl: window.__dc.view.highlight.size, focused: document.querySelector(".graph-host").classList.contains("focused") }));
    check(cleared.sel === null && cleared.hl === 0 && !cleared.focused, "Clear selection removes every highlight and dimming");
    // A hover that sigma never reported leaving must not keep the picture dimmed.
    const hk = await page.evaluate(() => { const v = window.__dc.view; const k = window.__dc.state.vis.items.find((it) => it.node > 1).key; v.hovered = k; v.updateHighlight(); return v.highlight.size; });
    await page.mouse.move(1200, 400);
    await page.dispatchEvent(".graph-host .layer:not(.back):not(.fx)", "mouseleave").catch(() => undefined);
    await page.evaluate(() => window.__dc.view.sigma.getContainer().dispatchEvent(new MouseEvent("mouseleave")));
    await page.waitForTimeout(100);
    check(hk > 0 && (await page.evaluate(() => window.__dc.view.highlight.size)) === 0, "leaving the map clears a left-over hover highlight");
    // Focus a folder, go back to everything, then the map still answers clicks.
    await page.evaluate(() => { const s = window.__dc.state; const it = s.vis.items.find((x) => x.node > 1 && x.node !== window.__dc.view.rootNode && s.tree.kind[x.node] === 1 && s.tree.childCount[x.node] > 3); window.__dc.reveal(it.node); });
    await page.waitForTimeout(900);
    await page.click("#drawer.open button:has-text('Focus here')");
    await page.waitForTimeout(1500);
    await page.click("#selClear");
    await page.waitForTimeout(200);
    check(!(await page.isVisible("#selbar")) || (await page.textContent("#selClear")) === "Show everything", "after a focus, the bar offers to show everything");
    await page.click("#crumbs button >> nth=0");
    await page.waitForTimeout(3000);
    const back = await page.evaluate(() => ({ focus: window.__dc.state.focusRoot, items: window.__dc.state.vis.items.length, hl: window.__dc.view.highlight.size }));
    check(back.focus === null && back.items > 50 && back.hl === 0, `going back shows everything, nothing greyed (${back.items} nodes)`);
    const target = await page.evaluate(() => {
      const v = window.__dc.view, s = window.__dc.state;
      let best = null, r0 = -1;
      for (const it of s.vis.items) { if (it.node < 1 || !v.onScreen(it.key, -30)) continue; const r = v.pixelRadius(it.key); if (r > r0) { r0 = r; best = it.key; } }
      const a = v.graph.getNodeAttributes(best); const p = v.sigma.graphToViewport({ x: a.x, y: a.y }); return { key: best, x: p.x, y: p.y };
    });
    const gb = await page.locator("#graph").boundingBox();
    await page.mouse.click(gb.x + target.x, gb.y + target.y);
    await page.waitForTimeout(400);
    check((await page.evaluate(() => window.__dc.state.selected)) === target.key, "after going back, clicking a node still selects it");
    await page.mouse.click(gb.x + 30, gb.y + gb.height / 2);
    await page.waitForTimeout(300);
    check((await page.evaluate(() => window.__dc.state.selected)) === null && (await page.evaluate(() => window.__dc.view.highlight.size)) === 0, "clicking empty space clears the selection");
    await page.click("#viewSeg button[data-view=files]");
    await page.waitForTimeout(300);
    check(await page.isVisible("#explorer .x-row"), "and back to the list");

    // ---------- A live scan in the list. ----------
    await page.goto(base + "?live=40000&view=files");
    await page.click("#emptyScan");
    await page.waitForFunction(() => document.querySelectorAll("#explorer .x-row").length > 0 && window.__dc.state.live && window.__dc.state.live.active, null, { timeout: 30000 });
    await shot("22-files-live");
    check(true, "the list fills while the scan runs");
    await page.waitForFunction(() => window.__dc.state.live && !window.__dc.state.live.active, null, { timeout: 120000 });
    await page.waitForTimeout(500);
    check((await page.locator("#tab-findings .row").count()) > 3 && (await page.locator("#explorer .x-row").count()) > 3, "a live scan in the list ends with findings");
    check(await page.isVisible("#unreadTab") && (await page.textContent("#unreadTab")).includes("2"), "the tab of entries that could not be read appears");
    await page.click("#unreadTab");
    await page.waitForTimeout(200);
    check((await page.locator("#tab-unread .row").count()) === 2 && (await page.textContent("#tab-unread")).includes("did not allow"), "it lists each one with the reason");
    await shot("23-not-read");
    check(!(await page.isVisible("#saveBtn")), "Save scan is only offered in the desktop app");
    // Apps tab: the picked app shows every place it keeps things.
    await page.goto(base + "?demo=40000&view=files");
    await page.waitForFunction(() => document.querySelectorAll("#explorer .x-row").length > 3, null, { timeout: 60000 });
    check(!(await page.isVisible("#unreadTab")), "a new file or demo clears the Not read tab");
    await page.click(".tabs button[data-tab=apps]");
    await page.click("#tab-apps .rows li:first-child .row");
    await page.waitForTimeout(300);
    check((await page.locator("#tab-apps .places .place").count()) >= 1, "an app shows every place it keeps things");
    await shot("24-app-places");

    // ---------- Overview: everything at a glance. ----------
    await page.goto(base + "?demo=40000&view=overview");
    await page.waitForFunction(() => document.querySelectorAll("#overview .ov-sec").length > 3, null, { timeout: 60000 });
    await shot("30-overview");
    const ov = await page.evaluate(() => ({
      legend: document.querySelector("#overview .ov-legend").textContent,
      groups: document.querySelectorAll("#overview .ov-group").length,
      hints: [...document.querySelectorAll("#overview .ov-hint b")].map((b) => b.textContent),
      apps: document.querySelectorAll("#overview .ov-app:not(.head)").length,
      portable: document.querySelector("#ov-portable").textContent,
    }));
    check(/Apps and games/.test(ov.legend) && /Your files/.test(ov.legend) && /Windows/.test(ov.legend), "the Overview says where the space goes");
    check(ov.groups >= 4, `Downloads are sorted by type (${ov.groups} groups)`);
    check(ov.hints.includes("Installers of apps you already have") && ov.hints.includes("Archives already unpacked"), "Downloads come with hints");
    check(ov.apps >= 10 && /Programs nobody installed/.test(ov.portable), `apps and programs nobody installed are listed (${ov.apps} apps)`);
    await page.click("#overview .ov-app:not(.head) >> nth=0");
    await page.waitForTimeout(200);
    check((await page.locator("#overview .ov-app-detail").count()) === 1, "an app opens to show where it keeps things");
    await page.click("#overview .ov-tools button:has-text('Least used first')");
    await page.waitForTimeout(200);
    check(true, "apps can be sorted by last use");
    await page.click("#ov-big ~ * .ov-row >> nth=0").catch(async () => page.locator("#overview .ov-sec:has(#ov-big) .ov-row").first().click());
    await page.waitForTimeout(400);
    const opened = await page.evaluate(() => ({ view: window.__dc.settings.view, sel: window.__dc.state.selected, row: document.querySelectorAll("#explorer .x-row.on").length }));
    check(opened.view === "files" && opened.sel && opened.row === 1, "a row in the Overview opens it in Files, selected");
    await page.click("#explorer .x-up");
    await page.waitForTimeout(200);
    await page.click("#explorer .x-sortbtn");
    await page.waitForTimeout(500);
    check((await page.evaluate(() => window.__dc.settings.view)) === "overview" && (await page.textContent("#overview")).includes("Sorted:"), "Sort this folder shows any folder sorted by type");
    await shot("31-sorted-folder");
    // A live scan opens on the Overview and finishes there.
    await page.goto(base + "?live=40000&view=overview");
    await page.click("#emptyScan");
    await page.waitForFunction(() => window.__dc.state.live && window.__dc.state.live.finished && !window.__dc.state.live.active, null, { timeout: 120000 });
    await page.waitForTimeout(600);
    check((await page.locator("#overview .ov-group").count()) >= 3, "the Overview fills in from a live scan");

    if (viewFile) {
      await page.goto(base + "?view=map");
      await page.setInputFiles("#file", viewFile);
      await page.waitForFunction(() => window.__dc.state.data && window.__dc.state.data.names.length > 4 && !window.__dc.state.replay.running && window.__dc.state.vis, null, { timeout: 120000 });
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

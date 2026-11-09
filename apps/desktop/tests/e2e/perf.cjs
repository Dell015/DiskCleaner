// Timing in the browser at real scale: node tests/e2e/perf.cjs [base-url] [entries]
const { chromium } = require("playwright");
const base = process.argv[2] || "http://127.0.0.1:4173/";
const n = Number(process.argv[3] || 1500000);
(async () => {
  const b = await chromium.launch({ args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const errs = [];
  p.on("pageerror", (e) => errs.push(String(e)));
  const t0 = Date.now();
  await p.goto(`${base}?demo=${n}`);
  await p.waitForFunction(() => window.__dc && window.__dc.state.replay.running, null, { timeout: 120000 });
  const loaded = Date.now() - t0;
  await p.click("#speed"); // middle speed
  await p.waitForFunction(() => !window.__dc.state.replay.running, null, { timeout: 300000 });
  const grown = Date.now() - t0;
  await p.waitForTimeout(6000);
  // Frame times while panning and zooming.
  const box = await p.locator("#graph").boundingBox();
  await p.evaluate(() => { window.__frames = []; let last = performance.now(); const f = (t) => { window.__frames.push(t - last); last = t; if (window.__frames.length < 400) requestAnimationFrame(f); }; requestAnimationFrame(f); });
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await p.mouse.down();
  for (let i = 0; i < 30; i++) await p.mouse.move(box.x + box.width / 2 + i * 8, box.y + box.height / 2 + i * 4);
  await p.mouse.up();
  for (let i = 0; i < 10; i++) { await p.mouse.wheel(0, -200); await p.waitForTimeout(50); }
  await p.waitForTimeout(1500);
  const frames = await p.evaluate(() => window.__frames.slice(1));
  frames.sort((a, b) => a - b);
  const pct = (q) => frames[Math.floor(frames.length * q)]?.toFixed(1);
  const st = await p.evaluate(() => ({ n: window.__dc.state.tree.n, items: window.__dc.state.vis.items.length }));
  const mem = await p.evaluate(() => (performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1e6) : null));
  console.log(JSON.stringify({ entries: st.n, nodes: st.items, loadMs: loaded, loadAndGrowMs: grown, frameMedianMs: pct(0.5), frameP95Ms: pct(0.95), heapMB: mem, errors: errs }));
  await b.close();
})();

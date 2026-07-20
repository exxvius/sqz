// Composes the per-feature "marketing" panels in docs/images/ from the REAL app
// components — not mockups. Each panel is built by:
//   1. opening a mocked scene (harness.tsx, Tauri IPC/events faked — see scenes.ts),
//   2. grabbing components as transparent-background PNG pieces (element
//      screenshots with the app background stripped),
//   3. arranging the pieces and screenshotting a TRANSPARENT stage.
//
// Every piece is re-clipped to its own rounded-rectangle shape when placed (see
// `place`): the raw element screenshot leaves faint artifacts in the bounding-box
// corners (outside the card's radius), and clipping to the exact rounded shape
// removes them cleanly. There are no drop-shadows — the panels are flat, arranged
// cards on transparency that sit on whatever background the README uses.
//
// Two panels are full-screen instead: the accent picker sliced into five colours,
// and the Live page split on a tilted seam into dark (left) and light (right).
//
// Usage: node screenshots/marketing.mjs   (dev server must be running on :1420)

import { chromium } from "playwright";
import { mkdir, readFile, writeFile } from "node:fs/promises";

const BASE = "http://localhost:1420/screenshots/harness.html";
const OUT = "docs/images";

const browser = await chromium.launch();
const ctx = await browser.newContext({
  viewport: { width: 1440, height: 1700 },
  deviceScaleFactor: 2,
});

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/** Open a mocked scene and run its event script; resolve to the ready page. */
async function open({ scene, theme = "dark", accent = "emerald", locked = false, nav, add, waitSel, expandBreakdown }) {
  const page = await ctx.newPage();
  await page.addInitScript((a) => localStorage.setItem("sqz-accent", a), accent);
  await page.goto(`${BASE}?scene=${scene}&theme=${theme}${locked ? "&locked=1" : ""}`, {
    waitUntil: "load",
    timeout: 20000,
  });
  await page.waitForFunction(() => window.sqz && window.sqz.ready);
  await page.evaluate(() => window.sqz.runScene());
  if (nav) await page.getByRole("button", { name: nav, exact: true }).click();
  if (add) await page.getByRole("button", { name: add === "folders" ? "Add folders" : "Add files", exact: true }).click();
  if (waitSel) await page.waitForSelector(waitSel, { timeout: 8000 });
  if (expandBreakdown) {
    await page.locator(".ab-toggle").click();
    await page.waitForSelector(".reclaim-table", { timeout: 8000 });
  }
  await page.waitForTimeout(600);
  return page;
}

/**
 * Grab one element as a transparent-background PNG piece + its size and its own
 * corner radius (so it can be re-clipped to shape when placed).
 *   opaque — fill a translucent card with the app's surface-over-bg so stacked
 *            cards don't show through each other.
 *   strip  — drop the card's own background/border/padding so only its inner
 *            content lands on the transparent canvas, no card frame.
 */
async function grab(page, selector, { index = 0, opaque = false, strip = false } = {}) {
  await page.evaluate(
    ({ selector, opaque, strip }) => {
      document.querySelectorAll(".lava").forEach((n) => (n.style.display = "none"));
      document.documentElement.style.background = "transparent";
      document.body.style.background = "transparent";
      const app = document.querySelector(".app");
      if (app) app.style.background = "transparent";
      document.querySelectorAll(selector).forEach((el) => {
        el.style.boxShadow = "none";
        if (strip) {
          el.style.background = "transparent";
          el.style.border = "none";
          el.style.backdropFilter = "none";
          el.style.padding = "0";
        } else if (opaque) {
          el.style.background = "linear-gradient(var(--surface), var(--surface)), var(--bg)";
          el.style.backdropFilter = "none";
          el.style.webkitBackdropFilter = "none";
        }
      });
    },
    { selector, opaque, strip },
  );
  const el = page.locator(selector).nth(index);
  await el.scrollIntoViewIfNeeded();
  await page.waitForTimeout(120);
  const box = await el.boundingBox();
  const r = await el.evaluate((n) => parseFloat(getComputedStyle(n).borderTopLeftRadius) || 0);
  const buf = await el.screenshot({ omitBackground: true });
  return { src: "data:image/png;base64," + buf.toString("base64"), w: Math.round(box.width), h: Math.round(box.height), r };
}

/** Full-window screenshot of a scene, as a data URL (for the full-bleed panels). */
async function fullShot(opts) {
  const page = await open(opts);
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForTimeout(300);
  const buf = await page.screenshot();
  await page.close();
  return "data:image/png;base64," + buf.toString("base64");
}

/**
 * Emit a piece clipped to its own rounded shape. The clip (overflow:hidden at the
 * element's own radius) removes the faint bounding-box-corner artifacts the raw
 * element screenshot leaves outside the card's radius — with no inset, so the
 * card's real borders stay intact and symmetric. `w`/`h` override the natural size
 * (radius scales with it) for the receding history deck; `clip:false` places a bare
 * image (for content that has no card frame to round, e.g. the disposal prompt).
 */
function place(p, { x, y, w = p.w, h = p.h, z, opacity = 1, clip = true } = {}) {
  const zi = z != null ? `z-index:${z};` : "";
  const op = opacity !== 1 ? `opacity:${opacity};` : "";
  if (!clip) {
    return `<img src="${p.src}" style="position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px;${zi}${op}display:block">`;
  }
  const rad = Math.min(p.r * (w / p.w), Math.min(w, h) / 2);
  return (
    `<div style="position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px;` +
    `border-radius:${rad}px;overflow:hidden;${zi}${op}">` +
    `<img src="${p.src}" style="width:${w}px;height:${h}px;display:block"></div>`
  );
}

/** Render an arranged stage to a transparent (or opaque, full-bleed) PNG buffer. */
async function stage({ w, h, inner, css = "", opaque = false }) {
  const page = await ctx.newPage();
  await page.setViewportSize({ width: Math.ceil(w), height: Math.ceil(h) });
  await page.setContent(
    `<!doctype html><meta charset="utf8"><style>*{margin:0;padding:0;box-sizing:border-box}` +
      `.stage{position:relative;overflow:hidden;background:transparent;width:${w}px;height:${h}px}${css}</style>` +
      `<div class="stage">${inner}</div>`,
  );
  await page.evaluate(async () => {
    await Promise.all([...document.images].map((i) => (i.decode ? i.decode().catch(() => {}) : null)));
  });
  await page.waitForTimeout(150);
  const buf = await page.locator(".stage").screenshot({ omitBackground: !opaque });
  await page.close();
  return buf;
}

async function save(file, buf) {
  await writeFile(`${OUT}/${file}`, buf);
  console.log(`✓ ${file}`);
}

// Drop each composed component panel into a uniform card (same size, same subtle
// backdrop, same padding) so the README's feature grid is even instead of ragged.
// GitHub strips CSS from README HTML, so the frame has to be baked into the image;
// object-fit:contain centres each panel at whatever scale fits. The full-screen
// panels (accents, theme) are already screenshots and stay as they are.
async function frameAll(names) {
  const W = 1540;
  const H = 770;
  const PAD = 48;
  const MARGIN = 16; // transparent gutter baked in, so cards don't touch in the grid
  for (const n of names) {
    const b64 = (await readFile(`${OUT}/feature-${n}.png`)).toString("base64");
    const page = await ctx.newPage();
    await page.setViewportSize({ width: W + 2 * MARGIN, height: H + 2 * MARGIN });
    await page.setContent(
      `<!doctype html><meta charset="utf8"><style>*{margin:0;box-sizing:border-box}
       .wrap{width:${W + 2 * MARGIN}px;height:${H + 2 * MARGIN}px;padding:${MARGIN}px;background:transparent}
       .card{width:${W}px;height:${H}px;padding:${PAD}px;display:flex;align-items:center;justify-content:center;
         border-radius:20px;border:1px solid rgba(255,255,255,.07);
         background:radial-gradient(90% 130% at 10% -25%, rgba(18,182,138,.12), transparent 55%), linear-gradient(158deg,#0f1c16 0%,#0a1310 100%)}
       .card img{max-width:100%;max-height:100%;object-fit:contain;display:block}</style>
       <div class="wrap"><div class="card"><img src="data:image/png;base64,${b64}"></div></div>`,
    );
    await page.evaluate(async () => {
      await Promise.all([...document.images].map((i) => i.decode().catch(() => {})));
    });
    await page.waitForTimeout(150);
    // omitBackground keeps the rounded-corner cut-outs and the gutter transparent.
    const buf = await page.locator(".wrap").screenshot({ omitBackground: true });
    await page.close();
    await writeFile(`${OUT}/feature-${n}.png`, buf);
    console.log(`✓ framed feature-${n}.png`);
  }
}

/** Vertically stack centred pieces (for the simpler single-column features). */
async function column(file, pieces, { gap = 22, m = 40 } = {}) {
  const W = Math.max(...pieces.map((p) => p.w));
  let y = m;
  let inner = "";
  for (const p of pieces) {
    inner += place(p, { x: m + (W - p.w) / 2, y });
    y += p.h + gap;
  }
  await save(file, await stage({ w: W + 2 * m, h: y - gap + m, inner }));
}

// ---------------------------------------------------------------------------
// 1. Watch every encode live — a fanned stack of three parallel encodes
// ---------------------------------------------------------------------------
async function live() {
  const page = await open({ scene: "dashboard", nav: "Live", waitSel: ".live-card" });
  const cards = [
    await grab(page, ".live-card", { index: 0, opaque: true }),
    await grab(page, ".live-card", { index: 1, opaque: true }),
    await grab(page, ".live-card", { index: 2, opaque: true }),
  ];
  await page.close();

  const W = cards[0].w;
  const H = cards[0].h;
  const DX = 46;
  const DY = 122;
  const M = 40;
  const inner = [
    place(cards[2], { x: M + 2 * DX, y: M + 2 * DY, z: 1 }),
    place(cards[1], { x: M + DX, y: M + DY, z: 2 }),
    place(cards[0], { x: M, y: M, z: 3 }),
  ].join("");
  await save("feature-live.png", await stage({ w: W + 2 * DX + 2 * M, h: H + 2 * DY + 2 * M, inner }));
}

// ---------------------------------------------------------------------------
// 2. Searchable history — a receding deck; top row full, the rest peeking under
// ---------------------------------------------------------------------------
async function history() {
  const page = await open({ scene: "history", nav: "History", waitSel: ".ecard" });
  const n = 6;
  const cards = [];
  for (let i = 0; i < n; i++) cards.push(await grab(page, ".ecard", { index: i, opaque: true }));
  await page.close();

  const W = cards[0].w;
  const PEEK = 22;
  const M = 40;
  let maxBottom = 0;
  const inner = cards
    .map((c, i) => {
      const s = 1 - i * 0.04;
      const w = W * s;
      const h = c.h * s;
      const x = M + (W - w) / 2;
      const y = M + i * PEEK;
      maxBottom = Math.max(maxBottom, y + h);
      return place(c, { x, y, w, h, z: n - i });
    })
    .join("");
  await save("feature-history.png", await stage({ w: W + 2 * M, h: maxBottom + M, inner }));
}

// ---------------------------------------------------------------------------
// 3. Lock it and walk away — a bento of the feature's controls
//    Row 1 (full width): the lock toggle, centred.
//    Row 2: the unlock prompt (left) · the three locked/masked cards (right).
// ---------------------------------------------------------------------------
async function locked() {
  const dash = await open({ scene: "dashboard", nav: "Live", locked: true, waitSel: ".live-card" });
  const maskedLive = await grab(dash, ".live-card", { index: 0, opaque: true });
  await dash.close();

  const hist = await open({ scene: "history", nav: "History", locked: true, waitSel: ".ecard" });
  await hist.locator(".ecard-head").first().click();
  await hist.waitForSelector(".ecard.open", { timeout: 8000 });
  await hist.waitForTimeout(600); // let the expand animation settle to full height
  const histOpen = await grab(hist, ".ecard.open", { index: 0, opaque: true });
  const histClosed = await grab(hist, ".ecard:not(.open)", { index: 0, opaque: true });
  await hist.close();

  const lk = await open({ scene: "home", locked: true, waitSel: ".foot-btn.on" });
  const toggleOn = await grab(lk, ".foot-btn.on", { index: 0 });
  await lk.locator(".foot-btn.on").first().click();
  await lk.waitForSelector(".pw-modal", { timeout: 8000 });
  const modal = await grab(lk, ".pw-modal", { opaque: true });
  await lk.close();

  const M = 40;
  const GAP = 30;
  const COLGAP = 34;
  const rightCards = [maskedLive, histOpen, histClosed]; // natural aspect ratios
  const rightW = Math.max(...rightCards.map((p) => p.w));
  const contentW = modal.w + COLGAP + rightW;

  // Row 1: lock toggle, centred across the full content width.
  let inner = place(toggleOn, { x: M + (contentW - toggleOn.w) / 2, y: M });
  const row2 = M + toggleOn.h + GAP;

  // Row 2 left: the unlock prompt; right: the three locked cards at true size.
  inner += place(modal, { x: M, y: row2 });
  let ry = row2;
  const rx = M + modal.w + COLGAP;
  for (const c of rightCards) {
    inner += place(c, { x: rx, y: ry });
    ry += c.h + GAP;
  }
  const stageH = Math.max(row2 + modal.h, ry - GAP) + M;
  await save("feature-locked.png", await stage({ w: M + contentW + M, h: stageH, inner }));
}

// ---------------------------------------------------------------------------
// 4. Recolor the whole app — the Live page sliced into ten accents on tilted seams
// ---------------------------------------------------------------------------
async function accents() {
  const picks = ["emerald", "lime", "teal", "cyan", "blue", "indigo", "violet", "fuchsia", "rose", "orange"];
  const shots = [];
  for (const accent of picks) shots.push(await fullShot({ scene: "dashboard", accent, nav: "Live", waitSel: ".live-card" }));

  const W = 1440;
  const H = 900;
  const N = picks.length;
  const sliceW = W / N;
  const dx = Math.tan((17 * Math.PI) / 180) * (H / 2); // ~17° seam, matching the theme split
  const EXT = 400; // let the outer bands overrun the edges so the corners fill
  const OV = 3; // each band overruns its right edge into the next so the later-drawn
  // band backs onto real content, not a transparent gap — no seam line
  const inner = shots
    .map((src, i) => {
      const lt = i === 0 ? -EXT : i * sliceW + dx;
      const lb = i === 0 ? -EXT : i * sliceW - dx;
      const rt = i === N - 1 ? W + EXT : (i + 1) * sliceW + dx + OV;
      const rb = i === N - 1 ? W + EXT : (i + 1) * sliceW - dx + OV;
      const clip = `polygon(${lt}px 0, ${rt}px 0, ${rb}px ${H}px, ${lb}px ${H}px)`;
      return `<img src="${src}" style="position:absolute;inset:0;width:${W}px;height:${H}px;clip-path:${clip}">`;
    })
    .join("");
  await save("feature-accents.png", await stage({ w: W, h: H, inner, opaque: true }));
}

// ---------------------------------------------------------------------------
// 5. Light & dark — the Home screen split on a tilted seam (dark left, light right)
// ---------------------------------------------------------------------------
async function themeSplit() {
  const dark = await fullShot({ scene: "home", theme: "dark", add: "folders", waitSel: ".ab-readout" });
  const light = await fullShot({ scene: "home", theme: "light", add: "folders", waitSel: ".ab-readout" });

  const W = 1440;
  const H = 900;
  const dx = Math.round(Math.tan((17 * Math.PI) / 180) * (H / 2)); // ~17° tilt from vertical
  const clip = `polygon(${W / 2 + dx}px 0, ${W}px 0, ${W}px ${H}px, ${W / 2 - dx}px ${H}px)`;
  const inner = `
    <img src="${dark}" style="position:absolute;inset:0;width:${W}px;height:${H}px">
    <img src="${light}" style="position:absolute;inset:0;width:${W}px;height:${H}px;clip-path:${clip}">`;
  await save("feature-theme.png", await stage({ w: W, h: H, inner, opaque: true }));
}

// ---------------------------------------------------------------------------
// 6. Estimate before you run — the floating action bar over its docked breakdown
// ---------------------------------------------------------------------------
async function projection() {
  const page = await open({ scene: "home", add: "folders", waitSel: ".ab-toggle", expandBreakdown: true });
  const breakdown = await grab(page, ".actionbar-details", { opaque: true });
  const bar = await grab(page, ".actionbar", { opaque: true });
  await page.close();
  await column("feature-projection.png", [breakdown, bar], { gap: 26 });
}

// ---------------------------------------------------------------------------
// 7. Uses your GPU — the three codec cells (in a row) + the acceleration line
// ---------------------------------------------------------------------------
async function encoders() {
  const page = await open({ scene: "home", waitSel: ".codec-matrix" });
  const cells = [
    await grab(page, ".matrix-cell", { index: 0, opaque: true }),
    await grab(page, ".matrix-cell", { index: 1, opaque: true }),
    await grab(page, ".matrix-cell", { index: 2, opaque: true }),
  ];
  const note = await grab(page, ".hw-note.ok", { opaque: true });
  await page.close();

  const GAP = 16;
  const M = 40;
  const rowW = cells.reduce((a, c) => a + c.w, 0) + GAP * (cells.length - 1);
  const cellH = Math.max(...cells.map((c) => c.h));
  const W = Math.max(rowW, note.w);
  let inner = "";
  let x = M + (W - rowW) / 2;
  for (const c of cells) {
    inner += place(c, { x, y: M });
    x += c.w + GAP;
  }
  inner += place(note, { x: M + (W - note.w) / 2, y: M + cellH + GAP });
  await save("feature-encoders.png", await stage({ w: W + 2 * M, h: M + cellH + GAP + note.h + M, inner }));
}

// ---------------------------------------------------------------------------
// 8. Codec + quality — the codec toggle + the four preset cards (in a row)
// ---------------------------------------------------------------------------
async function presets() {
  const page = await open({ scene: "home", waitSel: ".presets" });
  const seg = await grab(page, '.seg[aria-label="Target codec"]', { opaque: true });
  const cards = [];
  for (let i = 0; i < 4; i++) cards.push(await grab(page, ".preset", { index: i, opaque: true }));
  await page.close();

  const GAP = 18;
  const M = 40;
  const rowW = cards.reduce((a, c) => a + c.w, 0) + GAP * (cards.length - 1);
  const cardH = Math.max(...cards.map((c) => c.h));
  const W = Math.max(rowW, seg.w);
  let inner = place(seg, { x: M + (W - seg.w) / 2, y: M });
  const py = M + seg.h + 24;
  let x = M + (W - rowW) / 2;
  for (const c of cards) {
    inner += place(c, { x, y: py });
    x += c.w + GAP;
  }
  await save("feature-presets.png", await stage({ w: W + 2 * M, h: py + cardH + M, inner }));
}

// ---------------------------------------------------------------------------
// 9. Safe disposal — the "where the original goes" card
// ---------------------------------------------------------------------------
async function disposal() {
  const page = await open({ scene: "home", waitSel: '.seg[aria-label="Disposal of originals"]' });
  // Resolve the card via a Playwright locator (:has() in the page's own
  // querySelectorAll can silently miss); make it opaque like the other cards.
  const card = page.locator(".card", { has: page.locator('.seg[aria-label="Disposal of originals"]') });
  await page.evaluate(() => {
    document.querySelectorAll(".lava").forEach((n) => (n.style.display = "none"));
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    const app = document.querySelector(".app");
    if (app) app.style.background = "transparent";
  });
  await card.evaluate((el) => {
    el.style.boxShadow = "none";
    el.style.background = "linear-gradient(var(--surface), var(--surface)), var(--bg)";
    el.style.backdropFilter = "none";
  });
  await card.scrollIntoViewIfNeeded();
  await page.waitForTimeout(150);
  const box = await card.boundingBox();
  const r = await card.evaluate((n) => parseFloat(getComputedStyle(n).borderTopLeftRadius) || 0);
  const buf = await card.screenshot({ omitBackground: true });
  await page.close();

  const piece = { src: "data:image/png;base64," + buf.toString("base64"), w: Math.round(box.width), h: Math.round(box.height), r };
  await column("feature-disposal.png", [piece], { m: 40 });
}

// ---------------------------------------------------------------------------

await mkdir(OUT, { recursive: true });
await live();
await history();
await locked();
await accents();
await themeSplit();
await projection();
await encoders();
await presets();
await disposal();

// Wrap the component panels in uniform cards so the README grid is even. The
// full-screen accents/theme panels are left as-is.
await frameAll(["projection", "live", "presets", "encoders", "history", "locked", "disposal"]);

await browser.close();
console.log("done");

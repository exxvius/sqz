// Drives the mocked harness in headless Chromium and writes PNGs to docs/images.
// Usage: node screenshots/capture.mjs   (dev server must be running on :1420)

import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";

const BASE = "http://localhost:1420/screenshots/harness.html";
const OUT = "docs/images";

const SHOTS = [
  { scene: "dashboard", theme: "dark", file: "dashboard-dark.png", nav: "Live", wait: ".live-card" },
  { scene: "history", theme: "dark", file: "history-dark.png", nav: "History", wait: ".kv-grid" },
  { scene: "home", theme: "dark", file: "home-dark.png", nav: null, addFiles: true, wait: ".queue-row" },
  { scene: "dashboard", theme: "light", file: "dashboard-light.png", nav: "Live", wait: ".live-card" },
];

await mkdir(OUT, { recursive: true });

const browser = await chromium.launch();
const ctx = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 2,
});

for (const s of SHOTS) {
  const page = await ctx.newPage();
  const url = `${BASE}?scene=${s.scene}&theme=${s.theme}`;
  await page.goto(url, { waitUntil: "networkidle" });
  await page.waitForFunction(() => window.sqz && window.sqz.ready);
  await page.evaluate(() => window.sqz.runScene());

  if (s.nav) await page.getByRole("button", { name: s.nav, exact: true }).click();
  if (s.addFiles) await page.getByRole("button", { name: "Add files", exact: true }).click();

  await page.waitForSelector(s.wait, { timeout: 8000 });
  await page.waitForTimeout(600); // let bars/animations settle
  await page.screenshot({ path: `${OUT}/${s.file}` });
  console.log(`✓ ${s.file}`);
  await page.close();
}

await browser.close();
console.log("done");

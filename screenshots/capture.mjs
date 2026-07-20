// Captures the full-window hero shots (the live dashboard, light + dark) that head
// the README. The per-feature marketing panels are composed separately — see
// marketing.mjs. Both need the dev server running on :1420.
// Usage: node screenshots/capture.mjs

import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";

const BASE = "http://localhost:1420/screenshots/harness.html";
const OUT = "docs/images";

const SHOTS = [
  { theme: "dark", file: "dashboard-dark.png" },
  { theme: "light", file: "dashboard-light.png" },
];

await mkdir(OUT, { recursive: true });

const browser = await chromium.launch();
const ctx = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  deviceScaleFactor: 2,
});

for (const s of SHOTS) {
  const page = await ctx.newPage();
  await page.addInitScript(() => localStorage.setItem("sqz-accent", "emerald"));
  await page.goto(`${BASE}?scene=dashboard&theme=${s.theme}`, { waitUntil: "load", timeout: 20000 });
  await page.waitForFunction(() => window.sqz && window.sqz.ready);
  await page.evaluate(() => window.sqz.runScene());
  await page.getByRole("button", { name: "Live", exact: true }).click();
  await page.waitForSelector(".live-card", { timeout: 8000 });
  await page.waitForTimeout(700);
  await page.screenshot({ path: `${OUT}/${s.file}` });
  console.log(`✓ ${s.file}`);
  await page.close();
}

await browser.close();
console.log("done");

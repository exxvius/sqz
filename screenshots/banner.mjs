// Renders screenshots/banner.html to a PNG at 2x for the README header.
import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";

await mkdir("docs/images", { recursive: true });
const browser = await chromium.launch();
const page = await browser.newPage({ deviceScaleFactor: 2 });
await page.goto("http://localhost:1420/screenshots/banner.html", { waitUntil: "networkidle" });
await page.evaluate(() => document.fonts.ready);
await page.waitForTimeout(300);
const el = await page.$(".banner");
await el.screenshot({ path: "docs/images/banner.png" });
console.log("✓ banner.png");
await browser.close();

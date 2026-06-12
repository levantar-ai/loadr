// Record a real browsing session of the loadr web UI during a live run.
const { chromium } = require("playwright");

const BASE = "http://127.0.0.1:6470";

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    recordVideo: { dir: "video-out", size: { width: 1600, height: 900 } },
  });
  const page = await context.newPage();

  // Overview: let the live charts accumulate points.
  await page.goto(BASE + "/", { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(9000);

  // Smooth scroll through the overview.
  await page.mouse.wheel(0, 350);
  await page.waitForTimeout(2500);
  await page.mouse.wheel(0, -350);
  await page.waitForTimeout(1500);

  // Runs page → the live run detail.
  await page.click('a[data-route="runs"]');
  await page.waitForTimeout(2500);
  const row = page.locator("table tbody tr").first();
  if (await row.count()) {
    await row.click().catch(() => {});
    await page.waitForTimeout(7000);
    await page.mouse.wheel(0, 320);
    await page.waitForTimeout(2500);
    await page.mouse.wheel(0, -320);
  }

  // Tests page: the editor.
  await page.click('a[data-route="tests"]');
  await page.waitForTimeout(2800);
  const validate = page.locator("button", { hasText: /validate/i }).first();
  if (await validate.count()) {
    await validate.click().catch(() => {});
    await page.waitForTimeout(2500);
  }

  // Agents page (empty-state in standalone), then back to overview.
  await page.click('a[data-route="agents"]');
  await page.waitForTimeout(2000);
  await page.click('a[data-route="overview"]');
  await page.waitForTimeout(7000);

  await context.close(); // flushes the video
  await browser.close();
  console.log("recorded");
})();

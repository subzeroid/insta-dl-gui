import { chromium } from "playwright";
import { rename, rm } from "node:fs/promises";
import { basename, dirname, extname, join } from "node:path";

const url = process.argv[2] ?? "http://localhost:1420/explore?mock=1&demo=explore";
const out = process.argv[3] ?? "docs/screenshot.png";
const width = Number(process.argv[4] ?? 1200);
const height = Number(process.argv[5] ?? 800);
const readySelector = process.argv[6] ?? "[data-remote-image][data-state='loaded']";
const extension = extname(out) || ".png";
const temporaryOut = join(
  dirname(out),
  `.${basename(out)}.${process.pid}.${Date.now()}.tmp${extension}`,
);

const browser = await chromium.launch();
try {
  const pageErrors = [];
  const page = await browser.newPage({
    viewport: { width, height },
    deviceScaleFactor: 2,
  });

  page.on("pageerror", (err) => {
    pageErrors.push(err.message);
    console.error("[pageerror]", err.message);
  });
  page.on("console", (msg) => {
    if (msg.type() === "error" || msg.type() === "warning") {
      console.error(`[console.${msg.type()}]`, msg.text());
    }
  });

  await page.goto(url, { waitUntil: "networkidle" });
  await page.locator(readySelector).first().waitFor({ state: "visible" });
  await page.waitForFunction(() =>
    !document.querySelector(
      "[data-remote-image][data-state='loading'], [data-remote-video][data-state='loading']",
    ),
  );
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(200);
  await page.screenshot({ path: temporaryOut });
  if (pageErrors.length > 0) {
    throw new Error(`Screenshot aborted after ${pageErrors.length} page error(s)`);
  }
  await rename(temporaryOut, out);
  console.log("saved", out);
} finally {
  try {
    await browser.close();
  } finally {
    await rm(temporaryOut, { force: true });
  }
}

import { chromium } from "playwright";

const url = process.argv[2] ?? "http://localhost:1420/download?mock=1&demo=profile";
const out = process.argv[3] ?? "docs/screenshot.png";
const width = Number(process.argv[4] ?? 1000);
const height = Number(process.argv[5] ?? 700);

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width, height },
  deviceScaleFactor: 2,
});

page.on("pageerror", (err) => console.error("[pageerror]", err.message));
page.on("console", (msg) => {
  if (msg.type() === "error" || msg.type() === "warning") {
    console.error(`[console.${msg.type()}]`, msg.text());
  }
});

await page.goto(url, { waitUntil: "networkidle" });
await page.waitForTimeout(600);
await page.screenshot({ path: out });
console.log("saved", out);
await browser.close();

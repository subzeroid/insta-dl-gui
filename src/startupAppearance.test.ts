import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

describe("startup appearance", () => {
  it("configures the first Tauri window with the app background", () => {
    const config = JSON.parse(
      readFileSync(join(process.cwd(), "src-tauri/tauri.conf.json"), "utf8"),
    ) as { app?: { windows?: Array<{ backgroundColor?: string }> } };

    expect(config.app?.windows?.[0]?.backgroundColor).toBe("#0b0e14");
  });

  it("sets the earliest document background before the app module", () => {
    const html = readFileSync(join(process.cwd(), "index.html"), "utf8");
    const moduleIndex = html.indexOf('<script type="module" src="/src/main.ts"></script>');
    const beforeModule = html.slice(0, moduleIndex);
    const styleBlocks = [
      ...beforeModule.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style\s*>/gi),
    ].map((match) => match[1]);
    const css = styleBlocks.join("\n").replace(/\/\*[\s\S]*?\*\//g, "");

    expect(moduleIndex).toBeGreaterThan(0);
    expect(styleBlocks.length).toBeGreaterThan(0);
    expect(css).toMatch(
      /:root\s*\{[^}]*?(?<![-\w])color-scheme(?![-\w])\s*:\s*dark(?=\s*(?:;|}))[^}]*\}/s,
    );
    expect(css).toMatch(
      /html\s*,\s*body\s*,\s*#app\s*\{[^}]*?(?<![-\w])background-color(?![-\w])\s*:\s*#0b0e14(?=\s*(?:;|}))[^}]*\}/s,
    );
  });
});

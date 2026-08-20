import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

async function readShowcaseFile(path: string): Promise<string> {
  return readFile(resolve(root, path), "utf8");
}

describe("Astro SSR foundation", () => {
  it("pins the standalone Node SSR runtime", async () => {
    const [packageJson, astroConfig] = await Promise.all([
      readShowcaseFile("package.json"),
      readShowcaseFile("astro.config.mjs"),
    ]);

    expect(packageJson).toContain('"packageManager": "pnpm@10.12.4"');
    expect(packageJson).toContain('"node": ">=22.12.0"');
    expect(astroConfig).toContain('output: "server"');
    expect(astroConfig).toContain('node({ mode: "standalone" })');
  });

  it("renders a semantic filesystem showcase placeholder", async () => {
    const [layout, page] = await Promise.all([
      readShowcaseFile("src/layouts/Layout.astro"),
      readShowcaseFile("src/pages/index.astro"),
    ]);

    expect(layout).toContain('name="viewport"');
    expect(layout).toContain("global.css");
    expect(page).toContain("<main>");
    expect(page).toContain('<section aria-label="Filesystem showcase">');
  });
});

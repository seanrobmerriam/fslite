import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import astroConfig from "../../astro.config.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const repositoryRoot = resolve(root, "..");

async function readShowcaseFile(path: string): Promise<string> {
  return readFile(resolve(root, path), "utf8");
}

describe("Astro SSR foundation", () => {
  it("pins the standalone Node SSR runtime", async () => {
    const packageJson = JSON.parse(await readShowcaseFile("package.json")) as {
      packageManager: string;
      engines: { node: string };
    };

    expect(packageJson.packageManager).toBe("pnpm@10.12.4");
    expect(packageJson.engines.node).toBe(">=22.12.0");
    expect(astroConfig.output).toBe("server");
    expect(astroConfig.adapter?.name).toBe("@astrojs/node");
    expect(astroConfig.integrations).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: "@astrojs/react" }),
      ]),
    );
  });

  it("documents only Astro's private upstream server URL", async () => {
    const example = await readShowcaseFile(".env.example");

    expect(example).toContain("FSLITE_SERVER_URL=http://fslite-server:8080");
    expect(example).toContain("browser talks only to Astro");
    expect(example).not.toMatch(/^PUBLIC_.*FSLITE/m);
    expect(example).not.toMatch(/^FSLITE_TOKEN/m);
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

  it("pins the public workspace quotas in the reference deployment", async () => {
    const compose = await readFile(
      resolve(repositoryRoot, "deploy/showcase/compose.yml"),
      "utf8",
    );
    const serverService = compose
      .split("\n  fslite-server:\n", 2)
      .at(1)
      ?.split("\nsecrets:\n", 1)
      .at(0);

    expect(serverService).toContain('FSLITE_MAX_BYTES: "10485760"');
    expect(serverService).toContain('FSLITE_MAX_NODES: "250"');
    expect(serverService).toContain('FSLITE_MAX_FILE_BYTES: "1048576"');
  });
});

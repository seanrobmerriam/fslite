import { describe, expect, it, vi } from "vitest";

import { SEED_ENTRIES, seedWorkspace } from "./seed";

describe("SEED_ENTRIES", () => {
  it("defines a stable, secret-free tree with every directory before every file", () => {
    expect(SEED_ENTRIES.map((entry) => entry.path)).toEqual([
      "/docs",
      "/examples",
      "/README.md",
      "/docs/http-api.md",
      "/examples/hello.txt",
      "/examples/metadata.json",
    ]);
    expect(SEED_ENTRIES.map((entry) => entry.kind)).toEqual([
      "directory",
      "directory",
      "file",
      "file",
      "file",
      "file",
    ]);

    const text = SEED_ENTRIES.flatMap((entry) =>
      entry.kind === "file" ? [entry.text] : [],
    ).join("\n");
    expect(text).toMatch(/tree/i);
    expect(text).toMatch(/content/i);
    expect(text).toMatch(/trash/i);
    expect(text).toMatch(/search/i);
    expect(text).toMatch(/reset/i);
    expect(text).not.toMatch(/FSLITE_|Bearer\s+|token\b|process\.env/i);
  });

  it("creates entries serially in manifest order", async () => {
    const calls: string[] = [];
    const client = {
      mkdir: vi.fn(async (path: string) => {
        calls.push(`mkdir:${path}`);
      }),
      writeFile: vi.fn(async (path: string, bytes: Uint8Array) => {
        calls.push(`write:${path}:${new TextDecoder().decode(bytes)}`);
      }),
    };

    await seedWorkspace(client);

    expect(calls.map((call) => call.split(":", 2).join(":"))).toEqual([
      "mkdir:/docs",
      "mkdir:/examples",
      "write:/README.md",
      "write:/docs/http-api.md",
      "write:/examples/hello.txt",
      "write:/examples/metadata.json",
    ]);
    expect(client.mkdir).toHaveBeenNthCalledWith(1, "/docs", true);
    expect(client.mkdir).toHaveBeenNthCalledWith(2, "/examples", true);
  });
});

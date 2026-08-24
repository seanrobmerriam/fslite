import { describe, expect, it, vi } from "vitest";

import { ShowcaseApi } from "../lib/browser/api";

const runtime = {
  status: vi.fn(),
};

vi.mock("../lib/server/runtime", () => ({
  getShowcaseRuntime: vi.fn(async () => runtime),
}));
vi.mock("../lib/server/config", () => ({
  loadServerConfig: vi.fn(() => ({ trustProxy: false })),
}));

const usage = {
  workspace_id: "runtime-only-workspace",
  active_logical_bytes: 1,
  trashed_logical_bytes: 0,
  staged_bytes: 0,
  active_nodes: 1,
  trashed_nodes: 0,
  max_logical_bytes: 10,
  max_nodes: 250,
  max_file_bytes: 1024,
};
const publicUsage = {
  active_logical_bytes: usage.active_logical_bytes,
  trashed_logical_bytes: usage.trashed_logical_bytes,
  staged_bytes: usage.staged_bytes,
  active_nodes: usage.active_nodes,
  trashed_nodes: usage.trashed_nodes,
  max_logical_bytes: usage.max_logical_bytes,
  max_nodes: usage.max_nodes,
  max_file_bytes: usage.max_file_bytes,
};

function context(request: Request) {
  return { request, url: new URL(request.url), clientAddress: "198.51.100.7" };
}

describe("public status wire contract", () => {
  it("strips runtime internals and accepts a nullable reset timestamp end to end", async () => {
    runtime.status.mockResolvedValue({
      ready: true,
      workspaceId: "runtime-only-workspace",
      activeOperations: 7,
      generation: 3,
      resetting: false,
      nextResetAt: null,
      usage,
    });
    const { GET } = await import("../pages/api/status");
    const fetch = vi.fn<typeof globalThis.fetch>(async (input) =>
      GET(
        context(
          new Request(
            typeof input === "string"
              ? `http://showcase.test${input}`
              : input.toString(),
          ),
        ) as never,
      ),
    );
    const api = new ShowcaseApi({ fetch });

    const routeResponse = await GET(
      context(new Request("http://showcase.test/api/status")) as never,
    );
    const wire = (await routeResponse.json()) as Record<string, unknown>;

    expect(Object.keys(wire).sort()).toEqual([
      "generation",
      "nextResetAt",
      "now",
      "ready",
      "resetting",
      "usage",
    ]);
    expect(wire).toMatchObject({
      ready: true,
      generation: 3,
      resetting: false,
      nextResetAt: null,
      usage: publicUsage,
      now: expect.any(Number),
    });
    expect(wire).not.toHaveProperty("workspaceId");
    expect(wire).not.toHaveProperty("activeOperations");
    expect(JSON.stringify(wire)).not.toContain("runtime-only-workspace");
    expect(wire.usage as Record<string, unknown>).not.toHaveProperty(
      "workspace_id",
    );

    await expect(api.status()).resolves.toMatchObject({
      generation: 3,
      nextResetAt: null,
      usage: publicUsage,
    });
  });
});

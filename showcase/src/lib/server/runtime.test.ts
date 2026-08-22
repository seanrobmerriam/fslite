import { afterEach, describe, expect, it, vi } from "vitest";

import {
  getShowcaseRuntime,
  SHOWCASE_RUNTIME_SYMBOL,
  type RuntimeDependencies,
} from "./runtime";
import type { ActivityRecord } from "../shared/contracts";
import { validateVirtualPath } from "../shared/path";

function clearRuntimeSingleton(): void {
  delete (globalThis as Record<PropertyKey, unknown>)[SHOWCASE_RUNTIME_SYMBOL];
}

function runtimeDependencies(
  overrides: Partial<RuntimeDependencies> = {},
): RuntimeDependencies {
  const client = {
    identity: vi.fn(async () => ({
      data: {
        workspace_id: "workspace-1",
        capabilities: ["workspace_admin"],
      },
    })),
    resetWorkspace: vi.fn(async () => undefined),
    mkdir: vi.fn(async () => undefined),
    writeFile: vi.fn(async () => undefined),
    readFile: vi.fn(async () => ({ data: new Uint8Array([7]) })),
    usage: vi.fn(async () => ({ data: { active_nodes: 2 } })),
  };
  const gateway = {
    execute: vi.fn(async () => ({ data: { kind: "execute" } })),
    upload: vi.fn(async () => ({ data: { kind: "upload" } })),
  };
  const coordinator = {
    start: vi.fn(async () => undefined),
    snapshot: vi.fn(() => ({
      activeOperations: 0,
      resetting: false,
      generation: 3,
      nextResetAt: 900_000,
    })),
    withOperation: vi.fn(async <T>(operation: () => Promise<T>) => operation()),
  };

  return {
    loadConfig: vi.fn(() => ({
      serverUrl: new URL("http://upstream.example.test"),
      token: "test-token",
      resetIntervalMs: 900_000,
      requestTimeoutMs: 1_000,
      trustProxy: false,
    })),
    createClient: vi.fn(() => client),
    createGateway: vi.fn(() => gateway),
    createCoordinator: vi.fn(() => coordinator),
    ...overrides,
  } as RuntimeDependencies;
}

afterEach(() => {
  clearRuntimeSingleton();
});

describe("getShowcaseRuntime", () => {
  it("memoizes one initialized process runtime and starts its lifecycle once", async () => {
    const dependencies = runtimeDependencies();

    const first = getShowcaseRuntime(dependencies);
    const second = getShowcaseRuntime(dependencies);
    const runtime = await first;

    expect(second).toBe(first);
    expect(dependencies.loadConfig).toHaveBeenCalledTimes(1);
    expect(dependencies.createClient).toHaveBeenCalledTimes(1);
    expect(dependencies.createGateway).toHaveBeenCalledTimes(1);
    expect(dependencies.createCoordinator).toHaveBeenCalledTimes(1);
    expect(runtime.liveness()).toEqual({ ok: true });
    expect(await runtime.readiness()).toEqual({
      ready: true,
      workspaceId: "workspace-1",
    });
  });

  it("clears a failed initialization promise so a later request retries", async () => {
    const failed = runtimeDependencies();
    const failedClient = failed.createClient!(failed.loadConfig!());
    failedClient.identity = vi.fn(async () => {
      throw new Error("identity unavailable");
    });
    failed.createClient = vi.fn(() => failedClient);

    await expect(getShowcaseRuntime(failed)).rejects.toThrow(
      "identity unavailable",
    );
    const retried = runtimeDependencies();
    await expect(getShowcaseRuntime(retried)).resolves.toMatchObject({
      workspaceId: "workspace-1",
    });
    expect(retried.loadConfig).toHaveBeenCalledTimes(1);
  });

  it("requires the authenticated workspace-admin capability before starting", async () => {
    const dependencies = runtimeDependencies();
    const client = dependencies.createClient!(dependencies.loadConfig!());
    client.identity = vi.fn(async () => ({
      data: { workspace_id: "workspace-1", capabilities: ["read"] },
      activity: {} as ActivityRecord,
    }));
    dependencies.createClient = vi.fn(() => client);

    await expect(getShowcaseRuntime(dependencies)).rejects.toThrow(
      "workspace_admin",
    );
  });

  it("routes public work through the reset gate and keeps reset private", async () => {
    const dependencies = runtimeDependencies();
    const runtime = await getShowcaseRuntime(dependencies);
    const client = (dependencies.createClient as ReturnType<typeof vi.fn>).mock
      .results[0].value as {
      readFile: ReturnType<typeof vi.fn>;
      usage: ReturnType<typeof vi.fn>;
    };
    const gateway = (dependencies.createGateway as ReturnType<typeof vi.fn>)
      .mock.results[0].value as {
      execute: ReturnType<typeof vi.fn>;
      upload: ReturnType<typeof vi.fn>;
    };
    const coordinator = (
      dependencies.createCoordinator as ReturnType<typeof vi.fn>
    ).mock.results[0].value as { withOperation: ReturnType<typeof vi.fn> };

    await expect(
      runtime.execute({ kind: "usage" }, "203.0.113.1"),
    ).resolves.toEqual({
      data: { kind: "execute" },
    });
    await expect(
      runtime.upload("/new.txt", new Uint8Array([1]), "203.0.113.1"),
    ).resolves.toEqual({ data: { kind: "upload" } });
    await expect(
      runtime.download(validateVirtualPath("/download.txt")),
    ).resolves.toEqual({
      data: new Uint8Array([7]),
    });
    await expect(runtime.status()).resolves.toEqual({
      ready: true,
      workspaceId: "workspace-1",
      activeOperations: 0,
      resetting: false,
      generation: 3,
      nextResetAt: 900_000,
      usage: { active_nodes: 2 },
    });
    expect(gateway.execute).toHaveBeenCalledWith(
      { kind: "usage" },
      "203.0.113.1",
    );
    expect(gateway.upload).toHaveBeenCalledWith(
      "/new.txt",
      new Uint8Array([1]),
      "203.0.113.1",
    );
    expect(client.readFile).toHaveBeenCalledWith(
      validateVirtualPath("/download.txt"),
    );
    expect(client.usage).toHaveBeenCalledTimes(1);
    expect(coordinator.withOperation).toHaveBeenCalledTimes(4);
    expect("resetNow" in runtime).toBe(false);
  });
});

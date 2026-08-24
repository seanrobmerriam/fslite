import { describe, expect, it, vi } from "vitest";

import { ResetCoordinator, WorkspaceResettingError } from "./reset-coordinator";
import { seedWorkspace } from "./seed";

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("ResetCoordinator", () => {
  it("drains admitted work, rejects later work, and publishes a completed deterministic reset", async () => {
    const entered = deferred<void>();
    const releaseOperation = deferred<void>();
    const calls: string[] = [];
    const client = {
      resetWorkspace: vi.fn(async () => {
        calls.push("reset");
      }),
      mkdir: vi.fn(async (path: string) => {
        calls.push(`mkdir:${path}`);
      }),
      writeFile: vi.fn(async (path: string) => {
        calls.push(`write:${path}`);
      }),
    };
    const coordinator = new ResetCoordinator(
      client,
      () => seedWorkspace(client),
      { now: () => 10_000 },
    );
    const operation = coordinator.withOperation(async () => {
      entered.resolve();
      await releaseOperation.promise;
    });
    await entered.promise;

    const reset = coordinator.resetNow();

    expect(coordinator.snapshot()).toEqual({
      activeOperations: 1,
      resetting: true,
      generation: 0,
      nextResetAt: null,
    });
    await expect(
      coordinator.withOperation(async () => undefined),
    ).rejects.toBeInstanceOf(WorkspaceResettingError);
    expect(client.resetWorkspace).not.toHaveBeenCalled();

    releaseOperation.resolve();
    await operation;
    await reset;

    expect(calls).toEqual([
      "reset",
      "mkdir:/docs",
      "mkdir:/examples",
      "write:/README.md",
      "write:/docs/http-api.md",
      "write:/examples/hello.txt",
      "write:/examples/metadata.json",
    ]);
    expect(coordinator.snapshot()).toEqual({
      activeOperations: 0,
      resetting: false,
      generation: 1,
      nextResetAt: 910_000,
    });
  });

  it("coalesces concurrent reset callers into one upstream reset", async () => {
    const releaseReset = deferred<void>();
    const client = {
      resetWorkspace: vi.fn(() => releaseReset.promise),
    };
    const coordinator = new ResetCoordinator(client, async () => undefined);

    const first = coordinator.resetNow();
    const second = coordinator.resetNow();

    expect(second).toBe(first);
    await Promise.resolve();
    expect(client.resetWorkspace).toHaveBeenCalledTimes(1);
    releaseReset.resolve();
    await expect(Promise.all([first, second])).resolves.toEqual([
      undefined,
      undefined,
    ]);
    expect(coordinator.snapshot().generation).toBe(1);
  });

  it("always releases active-operation accounting when work rejects", async () => {
    const client = { resetWorkspace: vi.fn(async () => undefined) };
    const coordinator = new ResetCoordinator(client, async () => undefined);

    await expect(
      coordinator.withOperation(async () => {
        throw new Error("operation failed");
      }),
    ).rejects.toThrow("operation failed");
    expect(coordinator.snapshot().activeOperations).toBe(0);

    await coordinator.resetNow();
    expect(client.resetWorkspace).toHaveBeenCalledTimes(1);
  });

  it("leaves a failed reset retryable without publishing a partial generation", async () => {
    const client = {
      resetWorkspace: vi
        .fn<() => Promise<void>>()
        .mockRejectedValueOnce(new Error("upstream reset failed"))
        .mockResolvedValueOnce(undefined),
    };
    const coordinator = new ResetCoordinator(client, async () => undefined, {
      now: () => 500,
    });

    await expect(coordinator.resetNow()).rejects.toThrow(
      "upstream reset failed",
    );
    expect(coordinator.snapshot()).toEqual({
      activeOperations: 0,
      resetting: false,
      generation: 0,
      nextResetAt: null,
    });

    await coordinator.resetNow();
    expect(client.resetWorkspace).toHaveBeenCalledTimes(2);
    expect(coordinator.snapshot()).toMatchObject({
      resetting: false,
      generation: 1,
      nextResetAt: 900_500,
    });
  });

  it("does not publish or schedule after a seed failure, then retries reset and start", async () => {
    const timer = { unref: vi.fn() };
    const setTimeout = vi.fn(() => timer);
    const client = { resetWorkspace: vi.fn(async () => undefined) };
    const seed = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("seed write failed"))
      .mockResolvedValue(undefined);
    const coordinator = new ResetCoordinator(client, seed, {
      now: () => 50,
      setTimeout,
    });

    await expect(coordinator.start()).rejects.toThrow("seed write failed");
    expect(coordinator.snapshot()).toEqual({
      activeOperations: 0,
      resetting: false,
      generation: 0,
      nextResetAt: null,
    });
    expect(setTimeout).not.toHaveBeenCalled();

    await coordinator.resetNow();
    expect(setTimeout).not.toHaveBeenCalled();
    expect(coordinator.snapshot()).toMatchObject({
      resetting: false,
      generation: 1,
      nextResetAt: 900_050,
    });

    await coordinator.start();
    expect(client.resetWorkspace).toHaveBeenCalledTimes(3);
    expect(seed).toHaveBeenCalledTimes(3);
    expect(setTimeout).toHaveBeenCalledTimes(1);
    expect(coordinator.snapshot()).toMatchObject({
      resetting: false,
      generation: 2,
      nextResetAt: 900_050,
    });
  });

  it("starts one unref'd timer and disposes it without using real timers", async () => {
    const timer = { unref: vi.fn() };
    const setTimeout = vi.fn(() => timer);
    const clearTimeout = vi.fn();
    const client = { resetWorkspace: vi.fn(async () => undefined) };
    const coordinator = new ResetCoordinator(client, async () => undefined, {
      setTimeout,
      clearTimeout,
    });

    await coordinator.start();
    await coordinator.start();

    expect(setTimeout).toHaveBeenCalledTimes(1);
    expect(setTimeout).toHaveBeenCalledWith(expect.any(Function), 900_000);
    expect(timer.unref).toHaveBeenCalledTimes(1);
    expect(client.resetWorkspace).toHaveBeenCalledTimes(1);
    coordinator.dispose();
    expect(clearTimeout).toHaveBeenCalledWith(timer);
  });

  it("does not install a timer when disposal races a pending start", async () => {
    const releaseReset = deferred<void>();
    const setTimeout = vi.fn(() => ({ unref: vi.fn() }));
    const client = { resetWorkspace: vi.fn(() => releaseReset.promise) };
    const coordinator = new ResetCoordinator(client, async () => undefined, {
      setTimeout,
    });

    const starting = coordinator.start();
    await Promise.resolve();
    coordinator.dispose();
    releaseReset.resolve();
    await starting;
    await coordinator.start();

    expect(setTimeout).not.toHaveBeenCalled();
    expect(client.resetWorkspace).toHaveBeenCalledTimes(1);
  });

  it("gates the workspace and retries shortly when a scheduled seed fails", async () => {
    let now = 100;
    const timers: Array<{
      callback: () => void;
      delayMs: number;
      handle: { unref: ReturnType<typeof vi.fn> };
    }> = [];
    const setTimeout = vi.fn((callback: () => void, delayMs: number) => {
      const handle = { unref: vi.fn() };
      timers.push({ callback, delayMs, handle });
      return handle;
    });
    const clearTimeout = vi.fn();
    const client = { resetWorkspace: vi.fn(async () => undefined) };
    const seed = vi
      .fn<() => Promise<void>>()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("transient seed failure"))
      .mockResolvedValueOnce(undefined);
    const coordinator = new ResetCoordinator(client, seed, {
      now: () => now,
      setTimeout,
      clearTimeout,
    });

    await coordinator.start();
    expect(timers[0]?.delayMs).toBe(900_000);

    now = 900_100;
    timers[0]?.callback();
    await vi.waitFor(() => expect(seed).toHaveBeenCalledTimes(2));
    await vi.waitFor(() =>
      expect(coordinator.snapshot()).toEqual({
        activeOperations: 0,
        resetting: true,
        generation: 1,
        nextResetAt: null,
      }),
    );
    await expect(
      coordinator.withOperation(async () => undefined),
    ).rejects.toBeInstanceOf(WorkspaceResettingError);
    expect(timers[1]?.delayMs).toBe(1_000);

    now = 901_100;
    timers[1]?.callback();
    await vi.waitFor(() =>
      expect(coordinator.snapshot()).toEqual({
        activeOperations: 0,
        resetting: false,
        generation: 2,
        nextResetAt: 1_801_100,
      }),
    );
    expect(client.resetWorkspace).toHaveBeenCalledTimes(3);
    expect(seed).toHaveBeenCalledTimes(3);
    expect(timers[2]?.delayMs).toBe(900_000);
  });
});

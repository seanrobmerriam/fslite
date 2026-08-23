import { EventEmitter } from "node:events";
import type { ChildProcess } from "node:child_process";

import { describe, expect, it, vi } from "vitest";

import {
  runFixtureLifecycle,
  spawnChecked,
  stopProcessGroup,
} from "../../e2e/fixtures";

function child(pid = 4123): ChildProcess {
  const fake = new EventEmitter() as ChildProcess;
  Object.assign(fake, { pid, exitCode: null, signalCode: null });
  return fake;
}

function esrch(): NodeJS.ErrnoException {
  return Object.assign(new Error("gone"), { code: "ESRCH" });
}

describe("E2E process lifecycle primitives", () => {
  it("rejects a process that emits spawn error", async () => {
    const fake = child();
    const result = spawnChecked(
      "missing",
      [],
      {},
      {
        spawn: () => {
          queueMicrotask(() => fake.emit("error", new Error("missing")));
          return fake;
        },
      },
    );
    await expect(result).rejects.toThrow("missing");
  });

  it("cleans acquired Rust and exact fixture path when setup readiness throws", async () => {
    const cleaned: string[] = [];
    await expect(
      runFixtureLifecycle(
        async (lifecycle) => {
          lifecycle.addCleanup(async () => {
            cleaned.push("rust");
          });
          lifecycle.addCleanup(async () => {
            cleaned.push("/tmp/fslite-showcase-e2e-exact");
          });
          throw new Error("readiness failed");
        },
        async () => undefined,
      ),
    ).rejects.toThrow("E2E fixture setup or cleanup failed");
    expect(cleaned).toEqual(["/tmp/fslite-showcase-e2e-exact", "rust"]);
  });

  it("continues cleanup after one cleanup throws", async () => {
    const cleaned: string[] = [];
    await expect(
      runFixtureLifecycle(
        async (lifecycle) => {
          lifecycle.addCleanup(async () => {
            cleaned.push("rust");
          });
          lifecycle.addCleanup(async () => {
            cleaned.push("broken");
            throw new Error("cleanup failed");
          });
          lifecycle.addCleanup(async () => {
            cleaned.push("path");
          });
          return undefined;
        },
        async () => undefined,
      ),
    ).rejects.toThrow("E2E fixture setup or cleanup failed");
    expect(cleaned).toEqual(["path", "broken", "rust"]);
  });

  it("returns when SIGTERM observes an already-exited group", async () => {
    const kill = vi.fn((pid: number, signal?: NodeJS.Signals | 0) => {
      if (pid < 0 && signal === "SIGTERM") throw esrch();
    });
    await stopProcessGroup(child(), { kill });
    expect(kill.mock.calls).toEqual([
      [4123, 0],
      [-4123, "SIGTERM"],
    ]);
  });

  it("escalates a TERM timeout to KILL and waits with another bound", async () => {
    const kill = vi.fn();
    const waitForExit = vi
      .fn()
      .mockResolvedValueOnce(false)
      .mockResolvedValueOnce(true);
    await stopProcessGroup(child(), { kill, waitForExit, timeoutMs: 7 });
    expect(kill.mock.calls).toEqual([
      [4123, 0],
      [-4123, "SIGTERM"],
      [-4123, "SIGKILL"],
    ]);
    expect(waitForExit).toHaveBeenCalledTimes(2);
    expect(waitForExit).toHaveBeenNthCalledWith(1, expect.anything(), 7);
    expect(waitForExit).toHaveBeenNthCalledWith(2, expect.anything(), 7);
  });

  it("returns if KILL races with group exit", async () => {
    const kill = vi.fn((pid: number, signal?: NodeJS.Signals | 0) => {
      if (pid < 0 && signal === "SIGKILL") throw esrch();
    });
    const waitForExit = vi.fn().mockResolvedValue(false);
    await stopProcessGroup(child(), { kill, waitForExit });
    expect(waitForExit).toHaveBeenCalledTimes(1);
  });
});

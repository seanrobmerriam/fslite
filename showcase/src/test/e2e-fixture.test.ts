import { EventEmitter } from "node:events";
import type { Server } from "node:net";

import { describe, expect, it } from "vitest";

import { freeLoopbackPort } from "../../e2e/fixtures";

class FailingListener extends EventEmitter {
  listening = false;
  closed = false;

  listen(): this {
    queueMicrotask(() => this.emit("error", new Error("port denied")));
    return this;
  }

  close(callback: (error?: Error) => void): this {
    this.closed = true;
    callback();
    return this;
  }

  address(): null {
    return null;
  }
}

describe("E2E fixture lifecycle", () => {
  it("closes a listener when loopback allocation emits error", async () => {
    const listener = new FailingListener();
    await expect(
      freeLoopbackPort(() => listener as unknown as Server),
    ).rejects.toThrow("port denied");
    expect(listener.closed).toBe(true);
  });

  it("allocates an actual loopback port without leaving a listener", async () => {
    await expect(freeLoopbackPort()).resolves.toBeGreaterThan(0);
  });
});

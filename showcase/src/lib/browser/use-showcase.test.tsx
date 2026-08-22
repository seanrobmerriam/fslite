import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ShowcaseApi } from "./api";
import { useShowcase } from "./use-showcase";

const activity = {
  id: "a",
  timestamp: "now",
  method: "GET",
  path: "/safe",
  status: 200,
  durationMs: 1,
  requestId: "r",
  request: null,
  response: null,
  curl: "curl",
};

function apiMock() {
  return {
    status: vi.fn().mockResolvedValue({
      ready: true,
      generation: 1,
      resetting: false,
      nextResetAt: 100,
      now: 1,
      usage: {},
    }),
    operation: vi.fn().mockResolvedValue({ data: { items: [] }, activity }),
    upload: vi.fn(),
    download: vi.fn(),
  } as unknown as ShowcaseApi;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useShowcase", () => {
  it("loads status and tree once, then polls the tree without appending background activity", async () => {
    vi.useFakeTimers();
    const api = apiMock();
    const { result, unmount } = renderHook(() => useShowcase(api));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(result.current.state.status?.generation).toBe(1);
    expect(result.current.state.activities).toEqual([activity]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(api.operation).toHaveBeenCalledTimes(2);
    expect(result.current.state.activities).toEqual([activity]);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("refreshes after one successful mutation but does not retry a failed mutation", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    (api.operation as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      data: {},
      activity: { ...activity, id: "write" },
    });

    await act(async () => {
      await result.current.runOperation({
        kind: "mkdir",
        path: "/new" as never,
        parents: false,
      });
    });

    expect(api.operation).toHaveBeenCalledTimes(3);
    (api.operation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      Object.assign(new Error("no"), { code: "bad_gateway" }),
    );
    await act(async () => {
      await expect(
        result.current.runOperation({
          kind: "mkdir",
          path: "/no" as never,
          parents: false,
        }),
      ).rejects.toThrow("no");
    });
    expect(api.operation).toHaveBeenCalledTimes(4);
  });

  it("aborts and ignores a late initial request after unmount", async () => {
    let resolveStatus!: (value: unknown) => void;
    const api = apiMock();
    (api.status as ReturnType<typeof vi.fn>).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveStatus = resolve;
        }),
    );
    const { result, unmount } = renderHook(() => useShowcase(api));
    unmount();

    await act(async () => {
      resolveStatus({
        ready: true,
        generation: 9,
        resetting: false,
        nextResetAt: 1,
        now: 1,
        usage: {},
      });
    });
    expect(result.current.state.status).toBeUndefined();
  });
});

import { render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { createDefaultClock, WorkspaceStatus } from "./WorkspaceStatus";

const emptyUsage = {
  active_logical_bytes: 0,
  trashed_logical_bytes: 0,
  staged_bytes: 0,
  active_nodes: 0,
  trashed_nodes: 0,
  max_logical_bytes: 10 * 1024 * 1024,
  max_nodes: 250,
  max_file_bytes: 1024 * 1024,
};

describe("WorkspaceStatus", () => {
  it("binds default browser timers to their receiver and cleans up on unmount", () => {
    const setInterval = vi.fn(function (this: unknown) {
      expect(this).toBe(timerHost);
      return 42;
    });
    const clearInterval = vi.fn(function (this: unknown, timer: unknown) {
      expect(this).toBe(timerHost);
      expect(timer).toBe(42);
    });
    const timerHost = {
      performance: { now: () => 1 },
      setInterval,
      clearInterval,
    };
    const { unmount } = render(
      <WorkspaceStatus
        clock={createDefaultClock(timerHost)}
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 1,
          nextResetAt: 2,
          usage: emptyUsage,
        }}
      />,
    );

    expect(setInterval).toHaveBeenCalledWith(expect.any(Function), 1_000);
    unmount();
    expect(clearInterval).toHaveBeenCalledTimes(1);
  });

  it("uses monotonic elapsed time rather than wall-clock jumps for the reset countdown", () => {
    vi.useFakeTimers({ now: 2_000 });
    let monotonicNow = 10_000;
    const clock = {
      monotonicNow: () => monotonicNow,
      setInterval: globalThis.setInterval,
      clearInterval: globalThis.clearInterval,
    };
    render(
      <WorkspaceStatus
        clock={clock}
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 1_000,
          nextResetAt: 61_000,
          usage: {
            active_logical_bytes: 1_048_576,
            trashed_logical_bytes: 0,
            staged_bytes: 0,
            active_nodes: 25,
            trashed_nodes: 0,
            max_logical_bytes: 1_048_576 * 10,
            max_nodes: 250,
            max_file_bytes: 1_048_576,
          },
        }}
      />,
    );

    expect(screen.getByText("1 MiB / 10 MiB")).toBeInTheDocument();
    expect(screen.getByText("25 / 250 nodes")).toBeInTheDocument();
    expect(screen.getByText("Reset in 1:00")).toBeInTheDocument();
    vi.setSystemTime(900_000);
    monotonicNow += 15_000;
    act(() => vi.advanceTimersByTime(15_000));
    expect(screen.getByText("Reset in 0:45")).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("renders quota denominators returned by the server", () => {
    render(
      <WorkspaceStatus
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 1_000,
          nextResetAt: null,
          usage: {
            active_logical_bytes: 2_097_152,
            trashed_logical_bytes: 0,
            staged_bytes: 0,
            active_nodes: 7,
            trashed_nodes: 0,
            max_logical_bytes: 5_242_880,
            max_nodes: 42,
            max_file_bytes: 524_288,
          },
        }}
      />,
    );

    expect(screen.getByText("2 MiB / 5 MiB")).toBeInTheDocument();
    expect(screen.getByText("7 / 42 nodes")).toBeInTheDocument();
  });

  it("renders null, skewed, and resetting schedules without going negative", () => {
    const { rerender } = render(
      <WorkspaceStatus
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 2_000,
          nextResetAt: null,
          usage: emptyUsage,
        }}
      />,
    );
    expect(screen.getByText("Reset schedule unavailable")).toBeInTheDocument();

    rerender(
      <WorkspaceStatus
        status={{
          ready: true,
          generation: 2,
          resetting: false,
          now: 2_000,
          nextResetAt: 1_000,
          usage: emptyUsage,
        }}
      />,
    );
    expect(screen.getByText("Reset in 0:00")).toBeInTheDocument();

    rerender(
      <WorkspaceStatus
        status={{
          ready: true,
          generation: 2,
          resetting: true,
          now: 2_000,
          nextResetAt: null,
          usage: emptyUsage,
        }}
      />,
    );
    expect(screen.getByText("Resetting workspace")).toBeInTheDocument();
  });

  it("reanchors a refreshed server schedule before applying further monotonic elapsed time", () => {
    vi.useFakeTimers();
    let monotonicNow = 1_000;
    const clock = {
      monotonicNow: () => monotonicNow,
      setInterval: globalThis.setInterval,
      clearInterval: globalThis.clearInterval,
    };
    const { rerender } = render(
      <WorkspaceStatus
        clock={clock}
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 1_000,
          nextResetAt: 61_000,
          usage: emptyUsage,
        }}
      />,
    );
    monotonicNow += 10_000;
    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByText("Reset in 0:50")).toBeInTheDocument();

    rerender(
      <WorkspaceStatus
        clock={clock}
        status={{
          ready: true,
          generation: 2,
          resetting: false,
          now: 20_000,
          nextResetAt: 80_000,
          usage: emptyUsage,
        }}
      />,
    );
    expect(screen.getByText("Reset in 1:00")).toBeInTheDocument();
    monotonicNow += 10_000;
    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByText("Reset in 0:50")).toBeInTheDocument();
    vi.useRealTimers();
  });
});

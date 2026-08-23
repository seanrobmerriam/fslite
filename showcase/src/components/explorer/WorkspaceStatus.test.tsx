import { render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceStatus } from "./WorkspaceStatus";

describe("WorkspaceStatus", () => {
  it("calls browser timers through their owning global object", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/explorer/WorkspaceStatus.tsx"),
      "utf8",
    );

    expect(source).toContain(
      "setInterval: globalThis.setInterval.bind(globalThis)",
    );
    expect(source).toContain(
      "clearInterval: globalThis.clearInterval.bind(globalThis)",
    );
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
            workspace_id: "private",
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

  it("renders null, skewed, and resetting schedules without going negative", () => {
    const { rerender } = render(
      <WorkspaceStatus
        status={{
          ready: true,
          generation: 1,
          resetting: false,
          now: 2_000,
          nextResetAt: null,
          usage: {},
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
          usage: {},
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
          usage: {},
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
          usage: {},
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
          usage: {},
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
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

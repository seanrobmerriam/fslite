import { render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceStatus } from "./WorkspaceStatus";

describe("WorkspaceStatus", () => {
  it("uses server time plus local elapsed time for reset countdown and exact public limits", () => {
    vi.useFakeTimers({ now: 2_000 });
    render(
      <WorkspaceStatus
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
    act(() => vi.advanceTimersByTime(15_000));
    expect(screen.getByText("Reset in 0:45")).toBeInTheDocument();
    vi.useRealTimers();
  });
});

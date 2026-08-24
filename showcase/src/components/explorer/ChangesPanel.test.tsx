import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Change } from "../../lib/shared/contracts";
import { ChangesPanel } from "./ChangesPanel";

const change = (sequence: number) =>
  ({
    sequence,
    kind: "modified" as const,
    node_id: "node",
    old_path: "/old.txt" as never,
    new_path: "/new.txt" as never,
    revision: sequence,
    created_at_ms: sequence,
    actor_metadata: {},
  }) satisfies Change;

describe("ChangesPanel", () => {
  it("uses opaque cursors and retains ordered unique changes across pages", async () => {
    const user = userEvent.setup();
    const onLoad = vi
      .fn()
      .mockResolvedValueOnce({
        items: [change(1), change(2)],
        next_cursor: "opaque-next",
      })
      .mockResolvedValueOnce({
        items: [change(2), change(3)],
        next_cursor: null,
      });
    render(<ChangesPanel generation={1} onLoad={onLoad} />);
    expect(await screen.findByText("#1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Load more changes" }));
    expect(onLoad).toHaveBeenLastCalledWith("opaque-next");
    expect(screen.getAllByText("#2")).toHaveLength(1);
    expect(
      screen.getAllByText(/#(1|2|3)/).map((item) => item.textContent),
    ).toEqual(["#1", "#2", "#3"]);
  });

  it("safely resets its page when the workspace generation changes", async () => {
    const onLoad = vi
      .fn()
      .mockResolvedValueOnce({ items: [change(1)], next_cursor: null })
      .mockResolvedValueOnce({ items: [change(9)], next_cursor: null });
    const view = render(<ChangesPanel generation={1} onLoad={onLoad} />);
    expect(await screen.findByText("#1")).toBeInTheDocument();
    view.rerender(<ChangesPanel generation={2} onLoad={onLoad} />);
    expect(await screen.findByText("#9")).toBeInTheDocument();
    expect(screen.queryByText("#1")).not.toBeInTheDocument();
  });
});

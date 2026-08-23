import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { TrashEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { TrashPanel } from "./TrashPanel";

const item = {
  id: "018f3a64-1234-7123-8abc-123456789abc",
  original_path: "/docs/readme.txt" as VirtualPath,
  trashed_at_ms: 1,
  actor_metadata: {},
  node: {
    workspace_id: "private",
    id: "node",
    parent_id: null,
    name: "readme.txt",
    kind: "file" as const,
    logical_size: 2,
    created_at_ms: 1,
    modified_at_ms: 1,
    accessed_at_ms: 1,
    revision: 1,
    attributes: {},
  },
} satisfies TrashEntry;

describe("TrashPanel", () => {
  it("removes a restored item locally without a second visitor list request", async () => {
    const user = userEvent.setup();
    const onList = vi.fn().mockResolvedValue({ items: [item] });
    const onOperation = vi.fn().mockResolvedValue(undefined);
    render(<TrashPanel onList={onList} onOperation={onOperation} />);
    await screen.findByText("/docs/readme.txt");
    expect(onList).toHaveBeenCalledTimes(1);

    await user.click(
      screen.getByRole("button", { name: "Restore readme.txt" }),
    );
    const destination = screen.getByRole("textbox", {
      name: "Restore destination",
    });
    await user.type(destination, "/restored/readme.txt");
    await user.click(screen.getByRole("button", { name: "Confirm restore" }));
    expect(onOperation).toHaveBeenCalledWith({
      kind: "restore",
      trashId: item.id,
      destination: "/restored/readme.txt" as VirtualPath,
    });
    expect(onList).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("/docs/readme.txt")).not.toBeInTheDocument();
  });

  it("requires the exact live name before purging", async () => {
    const user = userEvent.setup();
    const onOperation = vi.fn().mockResolvedValue(undefined);
    render(
      <TrashPanel
        onList={vi.fn().mockResolvedValue({ items: [item] })}
        onOperation={onOperation}
      />,
    );
    await screen.findByText("/docs/readme.txt");
    await user.click(screen.getByRole("button", { name: "Purge readme.txt" }));
    const confirm = screen.getByRole("textbox", { name: "Confirm name" });
    await user.type(confirm, "readme");
    expect(
      screen.getByRole("button", { name: "Purge permanently" }),
    ).toBeDisabled();
    await user.type(confirm, ".txt");
    await user.click(screen.getByRole("button", { name: "Purge permanently" }));
    expect(onOperation).toHaveBeenCalledWith({
      kind: "purge",
      trashId: item.id,
      confirmedName: "readme.txt",
    });
    expect(screen.queryByText("/docs/readme.txt")).not.toBeInTheDocument();
  });
});

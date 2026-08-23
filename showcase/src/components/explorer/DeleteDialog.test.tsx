import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { DeleteDialog } from "./DeleteDialog";

const file = {
  path: "/docs/readme.txt" as VirtualPath,
  depth: 1,
  node: {
    workspace_id: "workspace",
    id: "readme",
    parent_id: "docs",
    name: "readme.txt",
    kind: "file" as const,
    logical_size: 3,
    created_at_ms: 1,
    modified_at_ms: 1,
    accessed_at_ms: 1,
    revision: 4,
    attributes: {},
  },
} satisfies TreeEntry;

describe("DeleteDialog", () => {
  it("uses trash by default with the selected entry revision", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<DeleteDialog entry={file} onSubmit={onSubmit} onClose={vi.fn()} />);

    expect(screen.getByRole("radio", { name: /move to trash/i })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Move to trash" }));

    expect(onSubmit).toHaveBeenCalledWith({
      kind: "trash",
      path: "/docs/readme.txt",
      expectedRevision: 4,
    });
  });

  it("separates permanent removal and requires the exact full path", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<DeleteDialog entry={file} onSubmit={onSubmit} onClose={vi.fn()} />);

    await user.click(
      screen.getByRole("radio", { name: /delete permanently/i }),
    );
    const confirmation = screen.getByRole("textbox", {
      name: "Confirm full path",
    });
    expect(
      screen.getByRole("button", { name: "Delete permanently" }),
    ).toBeDisabled();
    await user.type(confirmation, "/docs/README.txt");
    expect(
      screen.getByRole("button", { name: "Delete permanently" }),
    ).toBeDisabled();
    await user.clear(confirmation);
    await user.type(confirmation, "/docs/readme.txt");
    await user.click(
      screen.getByRole("button", { name: "Delete permanently" }),
    );

    expect(onSubmit).toHaveBeenCalledWith({
      kind: "remove",
      path: "/docs/readme.txt",
      recursive: false,
      confirmedPath: "/docs/readme.txt",
    });
  });
});

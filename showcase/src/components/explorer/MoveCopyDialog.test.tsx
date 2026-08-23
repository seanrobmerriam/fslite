import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { MoveCopyDialog } from "./MoveCopyDialog";

const directory = {
  path: "/docs" as VirtualPath,
  depth: 0,
  node: {
    workspace_id: "workspace",
    id: "docs",
    parent_id: null,
    name: "docs",
    kind: "directory" as const,
    logical_size: 0,
    created_at_ms: 1,
    modified_at_ms: 1,
    accessed_at_ms: 1,
    revision: 1,
    attributes: {},
  },
} satisfies TreeEntry;

describe("MoveCopyDialog", () => {
  it("copies a directory recursively to a canonical destination", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <MoveCopyDialog
        entry={directory}
        mode="copy"
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );

    const destination = screen.getByRole("textbox", { name: "Destination" });
    expect(destination).toHaveFocus();
    await user.clear(destination);
    await user.type(destination, "/archive/docs-copy");
    await user.click(screen.getByRole("button", { name: "Copy" }));

    expect(onSubmit).toHaveBeenCalledWith({
      kind: "copy",
      from: "/docs",
      to: "/archive/docs-copy",
      recursive: true,
    });
  });

  it("renames by sending a move and refuses traversal destinations inline", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <MoveCopyDialog
        entry={directory}
        mode="rename"
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );

    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "../outside");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/single path segment/i);
    expect(onSubmit).not.toHaveBeenCalled();

    await user.clear(name);
    await user.type(name, "guides");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "move",
      from: "/docs",
      to: "/guides",
    });
  });
});

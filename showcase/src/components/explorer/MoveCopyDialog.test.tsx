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

const file = {
  ...directory,
  path: "/docs/readme.txt" as VirtualPath,
  node: {
    ...directory.node,
    id: "readme",
    name: "readme.txt",
    kind: "file" as const,
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

  it("moves files and copies files non-recursively", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <MoveCopyDialog
        entry={file}
        mode="move"
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );

    const destination = screen.getByRole("textbox", { name: "Destination" });
    await user.clear(destination);
    await user.type(destination, "/archive/readme.txt");
    await user.click(screen.getByRole("button", { name: "Move" }));
    expect(onSubmit).toHaveBeenLastCalledWith({
      kind: "move",
      from: "/docs/readme.txt",
      to: "/archive/readme.txt",
    });

    rerender(
      <MoveCopyDialog
        entry={file}
        mode="copy"
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );
    const copiedDestination = screen.getByRole("textbox", {
      name: "Destination",
    });
    await user.clear(copiedDestination);
    await user.type(copiedDestination, "/archive/readme-copy.txt");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(onSubmit).toHaveBeenLastCalledWith({
      kind: "copy",
      from: "/docs/readme.txt",
      to: "/archive/readme-copy.txt",
      recursive: false,
    });
  });

  it("rejects self and descendant destinations before submit", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(
      <MoveCopyDialog
        entry={directory}
        mode="copy"
        onSubmit={onSubmit}
        onClose={vi.fn()}
      />,
    );

    const destination = screen.getByRole("textbox", { name: "Destination" });
    await user.clear(destination);
    await user.type(destination, "/docs");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(screen.getByRole("alert")).toHaveTextContent(
      /must not be the item/i,
    );

    await user.clear(destination);
    await user.type(destination, "/docs/archive");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/descendants/i);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});

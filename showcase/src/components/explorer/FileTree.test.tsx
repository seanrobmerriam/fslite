import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { FileTree } from "./FileTree";

const entries = [
  {
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
  },
  {
    path: "/docs/readme.txt" as VirtualPath,
    depth: 1,
    node: {
      workspace_id: "workspace",
      id: "readme",
      parent_id: "docs",
      name: "readme.txt",
      kind: "file" as const,
      logical_size: 5,
      created_at_ms: 1,
      modified_at_ms: 1,
      accessed_at_ms: 1,
      revision: 2,
      attributes: {},
    },
  },
  {
    path: "/todo.txt" as VirtualPath,
    depth: 0,
    node: {
      workspace_id: "workspace",
      id: "todo",
      parent_id: null,
      name: "todo.txt",
      kind: "file" as const,
      logical_size: 3,
      created_at_ms: 1,
      modified_at_ms: 1,
      accessed_at_ms: 1,
      revision: 1,
      attributes: {},
    },
  },
] satisfies TreeEntry[];

describe("FileTree", () => {
  it("provides a hierarchical roving tree that expands, selects, and follows keyboard navigation", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <FileTree
        entries={entries}
        selectedPath={undefined}
        onSelect={onSelect}
      />,
    );

    const tree = screen.getByRole("tree", { name: "Files" });
    const docs = screen.getByRole("treeitem", { name: /docs/i });
    expect(tree).toBeInTheDocument();
    expect(docs).toHaveAttribute("aria-level", "1");
    expect(docs).toHaveAttribute("aria-expanded", "false");
    expect(docs).toHaveAttribute("aria-setsize", "2");
    expect(docs).toHaveAttribute("aria-posinset", "1");

    docs.focus();
    await user.keyboard("{ArrowRight}");
    expect(docs).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("treeitem", { name: /readme.txt/i })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(docs).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(docs).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("treeitem", { name: /todo.txt/i })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(docs).toHaveFocus();
    await user.keyboard("{ArrowRight}{ArrowDown}");
    expect(screen.getByRole("treeitem", { name: /readme.txt/i })).toHaveFocus();
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenLastCalledWith(entries[1]);
    await user.keyboard("{End}");
    expect(screen.getByRole("treeitem", { name: /todo.txt/i })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(docs).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onSelect).toHaveBeenLastCalledWith(entries[0]);
  });
});

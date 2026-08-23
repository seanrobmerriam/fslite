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

  it("opens a named node action menu, closes it with Escape, and restores menu-button focus", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();
    render(
      <FileTree
        entries={entries}
        selectedPath={undefined}
        onSelect={vi.fn()}
        onAction={onAction}
      />,
    );

    const actions = screen.getByRole("button", {
      name: "Actions for todo.txt",
    });
    await user.click(actions);
    expect(
      screen.getByRole("menu", { name: "Actions for todo.txt" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Move to trash" }));
    expect(onAction).toHaveBeenCalledWith(
      entries[2],
      "trash",
      expect.any(HTMLButtonElement),
    );

    await user.click(actions);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(actions).toHaveFocus();
  });

  it("keeps node action menus keyboard navigable and closes them on click-away", async () => {
    const user = userEvent.setup();
    render(
      <>
        <FileTree
          entries={entries}
          selectedPath={undefined}
          onSelect={vi.fn()}
          onAction={vi.fn()}
        />
        <button type="button">Outside tree</button>
      </>,
    );

    await user.click(
      screen.getByRole("button", { name: "Actions for todo.txt" }),
    );
    const rename = screen.getByRole("menuitem", { name: "Rename" });
    const move = screen.getByRole("menuitem", { name: "Move" });
    expect(rename).toHaveFocus();
    expect(rename).toHaveAttribute("tabindex", "0");
    expect(move).toHaveAttribute("tabindex", "-1");

    await user.keyboard("{ArrowDown}");
    expect(move).toHaveFocus();
    expect(rename).toHaveAttribute("tabindex", "-1");
    expect(move).toHaveAttribute("tabindex", "0");
    await user.keyboard("{End}");
    expect(
      screen.getByRole("menuitem", { name: "Delete permanently" }),
    ).toHaveFocus();
    await user.keyboard("{Home}");
    expect(rename).toHaveFocus();

    await user.keyboard("{Tab}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Outside tree" })).toHaveFocus();

    await user.click(
      screen.getByRole("button", { name: "Actions for todo.txt" }),
    );
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Actions for docs" }),
    ).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Outside tree" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

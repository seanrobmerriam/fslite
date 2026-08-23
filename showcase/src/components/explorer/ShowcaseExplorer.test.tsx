import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Node, TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { ShowcaseExplorer } from "./ShowcaseExplorer";

const showcaseMock = vi.hoisted(() => ({
  reloadServerVersion: vi.fn(),
  runOperation: vi.fn(),
  upload: vi.fn(),
  tree: [] as TreeEntry[],
  selectedPath: undefined as VirtualPath | undefined,
  selectedNode: undefined as Node | undefined,
  editor: {
    path: undefined as VirtualPath | undefined,
    text: "local draft",
    original: "",
    dirty: true,
  },
  status: {
    ready: true,
    generation: 1,
    resetting: true,
    now: 1,
    nextResetAt: null,
    usage: {
      active_logical_bytes: 0,
      active_nodes: 0,
      max_logical_bytes: 10 * 1_048_576,
      max_nodes: 250,
    },
  },
}));

vi.mock("../../lib/browser/use-showcase", () => ({
  useShowcase: () => ({
    state: {
      status: showcaseMock.status,
      tree: showcaseMock.tree,
      selectedPath: showcaseMock.selectedPath,
      selectedNode: showcaseMock.selectedNode,
      editor: showcaseMock.editor,
      busyAction: undefined,
      error: undefined,
      revisionConflict: {
        path: "/readme.txt",
        message: "Another visitor saved this file.",
      },
    },
    refresh: vi.fn(),
    runOperation: showcaseMock.runOperation,
    selectEntry: vi.fn(),
    setEditorText: vi.fn(),
    save: vi.fn(),
    download: vi.fn(),
    upload: showcaseMock.upload,
    clearRevisionConflict: vi.fn(),
    reloadServerVersion: showcaseMock.reloadServerVersion,
  }),
}));

describe("ShowcaseExplorer", () => {
  it("disables mutations during reset and lets a visitor copy unsaved conflict text", async () => {
    showcaseMock.status.resetting = true;
    const writeText = vi.fn();
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    expect(globalThis.navigator.clipboard.writeText).toBe(writeText);
    render(<ShowcaseExplorer />);

    expect(screen.getByRole("button", { name: "New file" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Refresh files" }),
    ).toBeDisabled();
    expect(screen.getAllByText(/resetting/i)).not.toHaveLength(0);
    fireEvent.click(
      screen.getByRole("button", { name: "Copy my unsaved text" }),
    );
    expect(writeText).toHaveBeenCalledWith("local draft");
  });

  it("offers an explicit server reload conflict action when the workspace is available", () => {
    showcaseMock.status.resetting = false;
    render(<ShowcaseExplorer />);

    const reload = screen.getByRole("button", {
      name: "Reload server version",
    });
    expect(reload).toBeEnabled();
    fireEvent.click(reload);
    expect(showcaseMock.reloadServerVersion).toHaveBeenCalledTimes(1);
  });

  it("opens a labelled create dialog at the selected directory, isolates the background, and sends its exact mutation", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockResolvedValue(undefined);
    const { container } = render(<ShowcaseExplorer />);

    await user.click(screen.getByRole("button", { name: "New file" }));
    const dialog = screen.getByRole("dialog", { name: "Create file" });
    expect(dialog).toHaveAccessibleDescription(/create a file in \/./i);
    expect(container.querySelector(".showcase-explorer")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "draft.txt");
    await user.click(screen.getByRole("button", { name: "Create file" }));

    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "write_file",
        path: "/draft.txt",
        text: "",
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  it("protects a dirty nested draft before moving, trashing, or deleting its parent directory", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.tree = [
      {
        path: "/docs",
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "docs",
          parent_id: null,
          name: "docs",
          kind: "directory",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/docs" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/docs/readme.txt" as VirtualPath,
      text: "local draft",
      original: "server copy",
      dirty: true,
    };
    render(<ShowcaseExplorer />);

    await user.click(screen.getByRole("button", { name: "Actions for docs" }));
    await user.click(screen.getByRole("menuitem", { name: "Move to trash" }));

    expect(
      screen.getByRole("dialog", { name: "Unsaved changes" }),
    ).toHaveAccessibleDescription(/currently open in the editor/i);
  });
});

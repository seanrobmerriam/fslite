import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Node, TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { ShowcaseExplorer } from "./ShowcaseExplorer";

const showcaseMock = vi.hoisted(() => ({
  reloadServerVersion: vi.fn(),
  runOperation: vi.fn(),
  runReadOperation: vi
    .fn()
    .mockResolvedValue({ data: { items: [], next_cursor: null } }),
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
      activities: [],
      error: undefined,
      revisionConflict: {
        path: "/readme.txt",
        message: "Another visitor saved this file.",
      },
    },
    refresh: vi.fn(),
    runOperation: showcaseMock.runOperation,
    runReadOperation: showcaseMock.runReadOperation,
    selectEntry: vi.fn(),
    setEditorText: vi.fn(),
    save: vi.fn(),
    download: vi.fn(),
    upload: showcaseMock.upload,
    clearRevisionConflict: vi.fn(),
    reloadServerVersion: showcaseMock.reloadServerVersion,
    clearActivities: vi.fn(),
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

  it("uses labelled automatic tabs with keyboard selection while API activity stays below every work area", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    render(<ShowcaseExplorer />);

    const explorer = screen.getByRole("tab", { name: "Explorer" });
    expect(explorer).toHaveAttribute("aria-controls", "explorer-panel");
    expect(
      screen.getByRole("tabpanel", { name: "Explorer" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: "API activity" }),
    ).toBeInTheDocument();
    explorer.focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Search" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole("tabpanel", { name: "Search" }),
    ).toBeInTheDocument();
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Changes" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{Home}");
    expect(explorer).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Changes" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("tab", { name: "Trash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard("{Enter}");
    expect(screen.getByRole("tab", { name: "Trash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await user.keyboard(" ");
    expect(screen.getByRole("tab", { name: "Trash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("opens a labelled create dialog at the selected directory, isolates the background, and sends its exact mutation", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
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

  it("guards then executes trash for a directory containing a dirty draft", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockResolvedValue(undefined);
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
    await user.click(screen.getByRole("button", { name: "Move to trash" }));

    expect(
      screen.getByRole("dialog", { name: "Unsaved changes" }),
    ).toHaveAccessibleDescription(/currently open in the editor/i);
    expect(showcaseMock.runOperation).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );
    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "trash",
        path: "/docs",
        expectedRevision: 1,
      }),
    );
  });

  it("guards then executes a move that changes the dirty draft path", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/docs" as VirtualPath,
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "docs-move",
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
    await user.click(screen.getByRole("menuitem", { name: "Move" }));
    const destination = screen.getByRole("textbox", { name: "Destination" });
    await user.clear(destination);
    await user.type(destination, "/archive/docs");
    await user.click(screen.getByRole("button", { name: "Move" }));
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );

    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "move",
        from: "/docs",
        to: "/archive/docs",
      }),
    );
  });

  it("guards then executes confirmed permanent delete for a dirty draft parent", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/docs" as VirtualPath,
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "docs-delete",
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
    await user.click(
      screen.getByRole("menuitem", { name: "Delete permanently" }),
    );
    await user.click(screen.getByRole("radio", { name: "Delete permanently" }));
    await user.type(
      screen.getByRole("textbox", { name: "Confirm full path" }),
      "/docs",
    );
    await user.click(
      screen.getByRole("button", { name: "Delete permanently" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );

    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "remove",
        path: "/docs",
        recursive: true,
        confirmedPath: "/docs",
      }),
    );
  });

  it("guards copy targets that would overwrite the open dirty draft", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/source.txt" as VirtualPath,
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "source",
          parent_id: null,
          name: "source.txt",
          kind: "file",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/source.txt" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/docs/readme.txt" as VirtualPath,
      text: "local draft",
      original: "server copy",
      dirty: true,
    };
    render(<ShowcaseExplorer />);

    await user.click(
      screen.getByRole("button", { name: "Actions for source.txt" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Copy" }));
    const destination = screen.getByRole("textbox", { name: "Destination" });
    await user.clear(destination);
    await user.type(destination, "/docs/readme.txt");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(
      screen.getByRole("dialog", { name: "Unsaved changes" }),
    ).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );

    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "copy",
        from: "/source.txt",
        to: "/docs/readme.txt",
        recursive: false,
      }),
    );
  });

  it("returns focus to a tree action trigger after cancel, Escape, and success", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/todo.txt" as VirtualPath,
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "todo",
          parent_id: null,
          name: "todo.txt",
          kind: "file",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/todo.txt" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/todo.txt" as VirtualPath,
      text: "server copy",
      original: "server copy",
      dirty: false,
    };
    render(<ShowcaseExplorer />);

    const trigger = screen.getByRole("button", {
      name: "Actions for todo.txt",
    });
    await user.click(trigger);
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "renamed.txt");
    await user.click(screen.getByRole("button", { name: "Rename" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(trigger).toHaveFocus();
  });

  it("uses the stable explorer fallback after a successful action refresh removes its trigger", async () => {
    const user = userEvent.setup();
    let resolveOperation: (() => void) | undefined;
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveOperation = resolve;
        }),
    );
    showcaseMock.tree = [
      {
        path: "/todo.txt" as VirtualPath,
        depth: 0,
        node: {
          workspace_id: "workspace",
          id: "todo-removed",
          parent_id: null,
          name: "todo.txt",
          kind: "file",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/todo.txt" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/todo.txt" as VirtualPath,
      text: "server copy",
      original: "server copy",
      dirty: false,
    };
    const view = render(<ShowcaseExplorer />);

    await user.click(
      screen.getByRole("button", { name: "Actions for todo.txt" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "renamed.txt");
    await user.click(screen.getByRole("button", { name: "Rename" }));

    showcaseMock.tree = [];
    showcaseMock.selectedPath = undefined;
    showcaseMock.selectedNode = undefined;
    view.rerender(<ShowcaseExplorer />);
    resolveOperation?.();

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("region", { name: "Filesystem explorer" }),
    ).toHaveFocus();
  });

  it("requires an explicit draft decision before create-file or upload overwrite", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.runOperation.mockReset();
    showcaseMock.runOperation.mockResolvedValue(undefined);
    showcaseMock.upload.mockReset();
    showcaseMock.upload.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/docs/readme.txt" as VirtualPath,
        depth: 1,
        node: {
          workspace_id: "workspace",
          id: "readme",
          parent_id: "docs",
          name: "readme.txt",
          kind: "file",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/docs/readme.txt" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/docs/readme.txt" as VirtualPath,
      text: "local draft",
      original: "server copy",
      dirty: true,
    };
    render(<ShowcaseExplorer />);

    await user.click(screen.getByRole("button", { name: "New file" }));
    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "readme.txt");
    await user.click(screen.getByRole("button", { name: "Create file" }));
    expect(
      screen.getByRole("dialog", { name: "Unsaved changes" }),
    ).toBeInTheDocument();
    expect(showcaseMock.runOperation).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );
    await waitFor(() =>
      expect(showcaseMock.runOperation).toHaveBeenCalledWith({
        kind: "write_file",
        path: "/docs/readme.txt",
        text: "",
      }),
    );
  });

  it("requires an explicit draft decision before an upload overwrites the open path", async () => {
    const user = userEvent.setup();
    showcaseMock.status.resetting = false;
    showcaseMock.upload.mockReset();
    showcaseMock.upload.mockResolvedValue(undefined);
    showcaseMock.tree = [
      {
        path: "/docs/readme.txt" as VirtualPath,
        depth: 1,
        node: {
          workspace_id: "workspace",
          id: "readme-upload",
          parent_id: "docs",
          name: "readme.txt",
          kind: "file",
          logical_size: 0,
          created_at_ms: 1,
          modified_at_ms: 1,
          accessed_at_ms: 1,
          revision: 1,
          attributes: {},
        },
      },
    ] as TreeEntry[];
    showcaseMock.selectedPath = "/docs/readme.txt" as VirtualPath;
    showcaseMock.selectedNode = showcaseMock.tree[0]?.node;
    showcaseMock.editor = {
      path: "/docs/readme.txt" as VirtualPath,
      text: "local draft",
      original: "server copy",
      dirty: true,
    };
    render(<ShowcaseExplorer />);

    const file = new File(["replacement"], "readme.txt");
    await user.click(screen.getByRole("button", { name: "Upload" }));
    await user.upload(screen.getByLabelText("File"), file);
    await user.click(screen.getByRole("button", { name: "Upload file" }));
    expect(
      screen.getByRole("dialog", { name: "Unsaved changes" }),
    ).toBeInTheDocument();
    expect(showcaseMock.upload).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: "Continue without saving" }),
    );
    await waitFor(() =>
      expect(showcaseMock.upload).toHaveBeenCalledWith(
        "/docs/readme.txt",
        file,
      ),
    );
  });
});

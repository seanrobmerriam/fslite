import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ShowcaseExplorer } from "./ShowcaseExplorer";

const showcaseMock = vi.hoisted(() => ({
  reloadServerVersion: vi.fn(),
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
      tree: [],
      selectedPath: undefined,
      selectedNode: undefined,
      editor: {
        path: undefined,
        text: "local draft",
        original: "",
        dirty: true,
      },
      busyAction: undefined,
      error: undefined,
      revisionConflict: {
        path: "/readme.txt",
        message: "Another visitor saved this file.",
      },
    },
    refresh: vi.fn(),
    runOperation: vi.fn(),
    selectEntry: vi.fn(),
    setEditorText: vi.fn(),
    save: vi.fn(),
    download: vi.fn(),
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
});

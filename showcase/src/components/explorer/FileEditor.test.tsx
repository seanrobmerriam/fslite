import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { Node } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { FileEditor } from "./FileEditor";

const textNode = {
  workspace_id: "workspace",
  id: "readme",
  parent_id: null,
  name: "readme.txt",
  kind: "file" as const,
  logical_size: 5,
  created_at_ms: 1,
  modified_at_ms: 1,
  accessed_at_ms: 1,
  revision: 7,
  attributes: {},
} satisfies Node;

describe("FileEditor", () => {
  it("edits dirty text and saves from the button or Ctrl/Cmd+S", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <FileEditor
        node={textNode}
        path={"/readme.txt" as VirtualPath}
        text="hello"
        dirty
        busy={false}
        resetting={false}
        onChange={onChange}
        onSave={onSave}
        onDownload={vi.fn()}
      />,
    );

    const editor = screen.getByRole("textbox", { name: "File contents" });
    await user.type(editor, "!");
    expect(onChange).toHaveBeenCalled();
    expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
    await user.keyboard("{Control>}s{/Control}");
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(
      fireEvent.keyDown(editor, { key: "s", metaKey: true, cancelable: true }),
    ).toBe(false);
    expect(onSave).toHaveBeenCalledTimes(2);
    await user.click(screen.getByRole("button", { name: "Save file" }));
    expect(onSave).toHaveBeenCalledTimes(3);
  });

  it("offers download without decoding binary content", async () => {
    const onDownload = vi.fn();
    render(
      <FileEditor
        node={{ ...textNode, logical_size: 2 }}
        path={"/image.bin" as VirtualPath}
        text=""
        dirty={false}
        binary
        busy={false}
        resetting={false}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onDownload={onDownload}
      />,
    );

    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByText(/Binary file/i)).toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Download file" }));
    expect(onDownload).toHaveBeenCalledWith("/image.bin");
  });

  it("does not intercept Ctrl/Cmd+S when saving is not valid", () => {
    const { rerender } = render(
      <FileEditor
        node={textNode}
        path={"/readme.txt" as VirtualPath}
        text="hello"
        dirty={false}
        busy={false}
        resetting={false}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onDownload={vi.fn()}
      />,
    );
    const editor = screen.getByRole("textbox", { name: "File contents" });
    expect(
      fireEvent.keyDown(editor, { key: "s", ctrlKey: true, cancelable: true }),
    ).toBe(true);

    rerender(
      <FileEditor
        node={textNode}
        path={"/readme.txt" as VirtualPath}
        text="hello"
        dirty
        busy={false}
        resetting
        onChange={vi.fn()}
        onSave={vi.fn()}
        onDownload={vi.fn()}
      />,
    );
    expect(
      fireEvent.keyDown(editor, { key: "s", metaKey: true, cancelable: true }),
    ).toBe(true);
  });
});

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { VirtualPath } from "../../lib/shared/path";
import { CreateDialog } from "./CreateDialog";

describe("CreateDialog", () => {
  it("creates a file at the selected canonical directory and returns focus to its invoker", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const invoker = document.createElement("button");
    invoker.textContent = "New file";
    document.body.append(invoker);
    invoker.focus();

    const view = render(
      <CreateDialog
        directory={"/docs" as VirtualPath}
        kind="file"
        onCreate={onCreate}
        onClose={onClose}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Create file" });
    const name = screen.getByRole("textbox", { name: "Name" });
    expect(dialog).toHaveAccessibleDescription(/create a file in \/docs/i);
    expect(name).toHaveFocus();
    await user.clear(name);
    await user.type(name, "notes.txt");
    await user.click(screen.getByRole("button", { name: "Create file" }));

    expect(onCreate).toHaveBeenCalledWith({
      kind: "write_file",
      path: "/docs/notes.txt",
      text: "",
    });

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    view.unmount();
    expect(invoker).toHaveFocus();
    invoker.remove();
  });

  it("keeps invalid names in the dialog with an inline canonical-path error", async () => {
    const user = userEvent.setup();
    render(
      <CreateDialog
        directory={"/" as VirtualPath}
        kind="folder"
        onCreate={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "../outside");
    await user.click(screen.getByRole("button", { name: "Create folder" }));

    expect(screen.getByRole("alert")).toHaveTextContent(/single path segment/i);
  });
});

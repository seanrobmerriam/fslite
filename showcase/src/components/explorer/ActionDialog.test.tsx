import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { ActionDialog } from "./ActionDialog";

function DialogHarness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open dialog
      </button>
      {open ? (
        <ActionDialog
          title="Keyboard dialog"
          description="A safe keyboard test."
          onClose={() => setOpen(false)}
        >
          <button type="button">First action</button>
          <button type="button">Last action</button>
        </ActionDialog>
      ) : null}
    </>
  );
}

describe("ActionDialog", () => {
  it("contains Tab and Shift+Tab, closes with Escape, and returns focus to its invoker", async () => {
    const user = userEvent.setup();
    render(<DialogHarness />);
    const invoker = screen.getByRole("button", { name: "Open dialog" });
    await user.click(invoker);

    expect(
      screen.getByRole("dialog", { name: "Keyboard dialog" }),
    ).toHaveAccessibleDescription("A safe keyboard test.");
    expect(screen.getByRole("button", { name: "First action" })).toHaveFocus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(screen.getByRole("button", { name: "Last action" })).toHaveFocus();
    await user.keyboard("{Tab}");
    expect(screen.getByRole("button", { name: "First action" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(invoker).toHaveFocus();
  });

  it("uses an explicit return target and keeps focus on the dialog shell while busy", async () => {
    const user = userEvent.setup();
    const invoker = document.createElement("button");
    invoker.textContent = "Tree menu trigger";
    document.body.append(invoker);
    const onClose = vi.fn();
    const { rerender, unmount } = render(
      <ActionDialog
        title="Busy dialog"
        description="Busy work stays contained."
        returnFocusTarget={invoker}
        onClose={onClose}
      >
        <fieldset>
          <button type="button">First action</button>
        </fieldset>
      </ActionDialog>,
    );

    expect(screen.getByRole("button", { name: "First action" })).toHaveFocus();
    rerender(
      <ActionDialog
        title="Busy dialog"
        description="Busy work stays contained."
        returnFocusTarget={invoker}
        onClose={onClose}
        busy
        closeable={false}
      >
        <fieldset disabled>
          <button type="button">First action</button>
        </fieldset>
      </ActionDialog>,
    );

    const dialog = screen.getByRole("dialog", { name: "Busy dialog" });
    expect(dialog).toHaveAttribute("tabindex", "-1");
    expect(dialog).toHaveFocus();
    await user.keyboard("{Tab}{Escape}");
    expect(dialog).toHaveFocus();
    expect(onClose).not.toHaveBeenCalled();
    unmount();
    expect(invoker).toHaveFocus();
    invoker.remove();
  });
});

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";

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
});

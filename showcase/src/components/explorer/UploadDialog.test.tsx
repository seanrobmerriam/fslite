import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { MAX_BROWSER_FILE_BYTES } from "../../lib/browser/api";
import type { VirtualPath } from "../../lib/shared/path";
import { UploadDialog } from "./UploadDialog";

describe("UploadDialog", () => {
  it("rejects files over 1 MiB before calling the upload callback", async () => {
    const user = userEvent.setup();
    const onUpload = vi.fn();
    render(
      <UploadDialog
        directory={"/incoming" as VirtualPath}
        onUpload={onUpload}
        onClose={vi.fn()}
      />,
    );

    const file = new File(
      [new Uint8Array(MAX_BROWSER_FILE_BYTES + 1)],
      "large.bin",
    );
    await user.upload(screen.getByLabelText("File"), file);
    await user.click(screen.getByRole("button", { name: "Upload file" }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      /must not exceed 1048576 bytes/i,
    );
    expect(onUpload).not.toHaveBeenCalled();
  });

  it("uploads a selected file at the selected directory", async () => {
    const user = userEvent.setup();
    const onUpload = vi.fn().mockResolvedValue(undefined);
    render(
      <UploadDialog
        directory={"/incoming" as VirtualPath}
        onUpload={onUpload}
        onClose={vi.fn()}
      />,
    );

    const file = new File(["hi"], "hello.txt", { type: "text/plain" });
    await user.upload(screen.getByLabelText("File"), file);
    await user.click(screen.getByRole("button", { name: "Upload file" }));

    expect(onUpload).toHaveBeenCalledWith("/incoming/hello.txt", file);
  });
});

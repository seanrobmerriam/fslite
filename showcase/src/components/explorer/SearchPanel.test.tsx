import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { VirtualPath } from "../../lib/shared/path";
import { SearchPanel } from "./SearchPanel";

describe("SearchPanel", () => {
  it("emits the fixed filename, absolute glob, and content operation payloads", async () => {
    const user = userEvent.setup();
    const onSearch = vi
      .fn()
      .mockResolvedValue({ items: [{ path: "/docs/readme.txt" }] });
    render(<SearchPanel onSearch={onSearch} onSelectPath={vi.fn()} />);

    await user.type(
      screen.getByRole("textbox", { name: "Search text" }),
      "readme",
    );
    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(onSearch).toHaveBeenLastCalledWith({
      kind: "find",
      root: "/" as VirtualPath,
      nameContains: "readme",
    });
    expect(
      await screen.findByRole("button", { name: "/docs/readme.txt" }),
    ).toBeEnabled();

    await user.click(screen.getByRole("radio", { name: "Glob" }));
    const glob = screen.getByRole("textbox", { name: "Search text" });
    await user.clear(glob);
    await user.type(glob, "/docs/*.txt");
    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(onSearch).toHaveBeenLastCalledWith({
      kind: "glob",
      pattern: "/docs/*.txt",
    });

    await user.click(screen.getByRole("radio", { name: "Contents" }));
    const content = screen.getByRole("textbox", { name: "Search text" });
    await user.clear(content);
    await user.type(content, "welcome");
    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(onSearch).toHaveBeenLastCalledWith({
      kind: "search_content",
      root: "/" as VirtualPath,
      text: "welcome",
    });
  });

  it("rejects a non-canonical root before requesting and keeps a usable error state", async () => {
    const user = userEvent.setup();
    const onSearch = vi.fn();
    render(<SearchPanel onSearch={onSearch} onSelectPath={vi.fn()} />);
    await user.clear(screen.getByRole("textbox", { name: "Search root" }));
    await user.type(
      screen.getByRole("textbox", { name: "Search root" }),
      "/docs/../secret",
    );
    await user.type(
      screen.getByRole("textbox", { name: "Search text" }),
      "secret",
    );
    await user.click(screen.getByRole("button", { name: "Search" }));

    expect(onSearch).not.toHaveBeenCalled();
    expect(screen.getByRole("alert")).toHaveTextContent(/canonical/i);
  });
});

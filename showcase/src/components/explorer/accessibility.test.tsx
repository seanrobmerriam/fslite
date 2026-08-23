import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ShowcaseExplorer } from "./ShowcaseExplorer";
import { ToastRegion } from "./ToastRegion";

const showcaseMock = vi.hoisted(() => ({
  status: undefined as
    | {
        ready: boolean;
        generation: number;
        resetting: boolean;
        now: number;
        nextResetAt: number | null;
        usage: {
          active_logical_bytes: number;
          active_nodes: number;
          max_logical_bytes: number;
          max_nodes: number;
        };
      }
    | undefined,
  error: undefined as Error | undefined,
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
      activities: [],
      error: showcaseMock.error,
      revisionConflict: undefined,
    },
    refresh: vi.fn(),
    runOperation: vi.fn(),
    runReadOperation: vi.fn(),
    selectEntry: vi.fn(),
    setEditorText: vi.fn(),
    save: vi.fn(),
    download: vi.fn(),
    upload: vi.fn(),
    reloadServerVersion: vi.fn(),
    clearActivities: vi.fn(),
  }),
}));

const source = (path: string) =>
  readFileSync(resolve(process.cwd(), path), "utf8");

describe("showcase accessibility and responsive presentation", () => {
  it("keeps the page to one labelled h1", () => {
    const page = source("src/pages/index.astro");
    expect(page.match(/<h1\b/g)).toHaveLength(1);
    expect(page).toContain('id="showcase-title"');
  });

  it("labels explorer controls and explains unavailable workspace actions", () => {
    showcaseMock.status = undefined;
    showcaseMock.error = new Error("The filesystem service is unavailable.");
    render(<ShowcaseExplorer />);

    expect(screen.getByRole("button", { name: "New file" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "New folder" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Upload" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Refresh files" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("status", { name: "Workspace availability" }),
    ).toHaveTextContent(/backend unavailable/i);
    expect(
      screen.getByText(
        /actions are unavailable until the workspace reconnects/i,
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/seeded files will appear here/i),
    ).toBeInTheDocument();
  });

  it("uses a polite live toast for reset and error updates", () => {
    render(
      <ToastRegion
        error={new Error("The filesystem service is unavailable.")}
        resetting
      />,
    );

    expect(
      screen.getByRole("status", { name: "Workspace notices" }),
    ).toHaveTextContent(/unsaved editor text stays/i);
    expect(screen.getByRole("alert")).toHaveTextContent(/unavailable/i);
  });

  it("defines the editorial shell, focus treatment, motion fallback, and narrow layout", () => {
    const styles = source("src/styles/global.css");

    expect(styles).toContain("--content-max-width: 75rem");
    expect(styles).toContain(
      "grid-template-columns: minmax(240px, 0.36fr) minmax(0, 1fr)",
    );
    expect(styles).toContain("@media (max-width: 47.5rem)");
    expect(styles).toContain(":focus-visible");
    expect(styles).toContain("@media (prefers-reduced-motion: reduce)");
    expect(styles).toContain("min-height: 2.75rem");
  });
});

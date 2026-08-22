import { describe, expect, it } from "vitest";

import {
  initialShowcaseState,
  showcaseReducer,
  type ShowcaseState,
} from "./reducer";
import type { TreeEntry } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";

const entry = {
  path: "/docs/readme.txt" as VirtualPath,
  depth: 1,
  node: {
    workspace_id: "not-for-browser-display",
    id: "node-1",
    parent_id: null,
    name: "readme.txt",
    kind: "file" as const,
    logical_size: 5,
    created_at_ms: 1,
    modified_at_ms: 1,
    accessed_at_ms: 1,
    revision: 3,
    attributes: {},
  },
} satisfies TreeEntry;

function reduce(
  ...actions: Parameters<typeof showcaseReducer>[1][]
): ShowcaseState {
  return actions.reduce(showcaseReducer, initialShowcaseState);
}

describe("showcaseReducer", () => {
  it("loads status and tree, then selects a node without inventing editor text", () => {
    const state = reduce(
      {
        type: "status_loaded",
        status: {
          ready: true,
          generation: 1,
          resetting: false,
          nextResetAt: 100,
          now: 1,
          usage: {},
        },
      },
      { type: "tree_loaded", entries: [entry], background: false },
      { type: "selected", entry },
    );

    expect(state.status?.generation).toBe(1);
    expect(state.tree).toEqual([entry]);
    expect(state.selectedPath).toBe("/docs/readme.txt");
    expect(state.editor.text).toBe("");
  });

  it("keeps unsaved editor text across a ten-second background tree refresh", () => {
    const state = reduce(
      { type: "selected", entry },
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "local unsaved" },
      { type: "tree_loaded", entries: [], background: true },
    );

    expect(state.editor).toMatchObject({
      text: "local unsaved",
      original: "server",
      dirty: true,
    });
    expect(state.activities).toEqual([]);
  });

  it("preserves unsaved text when the reset generation changes and exposes resetting", () => {
    const state = reduce(
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "local unsaved" },
      {
        type: "status_loaded",
        status: {
          ready: true,
          generation: 2,
          resetting: true,
          nextResetAt: 200,
          now: 2,
          usage: {},
        },
      },
    );

    expect(state.status).toMatchObject({ generation: 2, resetting: true });
    expect(state.editor).toMatchObject({ text: "local unsaved", dirty: true });
  });

  it("appends and clears only visitor activity", () => {
    const activity = {
      id: "a",
      timestamp: "now",
      method: "POST",
      path: "/safe",
      status: 200,
      durationMs: 1,
      requestId: "r",
      request: null,
      response: null,
      curl: "curl",
    };
    const appended = reduce({ type: "activity_appended", activity });
    const cleared = showcaseReducer(appended, { type: "activities_cleared" });

    expect(appended.activities).toEqual([activity]);
    expect(cleared.activities).toEqual([]);
  });

  it("records a revision conflict without overwriting the local edit", () => {
    const state = reduce(
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "local unsaved" },
      { type: "revision_conflict", path: entry.path, message: "changed" },
    );

    expect(state.editor).toMatchObject({ text: "local unsaved", dirty: true });
    expect(state.revisionConflict).toMatchObject({
      path: entry.path,
      message: "changed",
    });
  });
});

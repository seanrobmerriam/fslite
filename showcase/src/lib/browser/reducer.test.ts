import { describe, expect, it } from "vitest";

import {
  MAX_ACTIVITY_RECORDS,
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

  it("retains the 100 newest activity records in chronological order", () => {
    const state = Array.from({
      length: MAX_ACTIVITY_RECORDS + 5,
    }).reduce<ShowcaseState>(
      (current, _, index) =>
        showcaseReducer(current, {
          type: "activity_appended",
          activity: {
            id: String(index),
            timestamp: "2026-08-22T00:00:00.000Z",
            method: "GET",
            path: "/safe",
            status: 200,
            durationMs: 0,
            requestId: String(index),
            request: null,
            response: null,
            curl: "curl",
          },
        }),
      initialShowcaseState,
    );

    expect(state.activities).toHaveLength(MAX_ACTIVITY_RECORDS);
    expect(state.activities[0]?.id).toBe("5");
    expect(state.activities.at(-1)?.id).toBe(String(MAX_ACTIVITY_RECORDS + 4));
  });

  it("preserves a matching dirty edit while adopting a coherent loaded baseline", () => {
    const state = reduce(
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "local unsaved" },
      {
        type: "editor_loaded",
        path: entry.path,
        text: "new server",
        revision: 4,
      },
    );

    expect(state.editor).toEqual({
      path: entry.path,
      text: "local unsaved",
      original: "new server",
      revision: 4,
      dirty: true,
    });
  });

  it("uses a write response as the editor baseline without discarding later typing", () => {
    const state = reduce(
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "saved" },
      { type: "editor_saved", path: entry.path, text: "saved", revision: 4 },
    );

    expect(state.editor).toMatchObject({
      text: "saved",
      original: "saved",
      revision: 4,
      dirty: false,
    });
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

  it("keeps a dirty same-path draft intact when a delayed read proves the server version is binary", () => {
    const state = reduce(
      { type: "selected", entry },
      { type: "editor_loaded", path: entry.path, text: "server", revision: 3 },
      { type: "editor_changed", text: "local unsaved" },
      { type: "editor_binary", path: entry.path, revision: 4 },
    );

    expect(state.editor).toEqual({
      path: entry.path,
      text: "local unsaved",
      original: "server",
      revision: 3,
      dirty: true,
    });
    expect(state.revisionConflict).toMatchObject({ path: entry.path });
  });
});

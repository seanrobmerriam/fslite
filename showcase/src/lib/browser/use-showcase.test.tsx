import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { TreeEntry } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import { ShowcaseError, type ShowcaseApi } from "./api";
import { useShowcase } from "./use-showcase";

const activity = {
  id: "a",
  timestamp: "now",
  method: "GET",
  path: "/safe",
  status: 200,
  durationMs: 1,
  requestId: "r",
  request: null,
  response: null,
  curl: "curl",
};

const usage = {
  workspace_id: "not-rendered",
  active_logical_bytes: 1,
  trashed_logical_bytes: 0,
  staged_bytes: 0,
  active_nodes: 1,
  trashed_nodes: 0,
  max_logical_bytes: 10,
  max_nodes: 250,
  max_file_bytes: 1024,
};

const fileEntry = {
  path: "/readme.txt" as VirtualPath,
  depth: 0,
  node: {
    workspace_id: usage.workspace_id,
    id: "node-1",
    parent_id: null,
    name: "readme.txt",
    kind: "file" as const,
    logical_size: 4,
    created_at_ms: 1,
    modified_at_ms: 1,
    accessed_at_ms: 1,
    revision: 3,
    attributes: {},
  },
} satisfies TreeEntry;

const tree = { items: [fileEntry], next_cursor: null };

function bytes(text: string) {
  return Object.fromEntries(
    [...new TextEncoder().encode(text)].map((byte, index) => [index, byte]),
  );
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function apiMock() {
  return {
    status: vi.fn().mockResolvedValue({
      ready: true,
      generation: 1,
      resetting: false,
      nextResetAt: 100,
      now: 1,
      usage,
    }),
    operation: vi.fn().mockResolvedValue({ data: tree, activity }),
    upload: vi.fn(),
    download: vi.fn(),
  } as unknown as ShowcaseApi;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("useShowcase", () => {
  it("transitions from ready to unavailable on a deferred upstream refresh failure and recovers on retry", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));

    await waitFor(() =>
      expect(
        (result.current.state as { availability?: string }).availability,
      ).toBe("ready"),
    );
    const priorStatus = result.current.state.status;
    const outage = deferred<typeof priorStatus>();
    (api.status as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      outage.promise,
    );

    let refresh!: Promise<readonly TreeEntry[]>;
    act(() => {
      refresh = result.current.refresh(false);
    });
    outage.reject(
      new ShowcaseError(
        "upstream_unavailable",
        "The filesystem service is unavailable.",
        502,
      ),
    );
    await act(async () => {
      await refresh;
    });

    expect(
      (result.current.state as { availability?: string }).availability,
    ).toBe("unavailable");
    expect(result.current.state.status).toEqual(priorStatus);
    expect(result.current.state.tree).toEqual(tree.items);

    await act(async () => {
      await result.current.refresh(false);
    });

    expect(
      (result.current.state as { availability?: string }).availability,
    ).toBe("ready");
    expect(result.current.state.error).toBeUndefined();
  });

  it("keeps a ready browser state for deferred non-connectivity refresh errors", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() =>
      expect(result.current.state.availability).toBe("ready"),
    );

    for (const [code, status] of [
      ["rate_limited", 429],
      ["workspace_resetting", 503],
      ["invalid_response", 502],
    ] as const) {
      const failure = deferred<typeof usage>();
      (api.status as ReturnType<typeof vi.fn>).mockReturnValueOnce(
        failure.promise,
      );
      let refresh!: Promise<readonly TreeEntry[]>;
      act(() => {
        refresh = result.current.refresh(false);
      });
      failure.reject(new ShowcaseError(code, code, status));
      await act(async () => {
        await refresh;
      });

      expect(result.current.state.availability).toBe("ready");
      expect(result.current.state.error).toMatchObject({ code });
    }
  });

  it("keeps initial contract failures checking while surfacing their public error", async () => {
    const api = apiMock();
    const contractFailure = deferred<typeof usage>();
    (api.status as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      contractFailure.promise,
    );
    const { result } = renderHook(() => useShowcase(api));

    contractFailure.reject(
      new ShowcaseError(
        "invalid_response",
        "Invalid response from server.",
        502,
      ),
    );
    await waitFor(() => expect(result.current.state.error).toBeDefined());

    expect(result.current.state.status).toBeUndefined();
    expect(result.current.state.availability).toBe("checking");
    expect(result.current.state.error).toMatchObject({
      code: "invalid_response",
    });
  });

  it("marks a tree refresh unavailable while preserving the last complete status", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    const priorStatus = result.current.state.status;
    (api.operation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new ShowcaseError(
        "upstream_unavailable",
        "The filesystem service is unavailable.",
        503,
      ),
    );

    await act(async () => {
      await result.current.refresh(false);
    });

    expect(result.current.state.availability).toBe("unavailable");
    expect(result.current.state.status).toEqual(priorStatus);
  });

  it("does not mark validation, conflict, rate, or reset errors unavailable", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() =>
      expect(result.current.state.availability).toBe("ready"),
    );

    for (const code of [
      "invalid_request",
      "revision_conflict",
      "rate_limited",
      "workspace_resetting",
    ]) {
      (api.operation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
        new ShowcaseError(code, code, 409),
      );
      await act(async () => {
        await expect(
          result.current.runOperation({
            kind: "mkdir",
            path: `/${code}` as VirtualPath,
            parents: false,
          }),
        ).rejects.toMatchObject({ code });
      });
      expect(result.current.state.availability).toBe("ready");
    }
  });

  it("keeps a nullable reset timestamp in browser state", async () => {
    const api = apiMock();
    (api.status as ReturnType<typeof vi.fn>).mockResolvedValue({
      ready: true,
      generation: 1,
      resetting: true,
      nextResetAt: null,
      now: 1,
      usage,
    });
    const { result } = renderHook(() => useShowcase(api));

    await waitFor(() => expect(result.current.state.status).toBeDefined());

    expect(result.current.state.status?.nextResetAt).toBeNull();
  });

  it("loads status and tree once, then polls the tree without appending background activity", async () => {
    vi.useFakeTimers();
    const api = apiMock();
    const { result, unmount } = renderHook(() => useShowcase(api));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(result.current.state.status?.generation).toBe(1);
    expect(result.current.state.activities).toEqual([activity]);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });

    expect(api.operation).toHaveBeenCalledTimes(2);
    expect(result.current.state.activities).toEqual([activity]);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("refreshes after one successful mutation but does not retry a failed mutation", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    (api.operation as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      data: {},
      activity: { ...activity, id: "write" },
    });

    await act(async () => {
      await result.current.runOperation({
        kind: "mkdir",
        path: "/new" as never,
        parents: false,
      });
    });

    expect(api.operation).toHaveBeenCalledTimes(3);
    (api.operation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      Object.assign(new Error("no"), { code: "bad_gateway" }),
    );
    await act(async () => {
      await expect(
        result.current.runOperation({
          kind: "mkdir",
          path: "/no" as never,
          parents: false,
        }),
      ).rejects.toThrow("no");
    });
    expect(api.operation).toHaveBeenCalledTimes(4);
  });

  it("records a visitor read once without treating discovery as a tree refresh", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    (api.operation as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      data: { items: [], next_cursor: null },
      activity: { ...activity, id: "find" },
    });
    await act(async () => {
      await result.current.runReadOperation({
        kind: "find",
        root: "/" as never,
        nameContains: "readme",
      });
    });
    expect(api.operation).toHaveBeenCalledTimes(2);
    expect(result.current.state.activities.map((item) => item.id)).toEqual([
      "a",
      "find",
    ]);
  });

  it("appends failed visitor activity exactly once without retrying", async () => {
    const api = apiMock();
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    (api.operation as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new ShowcaseError(
        "upstream_unavailable",
        "The filesystem service is unavailable.",
        502,
        "request-failed",
        undefined,
        { ...activity, id: "failed", status: 502 },
      ),
    );
    await act(async () => {
      await expect(
        result.current.runReadOperation({ kind: "usage" }),
      ).rejects.toMatchObject({ code: "upstream_unavailable" });
    });
    expect(result.current.state.activities.map((item) => item.id)).toEqual([
      "a",
      "failed",
    ]);
    expect(api.operation).toHaveBeenCalledTimes(2);
  });

  it("atomically rejects a mutation while a visitor read owns the workbench", async () => {
    const api = apiMock();
    const pending = deferred<{ data: unknown; activity: typeof activity }>();
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree")
          return Promise.resolve({ data: tree, activity });
        return pending.promise;
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    let read!: Promise<unknown>;
    act(() => {
      read = result.current.runReadOperation({ kind: "usage" });
    });
    await expect(
      result.current.runOperation({ kind: "usage" }),
    ).rejects.toMatchObject({ code: "operation_in_progress" });
    await act(async () => {
      pending.resolve({ data: {}, activity });
      await read;
    });
  });

  it("atomically rejects a visitor read while a mutation owns the workbench", async () => {
    const api = apiMock();
    const pending = deferred<{ data: unknown; activity: typeof activity }>();
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree")
          return Promise.resolve({ data: tree, activity });
        return pending.promise;
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    let mutation!: Promise<unknown>;
    act(() => {
      mutation = result.current.runOperation({ kind: "usage" });
    });
    await expect(
      result.current.runReadOperation({ kind: "usage" }),
    ).rejects.toMatchObject({ code: "operation_in_progress" });
    await act(async () => {
      pending.resolve({ data: {}, activity });
      await mutation;
    });
  });

  it("normalizes a download failure into application error state without activity or rejection", async () => {
    const api = apiMock();
    (api.download as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      Object.assign(new Error("Download failed."), { code: "bad_gateway" }),
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    const activities = result.current.state.activities;

    await act(async () => {
      await expect(
        result.current.download("/readme.txt" as VirtualPath),
      ).resolves.toBeUndefined();
    });

    expect(result.current.state.error).toMatchObject({
      message: "Download failed.",
    });
    expect(result.current.state.activities).toEqual(activities);
  });

  it("aborts and ignores a late initial request after unmount", async () => {
    let resolveStatus!: (value: unknown) => void;
    const api = apiMock();
    (api.status as ReturnType<typeof vi.fn>).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveStatus = resolve;
        }),
    );
    const { result, unmount } = renderHook(() => useShowcase(api));
    unmount();

    await act(async () => {
      resolveStatus({
        ready: true,
        generation: 9,
        resetting: false,
        nextResetAt: 1,
        now: 1,
        usage,
      });
    });
    expect(result.current.state.status).toBeUndefined();
  });

  it("uses returned write revisions for consecutive saves without rereading", async () => {
    const api = apiMock();
    let revision = 3;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree") {
          return Promise.resolve({ data: tree, activity });
        }
        if (operation.kind === "read_file") {
          return Promise.resolve({ data: bytes("base"), activity });
        }
        if (operation.kind === "write_file") {
          revision += 1;
          return Promise.resolve({
            data: { ...fileEntry.node, revision },
            activity: { ...activity, id: `write-${revision}` },
          });
        }
        throw new Error(`unexpected ${operation.kind}`);
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await act(async () => {
      await result.current.selectEntry(fileEntry);
    });

    act(() => result.current.setEditorText("first"));
    await act(async () => {
      await result.current.save();
    });
    act(() => result.current.setEditorText("second"));
    await act(async () => {
      await result.current.save();
    });

    const writes = (api.operation as ReturnType<typeof vi.fn>).mock.calls
      .map(([operation]) => operation)
      .filter((operation) => operation.kind === "write_file");
    expect(writes).toEqual([
      expect.objectContaining({ expectedRevision: 3, text: "first" }),
      expect.objectContaining({ expectedRevision: 4, text: "second" }),
    ]);
    expect(
      (api.operation as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([operation]) => operation.kind === "read_file",
      ),
    ).toHaveLength(1);
    expect(result.current.state.editor).toMatchObject({
      text: "second",
      original: "second",
      revision: 5,
      dirty: false,
    });
  });

  it("aborts and ignores stale same-path read successes and errors", async () => {
    const api = apiMock();
    const first = deferred<{ data: unknown; activity: typeof activity }>();
    const second = deferred<{ data: unknown; activity: typeof activity }>();
    const signals: AbortSignal[] = [];
    let reads = 0;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation, signal) => {
        if (operation.kind === "tree") {
          return Promise.resolve({ data: tree, activity });
        }
        if (operation.kind !== "read_file") {
          throw new Error(`unexpected ${operation.kind}`);
        }
        signals.push(signal);
        reads += 1;
        return reads === 1 ? first.promise : second.promise;
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());

    let firstSelection!: Promise<void>;
    let secondSelection!: Promise<void>;
    act(() => {
      firstSelection = result.current.selectEntry(fileEntry);
      secondSelection = result.current.selectEntry(fileEntry);
    });
    expect(signals[0]?.aborted).toBe(true);
    await act(async () => {
      second.resolve({ data: bytes("latest"), activity });
      await secondSelection;
    });
    await act(async () => {
      first.reject(new Error("stale read"));
      await firstSelection;
    });

    expect(result.current.state.editor).toMatchObject({
      text: "latest",
      original: "latest",
      dirty: false,
    });
    expect(result.current.state.error).toBeUndefined();
  });

  it("preserves user typing when a current same-path read settles after editing", async () => {
    const api = apiMock();
    const delayed = deferred<{ data: unknown; activity: typeof activity }>();
    let reads = 0;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree") {
          return Promise.resolve({ data: tree, activity });
        }
        if (operation.kind === "read_file") {
          reads += 1;
          return reads === 1
            ? Promise.resolve({ data: bytes("base"), activity })
            : delayed.promise;
        }
        throw new Error(`unexpected ${operation.kind}`);
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await act(async () => {
      await result.current.selectEntry(fileEntry);
    });
    let reload!: Promise<void>;
    act(() => {
      reload = result.current.selectEntry(fileEntry);
    });
    act(() => result.current.setEditorText("local"));
    await act(async () => {
      delayed.resolve({ data: bytes("new base"), activity });
      await reload;
    });

    expect(result.current.state.editor).toMatchObject({
      text: "local",
      original: "new base",
      revision: 3,
      dirty: true,
    });
  });

  it("keeps a dirty same-path draft when a delayed read contains invalid UTF-8 or NUL bytes", async () => {
    const api = apiMock();
    const delayed = deferred<{ data: unknown; activity: typeof activity }>();
    let reads = 0;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree")
          return Promise.resolve({ data: tree, activity });
        if (operation.kind === "read_file") {
          reads += 1;
          return reads === 1
            ? Promise.resolve({ data: bytes("server"), activity })
            : delayed.promise;
        }
        throw new Error(`unexpected ${operation.kind}`);
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await act(async () => {
      await result.current.selectEntry(fileEntry);
    });

    let reread!: Promise<void>;
    act(() => {
      reread = result.current.selectEntry(fileEntry);
      result.current.setEditorText("local draft");
    });
    await act(async () => {
      delayed.resolve({ data: [0xc3, 0x28], activity });
      await reread;
    });

    expect(result.current.state.editor).toMatchObject({
      path: fileEntry.path,
      text: "local draft",
      original: "server",
      revision: 3,
      dirty: true,
    });
    expect(result.current.state.editor).not.toHaveProperty("binary");
    expect(result.current.state.revisionConflict).toMatchObject({
      path: fileEntry.path,
    });
  });

  it("marks clean invalid UTF-8 and NUL-containing files as binary without decoding them", async () => {
    const api = apiMock();
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree")
          return Promise.resolve({ data: tree, activity });
        if (operation.kind === "read_file")
          return Promise.resolve({ data: [0, 65], activity });
        throw new Error(`unexpected ${operation.kind}`);
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());

    await act(async () => {
      await result.current.selectEntry(fileEntry);
    });

    expect(result.current.state.editor).toMatchObject({
      path: fileEntry.path,
      text: "",
      original: "",
      binary: true,
      dirty: false,
    });
  });

  it("reloads a revision conflict only after refreshing the current server revision", async () => {
    const api = apiMock();
    const refreshedEntry = {
      ...fileEntry,
      node: { ...fileEntry.node, revision: 4 },
    };
    let treeReads = 0;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation) => {
        if (operation.kind === "tree") {
          treeReads += 1;
          return Promise.resolve({
            data: { items: treeReads === 1 ? [fileEntry] : [refreshedEntry] },
            activity,
          });
        }
        if (operation.kind === "read_file")
          return Promise.resolve({ data: bytes("server two"), activity });
        if (operation.kind === "write_file") {
          return Promise.reject(
            Object.assign(new Error("Another visitor changed this file."), {
              code: "revision_conflict",
            }),
          );
        }
        throw new Error(`unexpected ${operation.kind}`);
      },
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await act(async () => {
      await result.current.selectEntry(fileEntry);
    });
    act(() => result.current.setEditorText("local draft"));
    await act(async () => {
      await expect(result.current.save()).rejects.toMatchObject({
        code: "revision_conflict",
      });
    });
    expect(result.current.state.editor.dirty).toBe(true);

    await act(async () => {
      await result.current.reloadServerVersion();
    });

    expect(result.current.state.editor).toMatchObject({
      text: "server two",
      original: "server two",
      revision: 4,
      dirty: false,
    });
    expect(result.current.state.revisionConflict).toBeUndefined();
  });

  it("aborts an in-flight file read on unmount and ignores its late result", async () => {
    const api = apiMock();
    const pendingRead = deferred<{
      data: unknown;
      activity: typeof activity;
    }>();
    let signal: AbortSignal | undefined;
    (api.operation as ReturnType<typeof vi.fn>).mockImplementation(
      (operation, nextSignal) => {
        if (operation.kind === "tree") {
          return Promise.resolve({ data: tree, activity });
        }
        signal = nextSignal;
        return pendingRead.promise;
      },
    );
    const { result, unmount } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    let selection!: Promise<void>;
    act(() => {
      selection = result.current.selectEntry(fileEntry);
    });
    unmount();
    expect(signal?.aborted).toBe(true);
    await act(async () => {
      pendingRead.resolve({ data: bytes("late"), activity });
      await selection;
    });
    expect(result.current.state.editor.path).toBeUndefined();
  });

  it("routes uploads through mutation lifecycle and reports failures without retries", async () => {
    const api = apiMock();
    const pendingUpload = deferred<{
      data: unknown;
      activity: typeof activity;
    }>();
    (api.upload as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      pendingUpload.promise,
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.upload(
        "/new.txt" as never,
        new File(["new"], "new.txt"),
      );
    });
    expect(result.current.state.busyAction).toBe("upload");
    await act(async () => {
      pendingUpload.resolve({
        data: {},
        activity: { ...activity, id: "upload" },
      });
      await pending;
    });
    expect(result.current.state.busyAction).toBeUndefined();
    expect(result.current.state.activities).toContainEqual(
      expect.objectContaining({ id: "upload" }),
    );

    (api.upload as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error("upload failed"),
    );
    await act(async () => {
      await expect(
        result.current.upload(
          "/fail.txt" as never,
          new File(["x"], "fail.txt"),
        ),
      ).rejects.toThrow("upload failed");
    });
    expect(api.upload).toHaveBeenCalledTimes(2);
    expect(result.current.state.error).toMatchObject({
      message: "upload failed",
    });
  });

  it("prevents upload and JSON mutations from overlapping in either direction", async () => {
    const api = apiMock();
    const pendingUpload = deferred<{
      data: unknown;
      activity: typeof activity;
    }>();
    const pendingOperation = deferred<{
      data: unknown;
      activity: typeof activity;
    }>();
    (api.upload as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      pendingUpload.promise,
    );
    const { result } = renderHook(() => useShowcase(api));
    await waitFor(() => expect(result.current.state.status).toBeDefined());
    await waitFor(() => expect(api.operation).toHaveBeenCalledTimes(1));

    let upload!: Promise<void>;
    act(() => {
      upload = result.current.upload(
        "/new.txt" as never,
        new File(["new"], "new.txt"),
      );
    });
    await expect(
      result.current.runOperation({
        kind: "mkdir",
        path: "/blocked" as never,
        parents: false,
      }),
    ).rejects.toMatchObject({ code: "operation_in_progress" });
    await act(async () => {
      pendingUpload.resolve({ data: {}, activity });
      await upload;
    });

    (api.operation as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      pendingOperation.promise,
    );
    let operation!: Promise<unknown>;
    act(() => {
      operation = result.current.runOperation({
        kind: "mkdir",
        path: "/new-dir" as never,
        parents: false,
      });
    });
    await expect(
      result.current.upload(
        "/blocked.txt" as never,
        new File(["x"], "blocked.txt"),
      ),
    ).rejects.toMatchObject({ code: "operation_in_progress" });
    await act(async () => {
      pendingOperation.resolve({ data: {}, activity });
      await operation;
    });
  });
});

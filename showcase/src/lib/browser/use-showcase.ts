import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";

import type { GatewayResult, Node, TreeEntry } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import type { PublicOperation } from "../server/schemas";
import { ShowcaseApi, ShowcaseError } from "./api";
import {
  initialShowcaseState,
  showcaseReducer,
  type ShowcaseAction,
} from "./reducer";

const BACKGROUND_REFRESH_MS = 10_000;

interface ShowcaseApiLike {
  status(signal?: AbortSignal): ReturnType<ShowcaseApi["status"]>;
  operation<T>(
    operation: PublicOperation,
    signal?: AbortSignal,
  ): Promise<GatewayResult<T>>;
  upload: ShowcaseApi["upload"];
  download: ShowcaseApi["download"];
}

function isRevisionConflict(
  error: unknown,
): error is { code: string; message: string } {
  if (!error || typeof error !== "object") {
    return false;
  }
  const value = error as { code?: unknown; message?: unknown };
  return (
    value.code === "revision_conflict" && typeof value.message === "string"
  );
}

function decodeText(data: unknown): string {
  if (typeof data === "string") {
    return data;
  }
  const bytes = Array.isArray(data)
    ? data
    : data && typeof data === "object"
      ? Object.values(data as Record<string, unknown>)
      : undefined;
  if (
    !bytes ||
    !bytes.every(
      (value) => typeof value === "number" && value >= 0 && value <= 255,
    )
  ) {
    throw new ShowcaseError(
      "invalid_response",
      "The selected file is not available as text.",
      422,
    );
  }
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(
      new Uint8Array(bytes),
    );
    if (text.includes("\0")) {
      throw new TypeError("NUL bytes are not editable text");
    }
    return text;
  } catch {
    throw new ShowcaseError(
      "invalid_response",
      "The selected file is not valid UTF-8 text.",
      422,
    );
  }
}

/**
 * The stateful React-island boundary. Browser work starts only in effects or
 * user events, so importing and server-rendering this module remain safe.
 */
export function useShowcase(api: ShowcaseApiLike = new ShowcaseApi()) {
  const [state, dispatch] = useReducer(showcaseReducer, initialShowcaseState);
  const stateRef = useRef(state);
  const apiRef = useRef(api);
  const mountedRef = useRef(false);
  const refreshRef = useRef<AbortController | undefined>(undefined);
  const refreshEpochRef = useRef(0);
  const readRef = useRef<AbortController | undefined>(undefined);
  const readEpochRef = useRef(0);
  const mutationRef = useRef(false);
  const mutationControllerRef = useRef<AbortController | undefined>(undefined);
  stateRef.current = state;
  apiRef.current = api;

  const dispatchIfMounted = useCallback((action: ShowcaseAction) => {
    if (mountedRef.current) {
      dispatch(action);
    }
  }, []);

  const refresh = useCallback(
    async (background = false): Promise<readonly TreeEntry[]> => {
      refreshRef.current?.abort();
      const controller = new AbortController();
      refreshRef.current = controller;
      const epoch = ++refreshEpochRef.current;
      const current = () =>
        mountedRef.current &&
        !controller.signal.aborted &&
        epoch === refreshEpochRef.current;

      try {
        const status = await apiRef.current.status(controller.signal);
        if (!current()) return [];
        dispatch({ type: "status_loaded", status });
        const tree = await apiRef.current.operation<{ items: TreeEntry[] }>(
          { kind: "tree", path: "/" as VirtualPath },
          controller.signal,
        );
        if (!current()) return [];
        dispatch({
          type: "tree_loaded",
          entries: tree.data.items ?? [],
          background,
        });
        if (!background) {
          dispatch({ type: "activity_appended", activity: tree.activity });
        }
        dispatch({ type: "error_set", error: undefined });
        return tree.data.items ?? [];
      } catch (error) {
        if (
          current() &&
          !(error instanceof DOMException && error.name === "AbortError")
        ) {
          dispatch({ type: "error_set", error: error as Error });
        }
        return [];
      } finally {
        if (refreshRef.current === controller) {
          refreshRef.current = undefined;
        }
      }
    },
    [],
  );

  useEffect(() => {
    mountedRef.current = true;
    void refresh(false);
    const timer = globalThis.setInterval(() => {
      void refresh(true);
    }, BACKGROUND_REFRESH_MS);
    return () => {
      mountedRef.current = false;
      globalThis.clearInterval(timer);
      refreshEpochRef.current += 1;
      refreshRef.current?.abort();
      refreshRef.current = undefined;
      readEpochRef.current += 1;
      readRef.current?.abort();
      readRef.current = undefined;
      mutationControllerRef.current?.abort();
      mutationControllerRef.current = undefined;
    };
  }, [refresh]);

  const runMutation = useCallback(
    async <T>(
      busyAction: string,
      mutation: (signal: AbortSignal) => Promise<GatewayResult<T>>,
      conflictPath?: VirtualPath,
    ): Promise<GatewayResult<T>> => {
      if (stateRef.current.status?.resetting) {
        throw new ShowcaseError(
          "workspace_resetting",
          "The workspace is resetting. Please wait before making changes.",
          503,
        );
      }
      if (mutationRef.current) {
        throw new ShowcaseError(
          "operation_in_progress",
          "Another operation is still running.",
          409,
        );
      }
      mutationRef.current = true;
      const controller = new AbortController();
      mutationControllerRef.current = controller;
      refreshEpochRef.current += 1;
      refreshRef.current?.abort();
      dispatchIfMounted({ type: "busy_changed", busyAction });
      try {
        const result = await mutation(controller.signal);
        if (!mountedRef.current || controller.signal.aborted) {
          return result;
        }
        dispatchIfMounted({
          type: "activity_appended",
          activity: result.activity,
        });
        await refresh(false);
        return result;
      } catch (error) {
        if (isRevisionConflict(error) && conflictPath) {
          dispatchIfMounted({
            type: "revision_conflict",
            path: conflictPath,
            message: error.message,
          });
        }
        dispatchIfMounted({ type: "error_set", error: error as Error });
        throw error;
      } finally {
        mutationRef.current = false;
        if (mutationControllerRef.current === controller) {
          mutationControllerRef.current = undefined;
        }
        dispatchIfMounted({ type: "busy_changed", busyAction: undefined });
      }
    },
    [dispatchIfMounted, refresh],
  );

  const runOperation = useCallback(
    <T>(operation: PublicOperation): Promise<GatewayResult<T>> =>
      runMutation(
        operation.kind,
        (signal) => apiRef.current.operation<T>(operation, signal),
        "path" in operation ? (operation.path as VirtualPath) : undefined,
      ),
    [runMutation],
  );

  const loadEntry = useCallback(
    async (entry: TreeEntry, force = false): Promise<void> => {
      readEpochRef.current += 1;
      readRef.current?.abort();
      const controller = new AbortController();
      readRef.current = controller;
      const epoch = readEpochRef.current;
      const current = () =>
        mountedRef.current &&
        !controller.signal.aborted &&
        epoch === readEpochRef.current;
      dispatchIfMounted({ type: "selected", entry });
      if (entry.node.kind !== "file") {
        readRef.current = undefined;
        return;
      }
      try {
        const result = await apiRef.current.operation<unknown>(
          {
            kind: "read_file",
            path: entry.path,
          },
          controller.signal,
        );
        if (!current()) return;
        let text: string;
        try {
          text = decodeText(result.data);
        } catch (error) {
          if (
            current() &&
            error instanceof ShowcaseError &&
            error.status === 422
          ) {
            dispatch({
              type: "editor_binary",
              path: entry.path,
              revision: entry.node.revision,
              force,
            });
            dispatch({ type: "activity_appended", activity: result.activity });
            return;
          }
          throw error;
        }
        if (!current()) return;
        dispatch({
          type: "editor_loaded",
          path: entry.path,
          text,
          revision: entry.node.revision,
          force,
        });
        dispatch({ type: "activity_appended", activity: result.activity });
      } catch (error) {
        if (
          current() &&
          !(error instanceof DOMException && error.name === "AbortError")
        ) {
          dispatch({ type: "error_set", error: error as Error });
        }
      } finally {
        if (readRef.current === controller) {
          readRef.current = undefined;
        }
      }
    },
    [dispatchIfMounted],
  );

  const selectEntry = useCallback(
    (entry: TreeEntry): Promise<void> => loadEntry(entry),
    [loadEntry],
  );

  const setEditorText = useCallback(
    (text: string) => dispatchIfMounted({ type: "editor_changed", text }),
    [dispatchIfMounted],
  );

  const save = useCallback(async (): Promise<void> => {
    const editor = stateRef.current.editor;
    if (!editor.path) {
      throw new ShowcaseError(
        "invalid_request",
        "Select a file before saving.",
        400,
      );
    }
    const result = await runOperation<Node>({
      kind: "write_file",
      path: editor.path,
      text: editor.text,
      ...(editor.revision === undefined
        ? {}
        : { expectedRevision: editor.revision }),
    });
    dispatchIfMounted({
      type: "editor_saved",
      path: editor.path,
      text: editor.text,
      revision: result.data.revision,
    });
  }, [dispatchIfMounted, runOperation]);

  const upload = useCallback(
    async (path: VirtualPath, file: File): Promise<void> => {
      await runMutation("upload", (signal) =>
        apiRef.current.upload(path, file, signal),
      );
    },
    [runMutation],
  );

  const download = useCallback(
    async (path: VirtualPath): Promise<void> => {
      const result = await apiRef.current.download(path);
      dispatchIfMounted({
        type: "activity_appended",
        activity: result.activity,
      });
    },
    [dispatchIfMounted],
  );

  return useMemo(
    () => ({
      state,
      refresh,
      runOperation,
      runMutation,
      selectEntry,
      setEditorText,
      save,
      upload,
      download,
      clearActivities: () => dispatchIfMounted({ type: "activities_cleared" }),
      setDialog: (name: string, open: boolean) =>
        dispatchIfMounted({ type: "dialog_changed", name, open }),
      clearRevisionConflict: () =>
        dispatchIfMounted({ type: "revision_conflict_cleared" }),
      reloadServerVersion: async () => {
        const path = stateRef.current.revisionConflict?.path;
        const entries = await refresh(false);
        const entry = entries.find((candidate) => candidate.path === path);
        if (entry) await loadEntry(entry, true);
      },
    }),
    [
      dispatchIfMounted,
      download,
      refresh,
      loadEntry,
      runOperation,
      runMutation,
      save,
      selectEntry,
      setEditorText,
      state,
      upload,
    ],
  );
}

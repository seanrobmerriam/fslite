import { useCallback, useRef, useState, type KeyboardEvent } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import type { Change, TrashEntry, TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";
import { useShowcase } from "../../lib/browser/use-showcase";
import { ActionDialog } from "./ActionDialog";
import { CreateDialog } from "./CreateDialog";
import { DeleteDialog } from "./DeleteDialog";
import { FileEditor } from "./FileEditor";
import { FileTree, type FileTreeAction } from "./FileTree";
import { MoveCopyDialog } from "./MoveCopyDialog";
import { ToastRegion } from "./ToastRegion";
import { Toolbar } from "./Toolbar";
import { UploadDialog } from "./UploadDialog";
import { WorkspaceStatus } from "./WorkspaceStatus";
import { SearchPanel } from "./SearchPanel";
import { TrashPanel } from "./TrashPanel";
import { ChangesPanel } from "./ChangesPanel";
import { ApiActivity } from "./ApiActivity";

function directoryFor(
  path: VirtualPath | undefined,
  kind: string | undefined,
): VirtualPath {
  if (!path || kind === "directory") return path ?? ("/" as VirtualPath);
  const boundary = path.lastIndexOf("/");
  return (boundary === 0 ? "/" : path.slice(0, boundary)) as VirtualPath;
}

function pathAffectsDraft(
  path: VirtualPath,
  draftPath: VirtualPath | undefined,
): boolean {
  return Boolean(
    draftPath && (draftPath === path || draftPath.startsWith(`${path}/`)),
  );
}

function operationAffectsDraft(
  operation: PublicOperation,
  draftPath: VirtualPath | undefined,
): boolean {
  if (!draftPath) return false;
  switch (operation.kind) {
    case "write_file":
      return operation.path === draftPath;
    case "copy":
      return pathAffectsDraft(operation.to, draftPath);
    case "move":
      return (
        pathAffectsDraft(operation.from, draftPath) ||
        pathAffectsDraft(operation.to, draftPath)
      );
    case "trash":
    case "remove":
      return pathAffectsDraft(operation.path, draftPath);
    default:
      return false;
  }
}

type DialogState =
  | {
      kind: "create";
      item: "file" | "folder";
      directory: VirtualPath;
      returnFocusTarget?: HTMLElement | null;
    }
  | {
      kind: "upload";
      directory: VirtualPath;
      returnFocusTarget?: HTMLElement | null;
    }
  | {
      kind: "move-copy";
      entry: TreeEntry;
      mode: "rename" | "move" | "copy";
      returnFocusTarget?: HTMLElement | null;
    }
  | {
      kind: "delete";
      entry: TreeEntry;
      initialMode: "trash" | "remove";
      returnFocusTarget?: HTMLElement | null;
    };

type DraftGuardState =
  | { kind: "operation"; operation: PublicOperation; dialog: DialogState }
  | { kind: "upload"; path: VirtualPath; file: File; dialog: DialogState };

/** The only hydrated explorer island; all browser API calls remain behind useShowcase. */
export function ShowcaseExplorer() {
  const {
    state,
    download,
    refresh,
    reloadServerVersion: reloadServerVersion,
    runOperation,
    runReadOperation,
    save,
    selectEntry,
    setEditorText,
    upload,
    clearActivities,
  } = useShowcase();
  const explorerRef = useRef<HTMLDivElement>(null);
  const [dialog, setDialog] = useState<DialogState>();
  const [draftGuard, setDraftGuard] = useState<DraftGuardState>();
  const [activeTab, setActiveTab] = useState<
    "explorer" | "search" | "trash" | "changes"
  >("explorer");
  const resetting = state.status?.resetting ?? false;
  const busy = Boolean(state.busyAction);
  const availability =
    state.availability ?? (state.status ? "ready" : "checking");
  const workspaceUnavailable = availability === "unavailable";
  const mutationDisabled =
    resetting || busy || workspaceUnavailable || availability === "checking";
  const dialogBusy =
    busy || workspaceUnavailable || availability === "checking";
  const selectedDirectory = directoryFor(
    state.selectedPath,
    state.selectedNode?.kind,
  );
  const editorNode =
    state.selectedNode?.kind === "file" &&
    state.editor.path === state.selectedPath
      ? state.selectedNode
      : undefined;

  const closeDialog = useCallback(() => setDialog(undefined), []);
  const openCreate = useCallback(
    (item: "file" | "folder") => {
      if (!mutationDisabled)
        setDialog({ kind: "create", item, directory: selectedDirectory });
    },
    [mutationDisabled, selectedDirectory],
  );
  const openUpload = useCallback(() => {
    if (!mutationDisabled)
      setDialog({ kind: "upload", directory: selectedDirectory });
  }, [mutationDisabled, selectedDirectory]);
  const completeOperation = useCallback(
    async (operation: PublicOperation, source: DialogState) => {
      if (
        state.editor.dirty &&
        operationAffectsDraft(operation, state.editor.path)
      ) {
        setDialog(undefined);
        setDraftGuard({ kind: "operation", operation, dialog: source });
        return;
      }
      await runOperation(operation);
      closeDialog();
    },
    [closeDialog, runOperation, state.editor.dirty, state.editor.path],
  );
  const completeUpload = useCallback(
    async (path: VirtualPath, file: File, source: DialogState) => {
      if (
        state.editor.dirty &&
        operationAffectsDraft(
          { kind: "write_file", path, text: "" },
          state.editor.path,
        )
      ) {
        setDialog(undefined);
        setDraftGuard({ kind: "upload", path, file, dialog: source });
        return;
      }
      await upload(path, file);
      closeDialog();
    },
    [closeDialog, state.editor.dirty, state.editor.path, upload],
  );
  const openNodeAction = useCallback(
    (
      entry: TreeEntry,
      action: FileTreeAction,
      returnFocusTarget: HTMLButtonElement | null,
    ) => {
      if (mutationDisabled) return;
      if (action === "download") {
        void download(entry.path);
        return;
      }
      const next: DialogState =
        action === "rename" || action === "move" || action === "copy"
          ? { kind: "move-copy", entry, mode: action, returnFocusTarget }
          : {
              kind: "delete",
              entry,
              initialMode: action === "remove" ? "remove" : "trash",
              returnFocusTarget,
            };
      setDialog(next);
    },
    [download, mutationDisabled],
  );

  const continueDraftGuard = useCallback(async () => {
    if (!draftGuard) return;
    try {
      if (draftGuard.kind === "operation") {
        await runOperation(draftGuard.operation);
      } else {
        await upload(draftGuard.path, draftGuard.file);
      }
      setDraftGuard(undefined);
    } catch {
      // The shared mutation controller already publishes the safe error state.
    }
  }, [draftGuard, runOperation, upload]);

  const copyUnsavedText = useCallback(async () => {
    const text = state.editor.text;
    if (!text) return;
    try {
      await globalThis.navigator?.clipboard?.writeText(text);
    } catch {
      // The editor remains intact; copying is convenience, never the only recovery path.
    }
  }, [state.editor.text]);

  const reloadCurrentServerVersion = useCallback(() => {
    void reloadServerVersion?.();
  }, [reloadServerVersion]);

  const modalOpen = Boolean(dialog || draftGuard);
  const tabs = [
    ["explorer", "Explorer"],
    ["search", "Search"],
    ["trash", "Trash"],
    ["changes", "Changes"],
  ] as const;
  const selectSearchPath = useCallback(
    (path: VirtualPath) => {
      const entry = state.tree.find((candidate) => candidate.path === path);
      if (entry) void selectEntry(entry);
      setActiveTab("explorer");
    },
    [selectEntry, state.tree],
  );
  const search = useCallback(
    async (operation: PublicOperation) =>
      (await runReadOperation<{ items: unknown[] }>(operation)).data,
    [runReadOperation],
  );
  const listTrash = useCallback(
    async () =>
      (
        await runReadOperation<{ items: TrashEntry[] }>({
          kind: "list_trash",
        })
      ).data,
    [runReadOperation],
  );
  const loadChanges = useCallback(
    async (after?: string) =>
      (
        await runReadOperation<{
          items: Change[];
          next_cursor: string | null;
        }>({ kind: "changes", ...(after ? { after } : {}) })
      ).data,
    [runReadOperation],
  );
  const moveTab = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    let next: number;
    if (event.key === "ArrowRight" || event.key === "ArrowDown")
      next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft" || event.key === "ArrowUp")
      next = (index + tabs.length - 1) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else if (event.key === "Enter" || event.key === " ") {
      setActiveTab(tabs[index][0]);
      return;
    } else return;
    event.preventDefault();
    const tab = tabs[next];
    if (!tab) return;
    setActiveTab(tab[0]);
    document.getElementById(`${tab[0]}-tab`)?.focus();
  };

  return (
    <>
      <div
        ref={explorerRef}
        className="showcase-explorer"
        role="region"
        tabIndex={-1}
        aria-label="Filesystem explorer"
        aria-busy={busy}
        aria-hidden={modalOpen}
        inert={modalOpen}
      >
        <WorkspaceStatus status={state.status} availability={availability} />
        <ToastRegion error={state.error} resetting={resetting} />
        {resetting ? (
          <div
            className="workspace-resetting-overlay"
            role="status"
            aria-label="Workspace reset in progress"
          >
            Reset in progress — editing remains visible, but server changes are
            temporarily disabled.
          </div>
        ) : null}
        <div
          role="tablist"
          aria-label="Explorer views"
          className="explorer-tabs"
        >
          {tabs.map(([id, label], index) => (
            <button
              key={id}
              id={`${id}-tab`}
              role="tab"
              type="button"
              tabIndex={activeTab === id ? 0 : -1}
              aria-selected={activeTab === id}
              aria-controls={`${id}-panel`}
              onClick={() => setActiveTab(id)}
              onKeyDown={(event) => moveTab(event, index)}
            >
              {label}
            </button>
          ))}
        </div>
        {activeTab === "explorer" ? (
          <div
            role="tabpanel"
            id="explorer-panel"
            aria-labelledby="explorer-tab"
          >
            <Toolbar
              actionsDisabled={mutationDisabled}
              refreshDisabled={busy || resetting || availability === "checking"}
              refreshLabel={
                workspaceUnavailable ? "Retry connection" : "Refresh files"
              }
              onRefresh={() => void refresh()}
              onNewFile={() => openCreate("file")}
              onNewFolder={() => openCreate("folder")}
              onUpload={openUpload}
            />
            {state.revisionConflict ? (
              <section
                className="revision-conflict"
                aria-label="Revision conflict"
                role="alert"
              >
                <div>
                  <strong>This file changed on the shared workspace.</strong>
                  <p>{state.revisionConflict.message}</p>
                </div>
                <div className="editor-actions">
                  <button
                    type="button"
                    className="button button--quiet"
                    onClick={() => void copyUnsavedText()}
                  >
                    Copy my unsaved text
                  </button>
                  <button
                    type="button"
                    className="button button--accent"
                    disabled={mutationDisabled}
                    onClick={reloadCurrentServerVersion}
                  >
                    Reload server version
                  </button>
                </div>
              </section>
            ) : null}
            <div className="explorer-workbench">
              <FileTree
                entries={state.tree}
                selectedPath={state.selectedPath}
                disabled={mutationDisabled}
                onSelect={(entry) => void selectEntry(entry)}
                onAction={openNodeAction}
              />
              <FileEditor
                node={editorNode}
                path={editorNode ? state.editor.path : undefined}
                text={state.editor.text}
                dirty={state.editor.dirty}
                binary={state.editor.binary}
                busy={busy}
                resetting={resetting}
                unavailable={
                  workspaceUnavailable || availability === "checking"
                }
                onChange={setEditorText}
                onSave={save}
                onDownload={download}
              />
            </div>
          </div>
        ) : null}
        {activeTab === "search" ? (
          <div role="tabpanel" id="search-panel" aria-labelledby="search-tab">
            <SearchPanel
              busy={busy || resetting}
              unavailable={workspaceUnavailable || availability === "checking"}
              entries={state.tree}
              onSearch={search}
              onSelectPath={selectSearchPath}
            />
          </div>
        ) : null}
        {activeTab === "trash" ? (
          <div role="tabpanel" id="trash-panel" aria-labelledby="trash-tab">
            <TrashPanel
              busy={busy || resetting}
              unavailable={workspaceUnavailable || availability === "checking"}
              onList={listTrash}
              onOperation={runOperation}
            />
          </div>
        ) : null}
        {activeTab === "changes" ? (
          <div role="tabpanel" id="changes-panel" aria-labelledby="changes-tab">
            <ChangesPanel
              generation={state.status?.generation}
              unavailable={workspaceUnavailable || availability === "checking"}
              onLoad={loadChanges}
            />
          </div>
        ) : null}
        <ApiActivity activities={state.activities} onClear={clearActivities} />
      </div>
      {dialog?.kind === "create" ? (
        <CreateDialog
          directory={dialog.directory}
          kind={dialog.item}
          busy={dialogBusy}
          returnFocusTarget={dialog.returnFocusTarget}
          fallbackFocusTarget={explorerRef.current}
          onCreate={(operation) => completeOperation(operation, dialog)}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "upload" ? (
        <UploadDialog
          directory={dialog.directory}
          busy={dialogBusy}
          returnFocusTarget={dialog.returnFocusTarget}
          fallbackFocusTarget={explorerRef.current}
          onUpload={(path, file) => completeUpload(path, file, dialog)}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "move-copy" ? (
        <MoveCopyDialog
          entry={dialog.entry}
          mode={dialog.mode}
          busy={dialogBusy}
          returnFocusTarget={dialog.returnFocusTarget}
          fallbackFocusTarget={explorerRef.current}
          onSubmit={(operation) => completeOperation(operation, dialog)}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "delete" ? (
        <DeleteDialog
          entry={dialog.entry}
          initialMode={dialog.initialMode}
          busy={dialogBusy}
          returnFocusTarget={dialog.returnFocusTarget}
          fallbackFocusTarget={explorerRef.current}
          onSubmit={(operation) => completeOperation(operation, dialog)}
          onClose={closeDialog}
        />
      ) : null}
      {draftGuard ? (
        <ActionDialog
          title="Unsaved changes"
          description="This action changes the server item currently open in the editor. Your local draft will remain here, but it will no longer match the server item."
          onClose={() => setDraftGuard(undefined)}
          returnFocusTarget={draftGuard.dialog.returnFocusTarget}
          fallbackFocusTarget={explorerRef.current}
          busy={dialogBusy}
          closeable={!dialogBusy}
        >
          <div className="action-dialog__actions action-dialog__actions--guard">
            <button
              type="button"
              className="button button--quiet"
              disabled={dialogBusy}
              onClick={() => setDraftGuard(undefined)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="button button--danger"
              disabled={dialogBusy}
              onClick={() => void continueDraftGuard()}
            >
              Continue without saving
            </button>
          </div>
        </ActionDialog>
      ) : null}
    </>
  );
}

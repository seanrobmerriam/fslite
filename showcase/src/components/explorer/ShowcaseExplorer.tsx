import { useCallback, useState } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import type { TreeEntry } from "../../lib/shared/contracts";
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

function directoryFor(
  path: VirtualPath | undefined,
  kind: string | undefined,
): VirtualPath {
  if (!path || kind === "directory") return path ?? ("/" as VirtualPath);
  const boundary = path.lastIndexOf("/");
  return (boundary === 0 ? "/" : path.slice(0, boundary)) as VirtualPath;
}

function actionAffectsDraft(
  entry: TreeEntry,
  action: FileTreeAction,
  draftPath: VirtualPath | undefined,
): boolean {
  if (!draftPath || action === "copy") return false;
  return draftPath === entry.path || draftPath.startsWith(`${entry.path}/`);
}

type DialogState =
  | { kind: "create"; item: "file" | "folder"; directory: VirtualPath }
  | { kind: "upload"; directory: VirtualPath }
  | { kind: "move-copy"; entry: TreeEntry; mode: "rename" | "move" | "copy" }
  | { kind: "delete"; entry: TreeEntry; initialMode: "trash" | "remove" };

/** The only hydrated explorer island; all browser API calls remain behind useShowcase. */
export function ShowcaseExplorer() {
  const {
    state,
    download,
    refresh,
    reloadServerVersion: reloadServerVersion,
    runOperation,
    save,
    selectEntry,
    setEditorText,
    upload,
  } = useShowcase();
  const [dialog, setDialog] = useState<DialogState>();
  const [draftGuard, setDraftGuard] = useState<DialogState>();
  const resetting = state.status?.resetting ?? false;
  const busy = Boolean(state.busyAction);
  const mutationDisabled = resetting || busy;
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
    async (operation: PublicOperation) => {
      await runOperation(operation);
      closeDialog();
    },
    [closeDialog, runOperation],
  );
  const completeUpload = useCallback(
    async (path: VirtualPath, file: File) => {
      await upload(path, file);
      closeDialog();
    },
    [closeDialog, upload],
  );
  const openNodeAction = useCallback(
    (entry: TreeEntry, action: FileTreeAction) => {
      if (mutationDisabled) return;
      if (action === "download") {
        void download(entry.path);
        return;
      }
      const next: DialogState =
        action === "rename" || action === "move" || action === "copy"
          ? { kind: "move-copy", entry, mode: action }
          : {
              kind: "delete",
              entry,
              initialMode: action === "remove" ? "remove" : "trash",
            };
      if (
        state.editor.dirty &&
        actionAffectsDraft(entry, action, state.editor.path)
      ) {
        setDraftGuard(next);
      } else {
        setDialog(next);
      }
    },
    [download, mutationDisabled, state.editor.dirty, state.editor.path],
  );

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

  return (
    <>
      <div
        className="showcase-explorer"
        aria-busy={busy}
        aria-hidden={modalOpen}
        inert={modalOpen}
      >
        <WorkspaceStatus status={state.status} />
        <ToastRegion error={state.error} resetting={resetting} />
        <Toolbar
          disabled={mutationDisabled}
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
                disabled={busy || resetting}
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
            disabled={busy}
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
            onChange={setEditorText}
            onSave={save}
            onDownload={download}
          />
        </div>
      </div>
      {dialog?.kind === "create" ? (
        <CreateDialog
          directory={dialog.directory}
          kind={dialog.item}
          busy={busy}
          onCreate={completeOperation}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "upload" ? (
        <UploadDialog
          directory={dialog.directory}
          busy={busy}
          onUpload={completeUpload}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "move-copy" ? (
        <MoveCopyDialog
          entry={dialog.entry}
          mode={dialog.mode}
          busy={busy}
          onSubmit={completeOperation}
          onClose={closeDialog}
        />
      ) : null}
      {dialog?.kind === "delete" ? (
        <DeleteDialog
          entry={dialog.entry}
          initialMode={dialog.initialMode}
          busy={busy}
          onSubmit={completeOperation}
          onClose={closeDialog}
        />
      ) : null}
      {draftGuard ? (
        <ActionDialog
          title="Unsaved changes"
          description="This action changes the server item currently open in the editor. Your local draft will remain here, but it will no longer match the server item."
          onClose={() => setDraftGuard(undefined)}
        >
          <div className="action-dialog__actions action-dialog__actions--guard">
            <button
              type="button"
              className="button button--quiet"
              onClick={() => setDraftGuard(undefined)}
            >
              Cancel
            </button>
            <button
              type="button"
              className="button button--danger"
              onClick={() => {
                setDialog(draftGuard);
                setDraftGuard(undefined);
              }}
            >
              Continue without saving
            </button>
          </div>
        </ActionDialog>
      ) : null}
    </>
  );
}

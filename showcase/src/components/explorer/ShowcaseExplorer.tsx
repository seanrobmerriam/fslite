import { useCallback } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import { validateVirtualPath, type VirtualPath } from "../../lib/shared/path";
import { useShowcase } from "../../lib/browser/use-showcase";
import { FileEditor } from "./FileEditor";
import { FileTree } from "./FileTree";
import { ToastRegion } from "./ToastRegion";
import { Toolbar } from "./Toolbar";
import { WorkspaceStatus } from "./WorkspaceStatus";

function directoryFor(
  path: VirtualPath | undefined,
  kind: string | undefined,
): string {
  if (!path || kind === "directory") return path ?? "/";
  const boundary = path.lastIndexOf("/");
  return boundary === 0 ? "/" : path.slice(0, boundary);
}

function promptForPath(
  kind: "file" | "folder",
  base: string,
): VirtualPath | undefined {
  const name = globalThis.prompt(
    `Name for the new ${kind}`,
    kind === "file" ? "note.txt" : "notes",
  );
  if (!name?.trim() || name.includes("/")) return undefined;
  return validateVirtualPath(`${base === "/" ? "" : base}/${name.trim()}`);
}

/** The only hydrated explorer island; all browser API calls remain behind useShowcase. */
export function ShowcaseExplorer() {
  const showcase = useShowcase();
  const { state } = showcase;
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

  const create = useCallback(
    (kind: "file" | "folder") => {
      if (mutationDisabled) return;
      let path: VirtualPath | undefined;
      try {
        path = promptForPath(kind, selectedDirectory);
      } catch {
        return;
      }
      if (!path) return;
      const operation: PublicOperation =
        kind === "file"
          ? { kind: "write_file", path, text: "" }
          : { kind: "mkdir", path, parents: false };
      void showcase.runOperation(operation);
    },
    [mutationDisabled, selectedDirectory, showcase],
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

  const reloadServerVersion = useCallback(() => {
    void showcase.reloadServerVersion?.();
  }, [showcase]);

  return (
    <div className="showcase-explorer" aria-busy={busy}>
      <WorkspaceStatus status={state.status} />
      <ToastRegion error={state.error} resetting={resetting} />
      <Toolbar
        disabled={mutationDisabled}
        onRefresh={() => void showcase.refresh()}
        onNewFile={() => create("file")}
        onNewFolder={() => create("folder")}
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
              onClick={reloadServerVersion}
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
          onSelect={(entry) => void showcase.selectEntry(entry)}
        />
        <FileEditor
          node={editorNode}
          path={editorNode ? state.editor.path : undefined}
          text={state.editor.text}
          dirty={state.editor.dirty}
          binary={state.editor.binary}
          busy={busy}
          resetting={resetting}
          onChange={showcase.setEditorText}
          onSave={showcase.save}
          onDownload={showcase.download}
        />
      </div>
    </div>
  );
}

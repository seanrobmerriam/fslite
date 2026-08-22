import { type KeyboardEvent } from "react";

import type { Node } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";

interface FileEditorProps {
  node: Node | undefined;
  path: VirtualPath | undefined;
  text: string;
  dirty: boolean;
  binary?: boolean;
  busy: boolean;
  resetting: boolean;
  onChange(text: string): void;
  onSave(): Promise<void> | void;
  onDownload(path: VirtualPath): Promise<void> | void;
}

export function FileEditor({
  node,
  path,
  text,
  dirty,
  binary = false,
  busy,
  resetting,
  onChange,
  onSave,
  onDownload,
}: FileEditorProps) {
  const mutationDisabled = busy || resetting;
  const canSave = Boolean(path && !binary && dirty && !mutationDisabled);
  const saveWithKeyboard = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (
      (event.ctrlKey || event.metaKey) &&
      event.key.toLowerCase() === "s" &&
      canSave
    ) {
      event.preventDefault();
      void onSave();
    }
  };

  if (!node || !path) {
    return (
      <section className="file-editor empty-editor" aria-label="File editor">
        <p>Select a text file to read and edit its contents.</p>
      </section>
    );
  }

  return (
    <section className="file-editor" aria-label="File editor">
      <header className="editor-header">
        <div>
          <p className="editor-path">{path}</p>
          <p className="editor-meta">
            revision {node.revision} · {node.logical_size} bytes
          </p>
        </div>
        <div className="editor-actions">
          {dirty ? <span className="dirty-badge">Unsaved changes</span> : null}
          <button
            type="button"
            className="button button--quiet"
            onClick={() => void onDownload(path)}
            title="Download this file"
          >
            Download file
          </button>
          {!binary ? (
            <button
              type="button"
              className="button button--accent"
              disabled={!canSave}
              onClick={() => void onSave()}
            >
              Save file
            </button>
          ) : null}
        </div>
      </header>
      {binary ? (
        <div className="binary-notice" role="status">
          <strong>Binary file</strong>
          <p>
            This content is not valid UTF-8 text and has not been decoded.
            Download it to inspect safely.
          </p>
        </div>
      ) : (
        <label className="editor-control">
          <span className="sr-only">File contents</span>
          <textarea
            aria-label="File contents"
            value={text}
            disabled={mutationDisabled}
            spellCheck={false}
            onChange={(event) => onChange(event.target.value)}
            onKeyDown={saveWithKeyboard}
          />
        </label>
      )}
    </section>
  );
}

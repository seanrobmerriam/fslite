import { useState, type SubmitEventHandler } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import { validateVirtualPath, type VirtualPath } from "../../lib/shared/path";
import { ActionDialog } from "./ActionDialog";

interface CreateDialogProps {
  directory: VirtualPath;
  kind: "file" | "folder";
  onCreate(
    operation: Extract<PublicOperation, { kind: "write_file" | "mkdir" }>,
  ): Promise<void> | void;
  onClose(): void;
  busy?: boolean;
  returnFocusTarget?: HTMLElement | null;
  fallbackFocusTarget?: HTMLElement | null;
}

function childPath(directory: VirtualPath, name: string): VirtualPath {
  const segment = name.trim();
  if (
    !segment ||
    segment.includes("/") ||
    segment === "." ||
    segment === ".."
  ) {
    throw new Error("Name must be a single path segment.");
  }
  return validateVirtualPath(
    `${directory === "/" ? "" : directory}/${segment}`,
  );
}

export function CreateDialog({
  directory,
  kind,
  onCreate,
  onClose,
  busy = false,
  returnFocusTarget,
  fallbackFocusTarget,
}: CreateDialogProps) {
  const [name, setName] = useState(kind === "file" ? "note.txt" : "notes");
  const [error, setError] = useState<string>();
  const action = kind === "file" ? "Create file" : "Create folder";

  const submit: SubmitEventHandler<HTMLFormElement> = async (event) => {
    event.preventDefault();
    try {
      const path = childPath(directory, name);
      setError(undefined);
      await onCreate(
        kind === "file"
          ? { kind: "write_file", path, text: "" }
          : { kind: "mkdir", path, parents: false },
      );
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Unable to create this item.",
      );
    }
  };

  return (
    <ActionDialog
      title={action}
      description={`Create a ${kind} in ${directory}.`}
      onClose={onClose}
      closeable={!busy}
      busy={busy}
      returnFocusTarget={returnFocusTarget}
      fallbackFocusTarget={fallbackFocusTarget}
    >
      <form className="action-dialog__form" onSubmit={submit}>
        <label>
          <span>Name</span>
          <input
            name="name"
            value={name}
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {error ? (
          <p className="dialog-error" role="alert">
            {error}
          </p>
        ) : null}
        <div className="action-dialog__actions">
          <button
            type="button"
            className="button button--quiet"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="button button--accent"
            disabled={busy}
          >
            {action}
          </button>
        </div>
      </form>
    </ActionDialog>
  );
}

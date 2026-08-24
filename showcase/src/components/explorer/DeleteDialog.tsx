import { useState, type SubmitEventHandler } from "react";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { PublicOperation } from "../../lib/server/schemas";
import { ActionDialog } from "./ActionDialog";

interface DeleteDialogProps {
  entry: TreeEntry;
  onSubmit(
    operation: Extract<PublicOperation, { kind: "trash" | "remove" }>,
  ): Promise<void> | void;
  onClose(): void;
  busy?: boolean;
  initialMode?: "trash" | "remove";
  returnFocusTarget?: HTMLElement | null;
  fallbackFocusTarget?: HTMLElement | null;
}

export function DeleteDialog({
  entry,
  onSubmit,
  onClose,
  busy = false,
  initialMode = "trash",
  returnFocusTarget,
  fallbackFocusTarget,
}: DeleteDialogProps) {
  const [mode, setMode] = useState<"trash" | "remove">(initialMode);
  const [confirmation, setConfirmation] = useState("");
  const [error, setError] = useState<string>();
  const permanentlyRemoving = mode === "remove";
  const action = permanentlyRemoving ? "Delete permanently" : "Move to trash";

  const submit: SubmitEventHandler<HTMLFormElement> = async (event) => {
    event.preventDefault();
    if (permanentlyRemoving && confirmation !== entry.path) return;
    try {
      setError(undefined);
      await onSubmit(
        permanentlyRemoving
          ? {
              kind: "remove",
              path: entry.path,
              recursive: entry.node.kind === "directory",
              confirmedPath: confirmation,
              expectedRevision: entry.node.revision,
            }
          : {
              kind: "trash",
              path: entry.path,
              expectedRevision: entry.node.revision,
            },
      );
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Unable to delete this item.",
      );
    }
  };

  return (
    <ActionDialog
      title="Delete item"
      description={`Choose how to remove ${entry.path}.`}
      onClose={onClose}
      closeable={!busy}
      busy={busy}
      returnFocusTarget={returnFocusTarget}
      fallbackFocusTarget={fallbackFocusTarget}
    >
      <form className="action-dialog__form" onSubmit={submit}>
        <fieldset disabled={busy}>
          <legend>Removal method</legend>
          <label>
            <input
              type="radio"
              name="removal"
              checked={mode === "trash"}
              onChange={() => setMode("trash")}
            />{" "}
            Move to trash
          </label>
          <label className="delete-choice">
            <input
              type="radio"
              name="removal"
              checked={permanentlyRemoving}
              onChange={() => setMode("remove")}
            />{" "}
            Delete permanently
          </label>
        </fieldset>
        {permanentlyRemoving ? (
          <label>
            <span>Confirm full path</span>
            <input
              aria-label="Confirm full path"
              value={confirmation}
              disabled={busy}
              onChange={(event) => setConfirmation(event.target.value)}
            />
            <small>Type {entry.path} exactly. This cannot be undone.</small>
          </label>
        ) : (
          <p className="dialog-note">
            The item will remain available in Trash until it is permanently
            purged.
          </p>
        )}
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
            className="button button--danger"
            disabled={
              busy || (permanentlyRemoving && confirmation !== entry.path)
            }
          >
            {action}
          </button>
        </div>
      </form>
    </ActionDialog>
  );
}

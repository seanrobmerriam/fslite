import { useState, type SubmitEventHandler } from "react";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { PublicOperation } from "../../lib/server/schemas";
import { validateVirtualPath, type VirtualPath } from "../../lib/shared/path";
import { ActionDialog } from "./ActionDialog";

type MoveCopyMode = "rename" | "move" | "copy";

interface MoveCopyDialogProps {
  entry: TreeEntry;
  mode: MoveCopyMode;
  onSubmit(
    operation: Extract<PublicOperation, { kind: "move" | "copy" }>,
  ): Promise<void> | void;
  onClose(): void;
  busy?: boolean;
  returnFocusTarget?: HTMLElement | null;
  fallbackFocusTarget?: HTMLElement | null;
}

function parentPath(path: VirtualPath): VirtualPath {
  const boundary = path.lastIndexOf("/");
  return (boundary <= 0 ? "/" : path.slice(0, boundary)) as VirtualPath;
}

function renamedPath(path: VirtualPath, name: string): VirtualPath {
  const segment = name.trim();
  if (
    !segment ||
    segment.includes("/") ||
    segment === "." ||
    segment === ".."
  ) {
    throw new Error("Name must be a single path segment.");
  }
  const directory = parentPath(path);
  return validateVirtualPath(
    `${directory === "/" ? "" : directory}/${segment}`,
  );
}

function destinationPath(value: string, source: VirtualPath): VirtualPath {
  const destination = validateVirtualPath(value.trim());
  if (destination === source || destination.startsWith(`${source}/`)) {
    throw new Error(
      "Destination must not be the item or one of its descendants.",
    );
  }
  return destination;
}

export function MoveCopyDialog({
  entry,
  mode,
  onSubmit,
  onClose,
  busy = false,
  returnFocusTarget,
  fallbackFocusTarget,
}: MoveCopyDialogProps) {
  const [name, setName] = useState(entry.node.name);
  const [destination, setDestination] = useState<string>(entry.path);
  const [error, setError] = useState<string>();
  const action =
    mode === "rename" ? "Rename" : mode === "move" ? "Move" : "Copy";

  const submit: SubmitEventHandler<HTMLFormElement> = async (event) => {
    event.preventDefault();
    try {
      const to =
        mode === "rename"
          ? renamedPath(entry.path, name)
          : destinationPath(destination, entry.path);
      const operation: Extract<PublicOperation, { kind: "move" | "copy" }> =
        mode === "copy"
          ? {
              kind: "copy",
              from: entry.path,
              to,
              recursive: entry.node.kind === "directory",
            }
          : { kind: "move", from: entry.path, to };
      setError(undefined);
      await onSubmit(operation);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : `Unable to ${action.toLowerCase()} this item.`,
      );
    }
  };

  return (
    <ActionDialog
      title={action}
      description={`${action} ${entry.path}.`}
      onClose={onClose}
      closeable={!busy}
      busy={busy}
      returnFocusTarget={returnFocusTarget}
      fallbackFocusTarget={fallbackFocusTarget}
    >
      <form className="action-dialog__form" onSubmit={submit}>
        {mode === "rename" ? (
          <label>
            <span>Name</span>
            <input
              name="name"
              value={name}
              disabled={busy}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
        ) : (
          <label>
            <span>Destination</span>
            <input
              name="destination"
              value={destination}
              disabled={busy}
              onChange={(event) => setDestination(event.target.value)}
            />
          </label>
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

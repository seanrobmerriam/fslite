import { useState, type ChangeEvent, type SubmitEventHandler } from "react";

import { MAX_BROWSER_FILE_BYTES } from "../../lib/browser/api";
import { validateVirtualPath, type VirtualPath } from "../../lib/shared/path";
import { ActionDialog } from "./ActionDialog";

interface UploadDialogProps {
  directory: VirtualPath;
  onUpload(path: VirtualPath, file: File): Promise<void> | void;
  onClose(): void;
  busy?: boolean;
  returnFocusTarget?: HTMLElement | null;
}

function uploadPath(directory: VirtualPath, file: File): VirtualPath {
  if (
    !file.name ||
    file.name.includes("/") ||
    file.name === "." ||
    file.name === ".."
  ) {
    throw new Error("File name must be a single path segment.");
  }
  return validateVirtualPath(
    `${directory === "/" ? "" : directory}/${file.name}`,
  );
}

export function UploadDialog({
  directory,
  onUpload,
  onClose,
  busy = false,
  returnFocusTarget,
}: UploadDialogProps) {
  const [file, setFile] = useState<File>();
  const [error, setError] = useState<string>();
  const selectFile = (event: ChangeEvent<HTMLInputElement>) => {
    setFile(event.target.files?.[0]);
    setError(undefined);
  };
  const submit: SubmitEventHandler<HTMLFormElement> = async (event) => {
    event.preventDefault();
    if (!file) {
      setError("Choose a file to upload.");
      return;
    }
    if (file.size > MAX_BROWSER_FILE_BYTES) {
      setError(`Files must not exceed ${MAX_BROWSER_FILE_BYTES} bytes.`);
      return;
    }
    try {
      setError(undefined);
      await onUpload(uploadPath(directory, file), file);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Unable to upload this file.",
      );
    }
  };

  return (
    <ActionDialog
      title="Upload file"
      description={`Upload one file to ${directory}.`}
      onClose={onClose}
      closeable={!busy}
      busy={busy}
      returnFocusTarget={returnFocusTarget}
    >
      <form className="action-dialog__form" onSubmit={submit}>
        <label>
          <span>File</span>
          <input type="file" disabled={busy} onChange={selectFile} />
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
            disabled={busy || !file}
          >
            Upload file
          </button>
        </div>
      </form>
    </ActionDialog>
  );
}

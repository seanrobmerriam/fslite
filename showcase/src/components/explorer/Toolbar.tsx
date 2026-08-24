interface ToolbarProps {
  actionsDisabled: boolean;
  refreshDisabled: boolean;
  refreshLabel?: string;
  onRefresh(): void;
  onNewFile(): void;
  onNewFolder(): void;
  onUpload(): void;
}

export function Toolbar({
  actionsDisabled,
  refreshDisabled,
  refreshLabel = "Refresh files",
  onRefresh,
  onNewFile,
  onNewFolder,
  onUpload,
}: ToolbarProps) {
  return (
    <div className="explorer-toolbar" role="toolbar" aria-label="File actions">
      <button
        type="button"
        className="button button--accent"
        disabled={actionsDisabled}
        onClick={onNewFile}
        title="Create a text file"
      >
        New file
      </button>
      <button
        type="button"
        className="button button--quiet"
        disabled={actionsDisabled}
        onClick={onNewFolder}
        title="Create a directory"
      >
        New folder
      </button>
      <button
        type="button"
        className="button button--quiet"
        disabled={actionsDisabled}
        onClick={onUpload}
        title="Upload one file"
      >
        Upload
      </button>
      <button
        type="button"
        className={
          refreshLabel === "Retry connection"
            ? "button button--quiet"
            : "icon-button"
        }
        disabled={refreshDisabled}
        onClick={onRefresh}
        aria-label={refreshLabel}
        title={refreshLabel}
      >
        {refreshLabel === "Retry connection" ? refreshLabel : "↻"}
      </button>
    </div>
  );
}

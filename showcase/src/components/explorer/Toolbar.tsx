interface ToolbarProps {
  disabled: boolean;
  onRefresh(): void;
  onNewFile(): void;
  onNewFolder(): void;
}

export function Toolbar({
  disabled,
  onRefresh,
  onNewFile,
  onNewFolder,
}: ToolbarProps) {
  return (
    <div className="explorer-toolbar" role="toolbar" aria-label="File actions">
      <button
        type="button"
        className="button button--accent"
        disabled={disabled}
        onClick={onNewFile}
        title="Create a text file"
      >
        New file
      </button>
      <button
        type="button"
        className="button button--quiet"
        disabled={disabled}
        onClick={onNewFolder}
        title="Create a directory"
      >
        New folder
      </button>
      <button
        type="button"
        className="icon-button"
        disabled={disabled}
        onClick={onRefresh}
        aria-label="Refresh files"
        title="Refresh files"
      >
        ↻
      </button>
    </div>
  );
}

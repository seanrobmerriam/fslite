interface ToastRegionProps {
  error: Error | undefined;
  resetting: boolean;
}

export function ToastRegion({ error, resetting }: ToastRegionProps) {
  if (!error && !resetting) return null;
  return (
    <div className="toast-region" aria-live="polite" aria-atomic="true">
      {resetting ? (
        <p className="toast toast--notice">
          Workspace resetting. Your unsaved editor text stays in this browser.
        </p>
      ) : null}
      {error ? (
        <p className="toast toast--error" role="alert">
          {error.message}
        </p>
      ) : null}
    </div>
  );
}

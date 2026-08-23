import {
  useEffect,
  useId,
  useRef,
  type KeyboardEvent,
  type ReactNode,
} from "react";

interface ActionDialogProps {
  title: string;
  description: string;
  onClose(): void;
  closeable?: boolean;
  busy?: boolean;
  returnFocusTarget?: HTMLElement | null;
  children: ReactNode;
}

function focusableElements(container: HTMLElement): HTMLElement[] {
  return [
    ...container.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
    ),
  ].filter(
    (element) =>
      !element.hasAttribute("hidden") && !element.matches(":disabled"),
  );
}

/** Modal shell with labelled content, a keyboard focus loop, and safe dismissal. */
export function ActionDialog({
  title,
  description,
  onClose,
  closeable = true,
  busy = false,
  returnFocusTarget,
  children,
}: ActionDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(
    returnFocusTarget ??
      (typeof document === "undefined"
        ? null
        : document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null),
  );

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (busy) {
      dialog.focus();
      return;
    }
    focusableElements(dialog)[0]?.focus();
  }, [busy]);

  useEffect(() => {
    return () => returnFocusRef.current?.focus();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && closeable) {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab" || !dialogRef.current) return;
    const elements = focusableElements(dialogRef.current);
    if (elements.length === 0) {
      event.preventDefault();
      dialogRef.current.focus();
      return;
    }
    const first = elements[0];
    const last = elements.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="action-dialog-backdrop">
      <div
        ref={dialogRef}
        className="action-dialog"
        role="dialog"
        tabIndex={-1}
        aria-modal="true"
        aria-busy={busy}
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        onKeyDown={handleKeyDown}
      >
        <header className="action-dialog__header">
          <h2 id={titleId}>{title}</h2>
          <p id={descriptionId}>{description}</p>
        </header>
        {children}
      </div>
    </div>
  );
}

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";

import type { TreeEntry } from "../../lib/shared/contracts";
import type { VirtualPath } from "../../lib/shared/path";

interface FileTreeProps {
  entries: readonly TreeEntry[];
  selectedPath: VirtualPath | undefined;
  disabled?: boolean;
  onSelect(entry: TreeEntry): void;
}

function parentPath(path: VirtualPath): string {
  const index = path.lastIndexOf("/");
  return index <= 0 ? "/" : path.slice(0, index);
}

function hasChildren(entry: TreeEntry, entries: readonly TreeEntry[]): boolean {
  return entries.some((candidate) => parentPath(candidate.path) === entry.path);
}

function isVisible(
  entry: TreeEntry,
  expanded: ReadonlySet<VirtualPath>,
): boolean {
  let parent = parentPath(entry.path);
  while (parent !== "/") {
    if (!expanded.has(parent as VirtualPath)) return false;
    parent = parentPath(parent as VirtualPath);
  }
  return true;
}

/** A compact, keyboard-first tree that keeps DOM and visual hierarchy aligned. */
export function FileTree({
  entries,
  selectedPath,
  disabled = false,
  onSelect,
}: FileTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<VirtualPath>>(
    () => new Set(),
  );
  const [activePath, setActivePath] = useState<VirtualPath | undefined>(
    selectedPath ?? entries[0]?.path,
  );
  const focusRequested = useRef(false);
  const visible = useMemo(
    () => entries.filter((entry) => isVisible(entry, expanded)),
    [entries, expanded],
  );

  useEffect(() => {
    if (
      (!activePath || !entries.some((entry) => entry.path === activePath)) &&
      entries.length > 0
    ) {
      setActivePath(selectedPath ?? entries[0]?.path);
    }
  }, [activePath, entries, selectedPath]);

  useEffect(() => {
    if (focusRequested.current && activePath) {
      document
        .getElementById(`treeitem-${encodeURIComponent(activePath)}`)
        ?.focus();
      focusRequested.current = false;
    }
  }, [activePath, visible]);

  const siblingMetrics = (entry: TreeEntry) => {
    const siblings = entries.filter(
      (candidate) => parentPath(candidate.path) === parentPath(entry.path),
    );
    return {
      setSize: siblings.length,
      position:
        siblings.findIndex((candidate) => candidate.path === entry.path) + 1,
    };
  };

  const focusPath = (path: VirtualPath) => {
    focusRequested.current = true;
    setActivePath(path);
  };

  const toggle = (path: VirtualPath) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const onKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    entry: TreeEntry,
  ) => {
    const index = visible.findIndex(
      (candidate) => candidate.path === entry.path,
    );
    const children = hasChildren(entry, entries);
    const isExpanded = expanded.has(entry.path);
    const focus = (candidate: TreeEntry | undefined) => {
      if (candidate) focusPath(candidate.path);
    };

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focus(visible[index + 1]);
        break;
      case "ArrowUp":
        event.preventDefault();
        focus(visible[index - 1]);
        break;
      case "Home":
        event.preventDefault();
        focus(visible[0]);
        break;
      case "End":
        event.preventDefault();
        focus(visible.at(-1));
        break;
      case "ArrowRight":
        event.preventDefault();
        if (children && !isExpanded) {
          toggle(entry.path);
        } else if (children && isExpanded) {
          focus(visible[index + 1]);
        }
        break;
      case "ArrowLeft": {
        event.preventDefault();
        if (children && isExpanded) {
          toggle(entry.path);
        } else {
          const parent = entries.find(
            (candidate) => candidate.path === parentPath(entry.path),
          );
          focus(parent);
        }
        break;
      }
      case "Enter":
      case " ":
        event.preventDefault();
        if (!disabled) onSelect(entry);
        break;
      default:
        break;
    }
  };

  return (
    <nav className="file-tree-panel" aria-label="Filesystem tree">
      <div className="panel-heading">
        <h2>Files</h2>
        <span>{entries.length} items</span>
      </div>
      <div
        className="file-tree"
        role="tree"
        aria-label="Files"
        aria-disabled={disabled}
      >
        {visible.length === 0 ? (
          <p className="tree-empty">
            No files yet. Make a note or a folder to begin.
          </p>
        ) : (
          visible.map((entry) => {
            const children = hasChildren(entry, entries);
            const metrics = siblingMetrics(entry);
            const itemExpanded = expanded.has(entry.path);
            const selected = selectedPath === entry.path;
            return (
              <button
                key={entry.path}
                id={`treeitem-${encodeURIComponent(entry.path)}`}
                className={`tree-item${selected ? " is-selected" : ""}`}
                type="button"
                role="treeitem"
                tabIndex={activePath === entry.path ? 0 : -1}
                aria-level={entry.depth + 1}
                aria-setsize={metrics.setSize}
                aria-posinset={metrics.position}
                aria-selected={selected}
                {...(children ? { "aria-expanded": itemExpanded } : {})}
                disabled={disabled}
                style={{ "--tree-depth": entry.depth } as CSSProperties}
                onFocus={() => setActivePath(entry.path)}
                onClick={() => onSelect(entry)}
                onKeyDown={(event) => onKeyDown(event, entry)}
              >
                <span className="tree-glyph" aria-hidden="true">
                  {children
                    ? itemExpanded
                      ? "−"
                      : "+"
                    : entry.node.kind === "file"
                      ? "·"
                      : "↗"}
                </span>
                <span>{entry.node.name}</span>
              </button>
            );
          })
        )}
      </div>
    </nav>
  );
}

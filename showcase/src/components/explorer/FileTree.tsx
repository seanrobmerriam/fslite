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

export type FileTreeAction =
  "rename" | "move" | "copy" | "download" | "trash" | "remove";

interface FileTreeProps {
  entries: readonly TreeEntry[];
  selectedPath: VirtualPath | undefined;
  disabled?: boolean;
  onSelect(entry: TreeEntry): void;
  onAction?(entry: TreeEntry, action: FileTreeAction): void;
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
  onAction,
}: FileTreeProps) {
  const [expanded, setExpanded] = useState<ReadonlySet<VirtualPath>>(
    () => new Set(),
  );
  const [activePath, setActivePath] = useState<VirtualPath | undefined>(
    selectedPath ?? entries[0]?.path,
  );
  const focusRequested = useRef(false);
  const treeRef = useRef<HTMLDivElement>(null);
  const menuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [menuPath, setMenuPath] = useState<VirtualPath>();
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

  useEffect(() => {
    if (!menuPath) return;
    const menu = treeRef.current?.querySelector<HTMLElement>("[role=menu]");
    menu?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }, [menuPath]);

  useEffect(() => {
    const closeOnClickAway = (event: MouseEvent) => {
      if (!treeRef.current?.contains(event.target as Node)) {
        setMenuPath(undefined);
      }
    };
    document.addEventListener("mousedown", closeOnClickAway);
    return () => document.removeEventListener("mousedown", closeOnClickAway);
  }, []);

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

  const closeMenu = (restoreFocus = false) => {
    setMenuPath(undefined);
    if (restoreFocus) menuTriggerRef.current?.focus();
  };

  const handleMenuKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenu(true);
      return;
    }

    const items = [
      ...event.currentTarget.querySelectorAll<HTMLButtonElement>(
        '[role="menuitem"]',
      ),
    ];
    if (items.length === 0) return;
    const currentIndex = items.indexOf(
      document.activeElement as HTMLButtonElement,
    );
    const focus = (index: number) => items.at(index)?.focus();

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focus(
          currentIndex < 0 || currentIndex === items.length - 1
            ? 0
            : currentIndex + 1,
        );
        break;
      case "ArrowUp":
        event.preventDefault();
        focus(currentIndex <= 0 ? items.length - 1 : currentIndex - 1);
        break;
      case "Home":
        event.preventDefault();
        focus(0);
        break;
      case "End":
        event.preventDefault();
        focus(items.length - 1);
        break;
      default:
        break;
    }
  };

  const menuItemsFor = (
    entry: TreeEntry,
  ): readonly [string, FileTreeAction][] => {
    const common: [string, FileTreeAction][] = [
      ["Rename", "rename"],
      ["Move", "move"],
      ["Copy", "copy"],
    ];
    if (entry.node.kind === "file") common.push(["Download", "download"]);
    common.push(["Move to trash", "trash"]);
    common.push(["Delete permanently", "remove"]);
    return common;
  };

  return (
    <nav className="file-tree-panel" aria-label="Filesystem tree">
      <div className="panel-heading">
        <h2>Files</h2>
        <span>{entries.length} items</span>
      </div>
      <div
        ref={treeRef}
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
            const menuOpen = menuPath === entry.path;
            const menuId = `tree-actions-${encodeURIComponent(entry.path)}`;
            return (
              <div className="tree-row" key={entry.path}>
                <button
                  id={`treeitem-${encodeURIComponent(entry.path)}`}
                  className="tree-item"
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
                {onAction ? (
                  <div className="tree-menu-wrap">
                    <button
                      ref={menuOpen ? menuTriggerRef : undefined}
                      type="button"
                      className="tree-action-trigger"
                      aria-label={`Actions for ${entry.node.name}`}
                      aria-haspopup="menu"
                      aria-expanded={menuOpen}
                      aria-controls={menuId}
                      disabled={disabled}
                      onClick={() => {
                        menuTriggerRef.current =
                          document.activeElement as HTMLButtonElement;
                        setMenuPath(menuOpen ? undefined : entry.path);
                      }}
                    >
                      <span aria-hidden="true">⋯</span>
                    </button>
                    {menuOpen ? (
                      <div
                        id={menuId}
                        className="tree-action-menu"
                        role="menu"
                        aria-label={`Actions for ${entry.node.name}`}
                        onKeyDown={handleMenuKeyDown}
                      >
                        {menuItemsFor(entry).map(([label, action]) => (
                          <button
                            key={label}
                            type="button"
                            role="menuitem"
                            onClick={() => {
                              closeMenu();
                              onAction(entry, action);
                            }}
                          >
                            {label}
                          </button>
                        ))}
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </div>
            );
          })
        )}
      </div>
    </nav>
  );
}

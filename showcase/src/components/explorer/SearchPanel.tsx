import { useState } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import type { TreeEntry } from "../../lib/shared/contracts";
import {
  validateGlobPattern,
  validateVirtualPath,
  type VirtualPath,
} from "../../lib/shared/path";

type SearchMode = "filename" | "glob" | "contents";

interface SearchPage {
  items: readonly unknown[];
}

export interface SearchPanelProps {
  busy?: boolean;
  entries?: readonly TreeEntry[];
  onSearch(operation: PublicOperation): Promise<SearchPage>;
  onSelectPath(path: VirtualPath): void;
}

function itemPath(
  item: unknown,
  entries: readonly TreeEntry[],
): VirtualPath | undefined {
  if (!item || typeof item !== "object") return undefined;
  const value = item as {
    path?: unknown;
    node?: { id?: unknown };
    id?: unknown;
  };
  if (typeof value.path === "string") return value.path as VirtualPath;
  const id = typeof value.node?.id === "string" ? value.node.id : value.id;
  return entries.find((entry) => entry.node.id === id)?.path;
}

/** Fixed, validated discovery requests; results can reopen a matching tree item. */
export function SearchPanel({
  busy = false,
  entries = [],
  onSearch,
  onSelectPath,
}: SearchPanelProps) {
  const [mode, setMode] = useState<SearchMode>("filename");
  const [root, setRoot] = useState("/");
  const [text, setText] = useState("");
  const [items, setItems] = useState<readonly unknown[] | undefined>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(false);

  const submit = async (event: { preventDefault(): void }) => {
    event.preventDefault();
    const query = mode === "glob" ? text : text.trim();
    if (!query) {
      setError("Enter search text.");
      return;
    }
    let operation: PublicOperation;
    if (mode === "glob") {
      try {
        validateGlobPattern(query);
      } catch (error) {
        setError(
          error instanceof Error ? error.message : "Invalid glob pattern.",
        );
        return;
      }
      operation = { kind: "glob", pattern: query };
    } else {
      try {
        validateVirtualPath(root);
      } catch {
        setError("Search root must be a canonical virtual path.");
        return;
      }
      operation =
        mode === "filename"
          ? { kind: "find", root: root as VirtualPath, nameContains: query }
          : { kind: "search_content", root: root as VirtualPath, text: query };
    }
    setLoading(true);
    setError(undefined);
    setItems(undefined);
    try {
      const result = await onSearch(operation);
      setItems(result.items ?? []);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Search failed.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="discovery-panel" aria-label="Search files">
      <div className="panel-heading">
        <h2>Search</h2>
      </div>
      <form onSubmit={(event) => void submit(event)}>
        <fieldset disabled={busy || loading}>
          <legend>Search mode</legend>
          {(["filename", "glob", "contents"] as const).map((option) => (
            <label className="search-mode-option" key={option}>
              <input
                className="search-mode-option"
                type="radio"
                name="search-mode"
                checked={mode === option}
                onChange={() => setMode(option)}
              />
              {option === "filename"
                ? "Filename"
                : option === "glob"
                  ? "Glob"
                  : "Contents"}
            </label>
          ))}
          {mode !== "glob" ? (
            <label>
              Search root
              <input
                aria-label="Search root"
                value={root}
                onChange={(event) => setRoot(event.target.value)}
              />
            </label>
          ) : null}
          <label>
            Search text
            <input
              aria-label="Search text"
              value={text}
              onChange={(event) => setText(event.target.value)}
            />
          </label>
          <button className="button button--accent" type="submit">
            {loading ? "Searching…" : "Search"}
          </button>
        </fieldset>
      </form>
      {error ? (
        <p role="alert" className="panel-error">
          {error}
        </p>
      ) : null}
      {items ? (
        items.length === 0 ? (
          <p className="panel-empty">No results found.</p>
        ) : (
          <ul className="result-list">
            {items.map((item, index) => {
              const path = itemPath(item, entries);
              const label =
                path ??
                (item && typeof item === "object" && "path" in item
                  ? String((item as { path: unknown }).path)
                  : "Matching item");
              return (
                <li key={`${label}-${index}`}>
                  <button
                    type="button"
                    className="result-row"
                    disabled={!path}
                    onClick={() => path && onSelectPath(path)}
                  >
                    {label}
                  </button>
                </li>
              );
            })}
          </ul>
        )
      ) : null}
    </section>
  );
}

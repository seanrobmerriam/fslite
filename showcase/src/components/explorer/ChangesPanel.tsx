import { useCallback, useEffect, useRef, useState } from "react";

import type { Change } from "../../lib/shared/contracts";

interface ChangePage {
  items: readonly Change[];
  next_cursor: string | null;
}
export interface ChangesPanelProps {
  active?: boolean;
  generation: number | undefined;
  onLoad(after?: string): Promise<ChangePage>;
}

/** Cursor pages are appended once, in server sequence order, and reset per generation. */
export function ChangesPanel({
  active = true,
  generation,
  onLoad,
}: ChangesPanelProps) {
  const [items, setItems] = useState<readonly Change[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const epochRef = useRef(0);
  const load = useCallback(
    async (after?: string, replace = false) => {
      const epoch = ++epochRef.current;
      setLoading(true);
      setError(undefined);
      try {
        const page = await onLoad(after);
        if (epoch !== epochRef.current) return;
        setItems((current) => {
          const merged = replace ? page.items : [...current, ...page.items];
          const unique = new Map<number, Change>();
          for (const item of merged) unique.set(item.sequence, item);
          return [...unique.values()].sort(
            (left, right) => left.sequence - right.sequence,
          );
        });
        setCursor(page.next_cursor);
      } catch (reason) {
        if (epoch === epochRef.current)
          setError(
            reason instanceof Error
              ? reason.message
              : "Could not load changes.",
          );
      } finally {
        if (epoch === epochRef.current) setLoading(false);
      }
    },
    [onLoad],
  );
  useEffect(() => {
    epochRef.current += 1;
    setItems([]);
    setCursor(undefined);
    setError(undefined);
    if (active) void load(undefined, true);
  }, [active, generation, load]);
  return (
    <section className="discovery-panel" aria-label="Changes">
      <div className="panel-heading">
        <h2>Changes</h2>
      </div>
      {error ? (
        <p role="alert" className="panel-error">
          {error}
        </p>
      ) : null}
      {loading && items.length === 0 ? <p>Loading changes…</p> : null}
      {!loading && !error && items.length === 0 ? (
        <p className="panel-empty">
          No changes recorded for this workspace yet.
        </p>
      ) : null}
      <ol className="change-list">
        {items.map((change) => (
          <li key={change.sequence}>
            <strong>#{change.sequence}</strong> <span>{change.kind}</span>
            <span className="path-code">
              {change.old_path ?? "—"} → {change.new_path ?? "—"}
            </span>
            <span>revision {change.revision ?? "—"}</span>
            <time dateTime={new Date(change.created_at_ms).toISOString()}>
              {new Date(change.created_at_ms).toLocaleString()}
            </time>
          </li>
        ))}
      </ol>
      {cursor ? (
        <button
          type="button"
          className="button button--quiet"
          disabled={loading}
          onClick={() => void load(cursor)}
        >
          Load more changes
        </button>
      ) : null}
    </section>
  );
}

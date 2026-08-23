import { useCallback, useEffect, useState } from "react";

import type { PublicOperation } from "../../lib/server/schemas";
import type { TrashEntry } from "../../lib/shared/contracts";
import { validateVirtualPath, type VirtualPath } from "../../lib/shared/path";

interface TrashPage {
  items: readonly TrashEntry[];
}
export interface TrashPanelProps {
  busy?: boolean;
  unavailable?: boolean;
  active?: boolean;
  onList(): Promise<TrashPage>;
  onOperation(operation: PublicOperation): Promise<unknown>;
}

/** Recoverable trash with deliberate destination and purge-name confirmations. */
export function TrashPanel({
  busy = false,
  unavailable = false,
  active = true,
  onList,
  onOperation,
}: TrashPanelProps) {
  const [items, setItems] = useState<readonly TrashEntry[]>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(false);
  const [restore, setRestore] = useState<TrashEntry>();
  const [purge, setPurge] = useState<TrashEntry>();
  const [destination, setDestination] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const refresh = useCallback(async () => {
    if (unavailable) return;
    setLoading(true);
    setError(undefined);
    try {
      setItems((await onList()).items ?? []);
    } catch (reason) {
      setError(
        reason instanceof Error ? reason.message : "Could not load trash.",
      );
    } finally {
      setLoading(false);
    }
  }, [onList, unavailable]);
  useEffect(() => {
    if (active && !unavailable) void refresh();
  }, [active, refresh, unavailable]);
  const restoreItem = async () => {
    if (!restore) return;
    const target = destination.trim();
    if (target)
      try {
        validateVirtualPath(target);
      } catch {
        setError("Restore destination must be a canonical virtual path.");
        return;
      }
    try {
      await onOperation({
        kind: "restore",
        trashId: restore.id,
        ...(target ? { destination: target as VirtualPath } : {}),
      });
      setRestore(undefined);
      setDestination("");
      // The mutation already reconciles the filesystem tree. Remove this
      // known item locally instead of turning one restore click into a second
      // visitor-visible list request.
      setItems((current) => current?.filter((item) => item.id !== restore.id));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Restore failed.");
    }
  };
  const purgeItem = async () => {
    if (!purge || confirmation !== purge.node.name) return;
    try {
      await onOperation({
        kind: "purge",
        trashId: purge.id,
        confirmedName: confirmation,
      });
      setPurge(undefined);
      setConfirmation("");
      setItems((current) => current?.filter((item) => item.id !== purge.id));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Purge failed.");
    }
  };
  return (
    <section className="discovery-panel" aria-label="Trash">
      <div className="panel-heading">
        <h2>Trash</h2>
        <button
          type="button"
          className="button button--quiet"
          disabled={busy || unavailable || loading}
          onClick={() => void refresh()}
        >
          Refresh trash
        </button>
      </div>
      {unavailable ? (
        <p className="panel-empty">
          Trash is unavailable until reconnect. Use Retry connection above.
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="panel-error">
          {error}
        </p>
      ) : null}
      {loading && !items ? <p>Loading trash…</p> : null}
      {items?.length === 0 ? (
        <p className="panel-empty">Trash is empty.</p>
      ) : null}
      {items?.map((item) => (
        <article className="trash-item" key={item.id}>
          <strong>{item.original_path}</strong>
          <span>{item.node.name}</span>
          <button
            type="button"
            className="button button--quiet"
            disabled={busy || unavailable || loading}
            onClick={() => {
              setRestore(item);
              setPurge(undefined);
            }}
          >
            Restore {item.node.name}
          </button>
          <button
            type="button"
            className="button button--danger"
            disabled={busy || unavailable || loading}
            onClick={() => {
              setPurge(item);
              setRestore(undefined);
            }}
          >
            Purge {item.node.name}
          </button>
        </article>
      ))}
      {restore ? (
        <div
          className="inline-action"
          role="group"
          aria-label={`Restore ${restore.node.name}`}
        >
          <label>
            Restore destination (optional)
            <input
              aria-label="Restore destination"
              value={destination}
              disabled={unavailable}
              onChange={(event) => setDestination(event.target.value)}
            />
          </label>
          <button
            type="button"
            className="button button--accent"
            disabled={busy || unavailable || loading}
            onClick={() => void restoreItem()}
          >
            Confirm restore
          </button>
        </div>
      ) : null}
      {purge ? (
        <div
          className="inline-action"
          role="group"
          aria-label={`Purge ${purge.node.name}`}
        >
          <label>
            Type <code>{purge.node.name}</code> to confirm
            <input
              aria-label="Confirm name"
              value={confirmation}
              disabled={unavailable}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </label>
          <button
            type="button"
            className="button button--danger"
            disabled={
              busy || unavailable || loading || confirmation !== purge.node.name
            }
            onClick={() => void purgeItem()}
          >
            Purge permanently
          </button>
        </div>
      ) : null}
    </section>
  );
}

import { useEffect, useState } from "react";

import type { BrowserStatus } from "../../lib/browser/api";
import type { WorkspaceUsage } from "../../lib/shared/contracts";

interface WorkspaceStatusProps {
  status: BrowserStatus | undefined;
}

const MEBIBYTE = 1_048_576;

function usageOf(value: unknown): Partial<WorkspaceUsage> {
  return value && typeof value === "object"
    ? (value as Partial<WorkspaceUsage>)
    : {};
}

function formatMebibytes(bytes: number | undefined): string {
  const safeBytes = typeof bytes === "number" && bytes >= 0 ? bytes : 0;
  const value = safeBytes / MEBIBYTE;
  return `${Number.isInteger(value) ? value : value.toFixed(1)} MiB`;
}

function formatCountdown(remainingMs: number): string {
  const seconds = Math.max(0, Math.ceil(remainingMs / 1_000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** Countdown anchors to server time, then advances by locally measured elapsed time. */
export function WorkspaceStatus({ status }: WorkspaceStatusProps) {
  const [receivedAt, setReceivedAt] = useState(() => Date.now());
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const received = Date.now();
    setReceivedAt(received);
    setNow(received);
  }, [status?.generation, status?.now, status?.nextResetAt, status?.resetting]);
  useEffect(() => {
    const timer = globalThis.setInterval(() => setNow(Date.now()), 1_000);
    return () => globalThis.clearInterval(timer);
  }, []);

  if (!status) {
    return (
      <p className="workspace-status workspace-status--loading">
        Connecting to workspace…
      </p>
    );
  }

  const usage = usageOf(status.usage);
  const serverNow = status.now + Math.max(0, now - receivedAt);
  const remaining =
    status.nextResetAt === null ? undefined : status.nextResetAt - serverNow;
  const resetLabel = status.resetting
    ? "Resetting workspace"
    : remaining === undefined
      ? "Reset schedule unavailable"
      : `Reset in ${formatCountdown(remaining)}`;

  return (
    <section className="workspace-status" aria-label="Workspace status">
      <p className="status-health">
        <span aria-hidden="true">●</span> Server ready
      </p>
      <dl>
        <div>
          <dt>Storage</dt>
          <dd>{formatMebibytes(usage.active_logical_bytes)} / 10 MiB</dd>
        </div>
        <div>
          <dt>Nodes</dt>
          <dd>{usage.active_nodes ?? 0} / 250 nodes</dd>
        </div>
        <div>
          <dt>Sandbox</dt>
          <dd aria-live="polite">{resetLabel}</dd>
        </div>
      </dl>
    </section>
  );
}

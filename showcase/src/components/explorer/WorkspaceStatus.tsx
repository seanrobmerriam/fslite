import { useEffect, useRef, useState } from "react";

import type { BrowserStatus } from "../../lib/browser/api";
import type { WorkspaceUsage } from "../../lib/shared/contracts";

interface WorkspaceStatusProps {
  status: BrowserStatus | undefined;
  unavailable?: boolean;
  clock?: MonotonicClock;
}

export interface MonotonicClock {
  monotonicNow(): number;
  setInterval: typeof globalThis.setInterval;
  clearInterval: typeof globalThis.clearInterval;
}

const MEBIBYTE = 1_048_576;
const defaultClock: MonotonicClock = {
  monotonicNow: () => {
    const performance = globalThis.performance;
    return typeof performance?.now === "function"
      ? performance.now()
      : Date.now();
  },
  setInterval: globalThis.setInterval.bind(globalThis),
  clearInterval: globalThis.clearInterval.bind(globalThis),
};

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
export function WorkspaceStatus({
  status,
  unavailable = false,
  clock = defaultClock,
}: WorkspaceStatusProps) {
  const [elapsed, setElapsed] = useState(0);
  const anchorRef = useRef<number | undefined>(undefined);
  const scheduleKey = status
    ? `${status.generation}:${status.now}:${status.nextResetAt}:${status.resetting}`
    : "unavailable";
  const anchoredScheduleRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    anchorRef.current = clock.monotonicNow();
    anchoredScheduleRef.current = scheduleKey;
    setElapsed(0);
  }, [clock, scheduleKey]);
  useEffect(() => {
    const timer = clock.setInterval(() => {
      const current = clock.monotonicNow();
      const anchor = anchorRef.current ?? current;
      anchorRef.current = anchor;
      setElapsed(Math.max(0, current - anchor));
    }, 1_000);
    return () => clock.clearInterval(timer);
  }, [clock]);

  if (!status) {
    return (
      <p
        className={`workspace-status workspace-status--loading${
          unavailable ? " workspace-status--unavailable" : ""
        }`}
        role="status"
        aria-label="Workspace availability"
      >
        {unavailable
          ? "Backend unavailable. Actions are unavailable until the workspace reconnects."
          : "Connecting to workspace…"}
      </p>
    );
  }

  const usage = usageOf(status.usage);
  const effectiveElapsed =
    anchoredScheduleRef.current === scheduleKey ? elapsed : 0;
  const serverNow = status.now + effectiveElapsed;
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

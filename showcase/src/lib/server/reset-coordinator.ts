const DEFAULT_RESET_INTERVAL_MS = 900_000;
const DEFAULT_RETRY_AFTER_MS = 1_000;

export interface ResetClient {
  resetWorkspace(): Promise<unknown>;
}

export interface ResetSnapshot {
  activeOperations: number;
  resetting: boolean;
  generation: number;
  nextResetAt: number | null;
}

interface IntervalHandle {
  unref?: () => unknown;
}

export interface ResetCoordinatorDependencies {
  now?: () => number;
  resetIntervalMs?: number;
  retryAfterMs?: number;
  setInterval?: (callback: () => void, intervalMs: number) => IntervalHandle;
  clearInterval?: (handle: IntervalHandle) => void;
}

export class WorkspaceResettingError extends Error {
  readonly name = "WorkspaceResettingError";

  constructor(readonly retryAfterMs = DEFAULT_RETRY_AFTER_MS) {
    super("The shared workspace is resetting; try again shortly");
  }
}

/**
 * A small reader/writer gate for the one shared showcase workspace. Operations
 * admitted before a reset finish; once a reset is pending no new work enters.
 */
export class ResetCoordinator {
  private activeOperations = 0;
  private resetting = false;
  private generation = 0;
  private nextResetAt: number | null = null;
  private readonly zeroWaiters: Array<() => void> = [];
  private resetPromise: Promise<void> | undefined;
  private interval: IntervalHandle | undefined;
  private disposed = false;
  private readonly now: () => number;
  private readonly resetIntervalMs: number;
  private readonly retryAfterMs: number;
  private readonly schedule: (
    callback: () => void,
    intervalMs: number,
  ) => IntervalHandle;
  private readonly cancelSchedule: (handle: IntervalHandle) => void;

  constructor(
    private readonly client: ResetClient,
    private readonly seed: () => Promise<void>,
    dependencies: ResetCoordinatorDependencies = {},
  ) {
    this.now = dependencies.now ?? Date.now;
    this.resetIntervalMs =
      dependencies.resetIntervalMs ?? DEFAULT_RESET_INTERVAL_MS;
    this.retryAfterMs = dependencies.retryAfterMs ?? DEFAULT_RETRY_AFTER_MS;
    this.schedule =
      dependencies.setInterval ??
      ((callback, intervalMs) =>
        globalThis.setInterval(
          callback,
          intervalMs,
        ) as unknown as IntervalHandle);
    this.cancelSchedule =
      dependencies.clearInterval ??
      ((handle) =>
        globalThis.clearInterval(handle as ReturnType<typeof setInterval>));
  }

  snapshot(): ResetSnapshot {
    return {
      activeOperations: this.activeOperations,
      resetting: this.resetting,
      generation: this.generation,
      nextResetAt: this.nextResetAt,
    };
  }

  async withOperation<T>(operation: () => Promise<T>): Promise<T> {
    if (this.resetting) {
      throw new WorkspaceResettingError(this.retryAfterMs);
    }

    this.activeOperations += 1;
    try {
      return await operation();
    } finally {
      this.activeOperations -= 1;
      if (this.activeOperations === 0) {
        this.releaseZeroWaiters();
      }
    }
  }

  resetNow(): Promise<void> {
    if (this.resetPromise) {
      return this.resetPromise;
    }

    this.resetting = true;
    const pending = this.performReset().finally(() => {
      this.resetting = false;
      if (this.resetPromise === pending) {
        this.resetPromise = undefined;
      }
    });
    this.resetPromise = pending;
    return pending;
  }

  async start(): Promise<void> {
    if (this.disposed || this.interval) {
      return;
    }
    await this.resetNow();
    if (this.disposed || this.interval) {
      return;
    }

    this.interval = this.schedule(() => {
      void this.resetNow().catch(() => undefined);
    }, this.resetIntervalMs);
    this.interval.unref?.();
  }

  dispose(): void {
    this.disposed = true;
    if (!this.interval) {
      return;
    }
    this.cancelSchedule(this.interval);
    this.interval = undefined;
  }

  private async performReset(): Promise<void> {
    await this.waitForZeroOperations();
    await this.client.resetWorkspace();
    await this.seed();
    this.generation += 1;
    this.nextResetAt = this.now() + this.resetIntervalMs;
  }

  private waitForZeroOperations(): Promise<void> {
    if (this.activeOperations === 0) {
      return Promise.resolve();
    }
    return new Promise((resolve) => this.zeroWaiters.push(resolve));
  }

  private releaseZeroWaiters(): void {
    for (const resolve of this.zeroWaiters.splice(0)) {
      resolve();
    }
  }
}

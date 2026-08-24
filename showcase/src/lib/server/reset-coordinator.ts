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

interface TimerHandle {
  unref?: () => unknown;
}

export interface ResetCoordinatorDependencies {
  now?: () => number;
  resetIntervalMs?: number;
  retryAfterMs?: number;
  setTimeout?: (callback: () => void, delayMs: number) => TimerHandle;
  clearTimeout?: (handle: TimerHandle) => void;
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
  private timer: TimerHandle | undefined;
  private started = false;
  private disposed = false;
  private readonly now: () => number;
  private readonly resetIntervalMs: number;
  private readonly retryAfterMs: number;
  private readonly schedule: (
    callback: () => void,
    delayMs: number,
  ) => TimerHandle;
  private readonly cancelSchedule: (handle: TimerHandle) => void;

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
      dependencies.setTimeout ??
      ((callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs) as unknown as TimerHandle);
    this.cancelSchedule =
      dependencies.clearTimeout ??
      ((handle) =>
        globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>));
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
    if (this.disposed || this.started) {
      return;
    }
    await this.resetNow();
    if (this.disposed || this.started) {
      return;
    }

    this.started = true;
    this.installTimer(this.resetIntervalMs);
  }

  dispose(): void {
    this.disposed = true;
    this.nextResetAt = null;
    if (!this.timer) {
      return;
    }
    this.cancelSchedule(this.timer);
    this.timer = undefined;
  }

  private installTimer(delayMs: number): void {
    this.timer = this.schedule(() => {
      this.timer = undefined;
      void this.runScheduledReset();
    }, delayMs);
    this.timer.unref?.();
  }

  private async runScheduledReset(): Promise<void> {
    this.nextResetAt = null;
    try {
      await this.resetNow();
    } catch {
      if (this.disposed) {
        return;
      }

      // A reset may have emptied the workspace before a seed write failed.
      // Keep all public work gated until a complete reset-and-seed succeeds.
      this.resetting = true;
      this.installTimer(this.retryAfterMs);
      return;
    }

    if (!this.disposed) {
      this.installTimer(this.resetIntervalMs);
    }
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

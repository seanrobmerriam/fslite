export const RATE_LIMITS = {
  read: 120,
  mutation: 30,
  upload: 10,
} as const;

export type RateLimitBucket = keyof typeof RATE_LIMITS;

export interface RateLimitResult {
  allowed: boolean;
  bucket?: RateLimitBucket;
  retryAfterMs: number;
}

export interface RateLimitDependencies {
  now?: () => number;
  maxKeys?: number;
}

const WINDOW_MS = 60_000;
const DEFAULT_MAX_KEYS = 10_000;

/** Per-IP in-memory rate limiter with sliding, one-minute windows. */
export class RollingWindowRateLimiter {
  private readonly timestamps = new Map<string, number[]>();
  private readonly now: () => number;
  private readonly maxKeys: number;

  constructor(dependencies: RateLimitDependencies = {}) {
    this.now = dependencies.now ?? Date.now;
    this.maxKeys = dependencies.maxKeys ?? DEFAULT_MAX_KEYS;
    if (!Number.isSafeInteger(this.maxKeys) || this.maxKeys < 1) {
      throw new Error("rate-limit maxKeys must be a positive safe integer");
    }
  }

  check(clientIp: string, bucket: RateLimitBucket): RateLimitResult {
    return this.checkAll(clientIp, [bucket]);
  }

  /**
   * Atomically checks every bucket that applies to a visitor operation. An
   * upload is also a mutation, so recording it in just the upload bucket would
   * let it bypass the normal mutation ceiling.
   */
  checkAll(
    clientIp: string,
    buckets: readonly RateLimitBucket[],
  ): RateLimitResult {
    const uniqueBuckets = [...new Set(buckets)];
    const now = this.now();
    this.pruneAll(now);
    const newKeyCount = uniqueBuckets.filter(
      (bucket) => !this.timestamps.has(this.key(clientIp, bucket)),
    ).length;
    if (this.timestamps.size + newKeyCount > this.maxKeys) {
      return {
        allowed: false,
        bucket: uniqueBuckets[0],
        retryAfterMs: this.capacityRetryAfterMs(now),
      };
    }

    for (const bucket of uniqueBuckets) {
      const timestamps = this.prune(clientIp, bucket, now);
      if (timestamps.length >= RATE_LIMITS[bucket]) {
        return {
          allowed: false,
          bucket,
          retryAfterMs: Math.max(0, timestamps[0] + WINDOW_MS - now),
        };
      }
    }

    for (const bucket of uniqueBuckets) {
      const timestamps = this.prune(clientIp, bucket, now);
      timestamps.push(now);
      this.timestamps.set(this.key(clientIp, bucket), timestamps);
    }
    return { allowed: true, retryAfterMs: 0 };
  }

  /** Visible for process-health metrics and deterministic memory-bound tests. */
  activeKeyCount(): number {
    this.pruneAll(this.now());
    return this.timestamps.size;
  }

  private prune(
    clientIp: string,
    bucket: RateLimitBucket,
    now: number,
  ): number[] {
    const key = this.key(clientIp, bucket);
    const current = this.timestamps.get(key) ?? [];
    const retained = current.filter((timestamp) => timestamp > now - WINDOW_MS);
    if (retained.length === 0) {
      this.timestamps.delete(key);
      return [];
    }
    this.timestamps.set(key, retained);
    return retained;
  }

  private key(clientIp: string, bucket: RateLimitBucket): string {
    return `${clientIp}:${bucket}`;
  }

  private pruneAll(now: number): void {
    for (const [key, timestamps] of this.timestamps) {
      const retained = timestamps.filter(
        (timestamp) => timestamp > now - WINDOW_MS,
      );
      if (retained.length === 0) {
        this.timestamps.delete(key);
      } else if (retained.length !== timestamps.length) {
        this.timestamps.set(key, retained);
      }
    }
  }

  private capacityRetryAfterMs(now: number): number {
    const oldestTimestamp = Math.min(
      ...[...this.timestamps.values()].map((timestamps) => timestamps[0]),
    );
    return Number.isFinite(oldestTimestamp)
      ? Math.max(0, oldestTimestamp + WINDOW_MS - now)
      : 0;
  }
}

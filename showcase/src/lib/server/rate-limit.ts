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
}

const WINDOW_MS = 60_000;

/** Per-IP in-memory rate limiter with sliding, one-minute windows. */
export class RollingWindowRateLimiter {
  private readonly timestamps = new Map<string, number[]>();
  private readonly now: () => number;

  constructor(dependencies: RateLimitDependencies = {}) {
    this.now = dependencies.now ?? Date.now;
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
}

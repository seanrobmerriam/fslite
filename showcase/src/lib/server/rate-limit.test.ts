import { describe, expect, it } from "vitest";

import { RollingWindowRateLimiter } from "./rate-limit";

function limiterAt(startedAt = 1_000_000) {
  let now = startedAt;
  return {
    limiter: new RollingWindowRateLimiter({ now: () => now }),
    advance(milliseconds: number) {
      now += milliseconds;
    },
  };
}

describe("RollingWindowRateLimiter", () => {
  it.each([
    { bucket: "read" as const, limit: 120 },
    { bucket: "mutation" as const, limit: 30 },
    { bucket: "upload" as const, limit: 10 },
  ])(
    "rejects request $limit + 1 above the $bucket rolling-minute limit and permits it after one minute",
    ({ bucket, limit }) => {
      const { limiter } = limiterAt();

      for (let request = 0; request < limit; request += 1) {
        expect(limiter.check("203.0.113.1", bucket).allowed).toBe(true);
      }

      expect(limiter.check("203.0.113.1", bucket)).toMatchObject({
        allowed: false,
        retryAfterMs: 60_000,
      });
      const sixtySecondsLater = limiterAt();
      for (let request = 0; request < limit; request += 1) {
        sixtySecondsLater.limiter.check("203.0.113.1", bucket);
      }
      sixtySecondsLater.advance(60_000);
      expect(sixtySecondsLater.limiter.check("203.0.113.1", bucket)).toEqual({
        allowed: true,
        retryAfterMs: 0,
      });
    },
  );

  it("prunes every stale timestamp and permits the next request after one minute", () => {
    const { limiter, advance } = limiterAt();

    for (let request = 0; request < 120; request += 1) {
      expect(limiter.check("203.0.113.1", "read").allowed).toBe(true);
    }
    expect(limiter.check("203.0.113.1", "read").allowed).toBe(false);

    advance(60_000);

    expect(limiter.check("203.0.113.1", "read")).toEqual({
      allowed: true,
      retryAfterMs: 0,
    });
  });

  it("keeps client IP and bucket windows independent", () => {
    const { limiter } = limiterAt();

    for (let request = 0; request < 30; request += 1) {
      limiter.check("203.0.113.1", "mutation");
    }

    expect(limiter.check("203.0.113.1", "mutation").allowed).toBe(false);
    expect(limiter.check("203.0.113.2", "mutation").allowed).toBe(true);
    expect(limiter.check("203.0.113.1", "read").allowed).toBe(true);
  });

  it("requires all applicable buckets before recording an upload", () => {
    const { limiter } = limiterAt();

    for (let request = 0; request < 10; request += 1) {
      expect(
        limiter.checkAll("203.0.113.1", ["mutation", "upload"]).allowed,
      ).toBe(true);
    }

    expect(
      limiter.checkAll("203.0.113.1", ["mutation", "upload"]),
    ).toMatchObject({ allowed: false, bucket: "upload" });
    expect(limiter.check("203.0.113.1", "mutation").allowed).toBe(true);
  });
});

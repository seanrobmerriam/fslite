import { describe, expect, it } from "vitest";

import { buildActivity } from "./activity";

describe("buildActivity", () => {
  it("never puts the bearer credential or headers in activity", () => {
    const record = buildActivity({
      token: "super-secret",
      serverUrl: "http://fslite-server:8080",
      method: "GET",
      path: "/v1/me",
      status: 200,
      durationMs: 4,
      headers: {
        authorization: "Bearer super-secret",
        cookie: "session=private",
      },
      response: { workspace_id: "w" },
      requestId: "request-1",
    });

    expect(JSON.stringify(record)).not.toContain("super-secret");
    expect(JSON.stringify(record)).not.toContain("session=private");
    expect(record.curl).toContain("Authorization: Bearer $FSLITE_TOKEN");
    expect(record.curl).toContain("$FSLITE_SERVER_URL/v1/me");
    expect(record).not.toHaveProperty("headers");
  });

  it("redacts token-shaped values in JSON payloads", () => {
    const record = buildActivity({
      token: "super-secret",
      serverUrl: "http://server",
      method: "POST",
      path: "/v1/workspaces/w/fs/a.txt?action=trash",
      status: 200,
      durationMs: 1,
      request: { authorization: "Bearer super-secret", nested: "super-secret" },
      response: { token: "super-secret" },
      requestId: "request-2",
    });

    expect(JSON.stringify(record)).not.toContain("super-secret");
    expect(record.request).toEqual({
      authorization: "[REDACTED]",
      nested: "[REDACTED]",
    });
    expect(record.response).toEqual({ token: "[REDACTED]" });
  });

  it("bounds large JSON activity payloads at 64 KiB", () => {
    const record = buildActivity({
      token: "token",
      serverUrl: "http://server",
      method: "GET",
      path: "/v1/me",
      status: 200,
      durationMs: 1,
      response: { payload: "x".repeat(70 * 1024) },
      requestId: "request-3",
    });

    expect(record.response).toMatchObject({
      truncated: true,
      originalBytes: expect.any(Number),
    });
    expect(JSON.stringify(record.response).length).toBeLessThanOrEqual(65_536);
  });

  it("summarizes binary data without serializing its bytes", () => {
    const record = buildActivity({
      token: "token",
      serverUrl: "http://server",
      method: "GET",
      path: "/v1/workspaces/w/content/archive.bin",
      status: 200,
      durationMs: 1,
      response: new Uint8Array([0, 1, 2, 255]),
      contentType: "application/octet-stream",
      requestId: "request-4",
    });

    expect(record.response).toEqual({
      binary: true,
      bytes: 4,
      contentType: "application/octet-stream",
    });
  });
});

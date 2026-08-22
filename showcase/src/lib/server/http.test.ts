import { describe, expect, it } from "vitest";

describe("server HTTP helpers", () => {
  it("accepts application/json with an optional charset", async () => {
    const { isJsonRequest } = await import("./http");

    expect(
      isJsonRequest(
        new Request("http://showcase.test/api/operation", {
          headers: { "content-type": "application/json; charset=utf-8" },
        }),
      ),
    ).toBe(true);
    expect(
      isJsonRequest(
        new Request("http://showcase.test/api/operation", {
          headers: { "content-type": "text/json" },
        }),
      ),
    ).toBe(false);
  });

  it("rejects an oversized declared request body before reading it", async () => {
    const { BoundedBodyError, readBoundedBody } = await import("./http");
    const request = new Request("http://showcase.test/api/upload", {
      method: "POST",
      headers: { "content-length": String(1024 * 1024 + 1) },
      body: new ReadableStream({
        pull() {
          throw new Error("the body must not be read");
        },
      }),
      // Node requires this for streamed request bodies.
      duplex: "half",
    } as RequestInit);

    await expect(readBoundedBody(request, 1024 * 1024)).rejects.toEqual(
      expect.objectContaining({
        name: BoundedBodyError.name,
        limitBytes: 1024 * 1024,
      }),
    );
  });

  it("cancels a streamed body once cumulative bytes exceed its limit", async () => {
    const { BoundedBodyError, readBoundedBody } = await import("./http");
    let cancelled = false;
    const request = new Request("http://showcase.test/api/upload", {
      method: "POST",
      body: new ReadableStream({
        start(controller) {
          controller.enqueue(new Uint8Array([1, 2]));
          controller.enqueue(new Uint8Array([3, 4]));
        },
        cancel() {
          cancelled = true;
        },
      }),
      duplex: "half",
    } as RequestInit);

    await expect(readBoundedBody(request, 3)).rejects.toBeInstanceOf(
      BoundedBodyError,
    );
    expect(cancelled).toBe(true);
  });

  it.each([
    ["203.0.113.8, 198.51.100.9", "203.0.113.8"],
    ["  2001:db8::8 , 2001:db8::9", "2001:db8::8"],
    ["", "198.51.100.7"],
    ["not-an-ip", "198.51.100.7"],
    ['"203.0.113.8"', "198.51.100.7"],
    ["[2001:db8::8]", "198.51.100.7"],
    ["203.0.113.8:443", "198.51.100.7"],
    ["[2001:db8::8]:443", "198.51.100.7"],
    ["fe80::8%en0", "198.51.100.7"],
    ["203.0.113.8\n198.51.100.9", "198.51.100.7"],
    ["203.0.113.8\0", "198.51.100.7"],
  ])(
    "uses only a plain first XFF IP literal when trusted: %j",
    async (forwarded, expected) => {
      const { clientIp } = await import("./http");
      const request = {
        headers: {
          get: (name: string) =>
            name === "x-forwarded-for" ? forwarded : null,
        },
      } as unknown as Request;

      expect(clientIp(request, "198.51.100.7", true)).toBe(expected);
      expect(clientIp(request, "198.51.100.7", false)).toBe("198.51.100.7");
    },
  );

  it("uses forwarded client addresses only when ServerConfig trusts the proxy", async () => {
    const { clientIp } = await import("./http");
    const request = new Request("http://showcase.test/api/status", {
      headers: { "x-forwarded-for": "203.0.113.8, 198.51.100.9" },
    });

    expect(clientIp(request, "198.51.100.7", false)).toBe("198.51.100.7");
    expect(clientIp(request, "198.51.100.7", true)).toBe("203.0.113.8");
  });

  it("keeps canonical once-decoded query percent paths unchanged", async () => {
    const { validateQueryPath } = await import("./http");

    expect(validateQueryPath("/docs/100%.txt")).toBe("/docs/100%.txt");
    expect(validateQueryPath("/docs/100%25.txt")).toBe("/docs/100%25.txt");
    expect(validateQueryPath("/docs/%2e%2e/private.txt")).toBe(
      "/docs/%2e%2e/private.txt",
    );
    expect(validateQueryPath("/docs/☃.txt")).toBe("/docs/☃.txt");
  });

  it.each([["/docs/../private.txt"], ["/docs//private.txt"]])(
    "rejects once-decoded non-canonical query paths: %s",
    async (path) => {
      const { validateQueryPath } = await import("./http");
      expect(() => validateQueryPath(path)).toThrow("The path is invalid.");
    },
  );

  it("returns consistent retry metadata for rate limits", async () => {
    const { gatewayErrorResponse } = await import("./http");
    const { GatewayRateLimitError } = await import("./gateway");

    const response = gatewayErrorResponse(
      new GatewayRateLimitError("read", 1_234),
      "request-123",
    );

    expect(response.status).toBe(429);
    expect(response.headers.get("retry-after")).toBe("2");
    expect(response.headers.get("x-request-id")).toBe("request-123");
    await expect(response.json()).resolves.toEqual({
      error: {
        code: "rate_limited",
        message: "Too many requests; try again shortly.",
        status: 429,
        requestId: "request-123",
        retryAfterMs: 1_234,
      },
    });
  });

  it("returns reset retry metadata without exposing coordinator internals", async () => {
    const { gatewayErrorResponse } = await import("./http");
    const { WorkspaceResettingError } = await import("./reset-coordinator");
    const response = gatewayErrorResponse(
      new WorkspaceResettingError(750),
      "request-reset",
    );

    expect(response.status).toBe(503);
    expect(response.headers.get("retry-after")).toBe("1");
    await expect(response.json()).resolves.toEqual({
      error: {
        code: "workspace_resetting",
        message: "The shared workspace is resetting; try again shortly.",
        status: 503,
        requestId: "request-reset",
        retryAfterMs: 750,
      },
    });
  });

  it("maps an allowlisted structured upstream error to a fixed public message", async () => {
    const { gatewayErrorResponse } = await import("./http");
    const { UpstreamApiError } = await import("./fslite-client");
    const response = gatewayErrorResponse(
      new UpstreamApiError(
        412,
        "revision_conflict",
        "file:///private/workspace/secret.txt Bearer accidental-secret",
        null,
        "upstream-123",
      ),
    );

    expect(response.status).toBe(412);
    await expect(response.json()).resolves.toEqual({
      error: {
        code: "revision_conflict",
        message: "The file changed before the operation completed.",
        status: 412,
        requestId: "upstream-123",
      },
    });
  });

  it.each([
    "file:///private/workspace/secret.txt",
    "ftp://private.example.test/secret",
    "custom-scheme://private.example.test/secret",
    "/var/private/secret.txt",
    "Bearer random-secret-value",
    "line one\nline two",
    "<script>alert('secret')</script>",
  ])("never reflects arbitrary upstream error text: %s", async (message) => {
    const { gatewayErrorResponse } = await import("./http");
    const { UpstreamApiError } = await import("./fslite-client");
    const response = gatewayErrorResponse(
      new UpstreamApiError(
        409,
        "unknown_upstream_code",
        message,
        null,
        "upstream-123",
      ),
    );

    expect(response.status).toBe(409);
    const body = await response.text();
    expect(body).toContain("The filesystem service rejected the request.");
    expect(body).not.toContain(message);
    expect(body).not.toContain("secret");
    expect(body).not.toContain("private");
  });
});

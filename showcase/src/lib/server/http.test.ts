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

  it("uses forwarded client addresses only when ServerConfig trusts the proxy", async () => {
    const { clientIp } = await import("./http");
    const request = new Request("http://showcase.test/api/status", {
      headers: { "x-forwarded-for": "203.0.113.8, 198.51.100.9" },
    });

    expect(clientIp(request, "198.51.100.7", false)).toBe("198.51.100.7");
    expect(clientIp(request, "198.51.100.7", true)).toBe("203.0.113.8");
  });

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

  it("keeps structured upstream client errors public without leaking URLs or tokens", async () => {
    const { gatewayErrorResponse } = await import("./http");
    const { UpstreamApiError } = await import("./fslite-client");
    const response = gatewayErrorResponse(
      new UpstreamApiError(
        412,
        "revision_conflict",
        "The current revision does not match.",
        null,
        "upstream-123",
      ),
    );

    expect(response.status).toBe(412);
    await expect(response.json()).resolves.toEqual({
      error: {
        code: "revision_conflict",
        message: "The current revision does not match.",
        status: 412,
        requestId: "upstream-123",
      },
    });

    const generic = gatewayErrorResponse(
      new Error("http://private.example.test/token/secret stack trace"),
      "request-456",
    );
    expect(generic.status).toBe(502);
    const body = await generic.text();
    expect(body).not.toContain("private.example.test");
    expect(body).not.toContain("secret");
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";

const runtime = {
  readiness: vi.fn(),
  status: vi.fn(),
  execute: vi.fn(),
  upload: vi.fn(),
  download: vi.fn(),
};
const config = { trustProxy: false };

vi.mock("../lib/server/runtime", () => ({
  getShowcaseRuntime: vi.fn(async () => runtime),
}));

vi.mock("../lib/server/config", () => ({
  loadServerConfig: vi.fn(() => config),
}));

function context(
  request: Request,
  params: Record<string, string | undefined> = {},
) {
  return {
    request,
    url: new URL(request.url),
    params,
    clientAddress: "198.51.100.7",
  };
}

const activity = {
  method: "GET",
  path: "/content/example.bin",
  status: 200,
  durationMs: 12,
  requestId: "upstream-1",
};

beforeEach(() => {
  vi.clearAllMocks();
  config.trustProxy = false;
  runtime.readiness.mockResolvedValue({ ready: true, workspaceId: "ws-1" });
  runtime.status.mockResolvedValue({
    ready: true,
    workspaceId: "ws-1",
    activeOperations: 0,
    generation: 3,
    resetting: false,
    nextResetAt: 2_000,
    usage: { active_nodes: 1 },
  });
  runtime.execute.mockResolvedValue({ data: { items: [] }, activity });
  runtime.upload.mockResolvedValue({ data: { id: "node-1" }, activity });
  runtime.download.mockResolvedValue({
    data: new Uint8Array([0, 255, 7]),
    activity,
    contentType: "text/html",
  });
});

describe("Astro API route contracts", () => {
  it("keeps liveness independent from runtime initialization", async () => {
    const { GET } = await import("../pages/api/health/live");
    const response = await GET(
      context(new Request("http://showcase.test/api/health/live")) as never,
    );

    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({ ok: true });
    const { getShowcaseRuntime } = await import("../lib/server/runtime");
    expect(getShowcaseRuntime).not.toHaveBeenCalled();
  });

  it("initializes readiness and returns a public runtime status contract", async () => {
    const { GET: ready } = await import("../pages/api/health/ready");
    const readyResponse = await ready(
      context(new Request("http://showcase.test/api/health/ready")) as never,
    );
    expect(await readyResponse.json()).toEqual({
      ready: true,
      workspaceId: "ws-1",
    });

    const { GET: status } = await import("../pages/api/status");
    const statusResponse = await status(
      context(new Request("http://showcase.test/api/status")) as never,
    );
    const body = (await statusResponse.json()) as Record<string, unknown>;
    expect(statusResponse.status).toBe(200);
    expect(body).toMatchObject({
      ready: true,
      generation: 3,
      resetting: false,
      nextResetAt: 2_000,
      usage: { active_nodes: 1 },
    });
    expect(body).not.toHaveProperty("workspaceId");
    expect(body).not.toHaveProperty("activeOperations");
    expect(body.now).toEqual(expect.any(Number));
  });

  it("uses one request ID for direct media-type errors", async () => {
    const { POST } = await import("../pages/api/operation");
    const rejected = await POST(
      context(
        new Request("http://showcase.test/api/operation", {
          method: "POST",
          headers: { "content-type": "text/plain" },
          body: "{}",
        }),
      ) as never,
    );
    expect(rejected.status).toBe(415);
    const error = (await rejected.json()) as { error: { requestId: string } };
    expect(rejected.headers.get("x-request-id")).toBe(error.error.requestId);
  });

  it("requires JSON and routes parsed operations with the direct client address", async () => {
    const { POST } = await import("../pages/api/operation");

    const response = await POST(
      context(
        new Request("http://showcase.test/api/operation", {
          method: "POST",
          headers: { "content-type": "application/json; charset=utf-8" },
          body: JSON.stringify({ kind: "usage" }),
        }),
      ) as never,
    );
    expect(response.status).toBe(200);
    expect(runtime.execute).toHaveBeenCalledWith(
      { kind: "usage" },
      "198.51.100.7",
    );
  });

  it("rejects malformed operations with the public 400 envelope", async () => {
    const { POST } = await import("../pages/api/operation");
    const response = await POST(
      context(
        new Request("http://showcase.test/api/operation", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: "{",
        }),
      ) as never,
    );

    expect(response.status).toBe(400);
    await expect(response.json()).resolves.toMatchObject({
      error: { code: "invalid_request", status: 400 },
    });
  });

  it("canonicalizes upload query paths before sending bounded raw bytes upstream", async () => {
    const { POST } = await import("../pages/api/upload");
    const response = await POST(
      context(
        new Request(
          "http://showcase.test/api/upload?path=%2Fdocs%2Fhello.txt",
          {
            method: "POST",
            body: new Uint8Array([1, 2, 3]),
          },
        ),
      ) as never,
    );

    expect(response.status).toBe(200);
    expect(runtime.upload).toHaveBeenCalledWith(
      "/docs/hello.txt",
      new Uint8Array([1, 2, 3]),
      "198.51.100.7",
    );
  });

  it("does not decode already-decoded upload query paths a second time", async () => {
    const { POST } = await import("../pages/api/upload");
    const response = await POST(
      context(
        new Request(
          "http://showcase.test/api/upload?path=%2Fdocs%2F100%2525.txt",
          { method: "POST", body: new Uint8Array([1]) },
        ),
      ) as never,
    );

    expect(response.status).toBe(200);
    expect(runtime.upload).toHaveBeenCalledWith(
      "/docs/100%25.txt",
      new Uint8Array([1]),
      "198.51.100.7",
    );
  });

  it("rejects a query path whose URL decoding creates traversal", async () => {
    const { POST } = await import("../pages/api/upload");
    const response = await POST(
      context(
        new Request(
          "http://showcase.test/api/upload?path=%2Fdocs%2F%2e%2e%2Fprivate.txt",
          { method: "POST", body: new Uint8Array([1]) },
        ),
      ) as never,
    );

    expect(response.status).toBe(400);
    expect(runtime.upload).not.toHaveBeenCalled();
  });

  it("uses a valid trusted XFF address for runtime rate-limit identity", async () => {
    config.trustProxy = true;
    const { POST } = await import("../pages/api/operation");
    const response = await POST(
      context(
        new Request("http://showcase.test/api/operation", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-forwarded-for": "2001:db8::8, 198.51.100.9",
          },
          body: JSON.stringify({ kind: "usage" }),
        }),
      ) as never,
    );

    expect(response.status).toBe(200);
    expect(runtime.execute).toHaveBeenCalledWith(
      { kind: "usage" },
      "2001:db8::8",
    );
  });

  it("emits only safe query-download headers and a sanitized attachment filename", async () => {
    const { GET } = await import("../pages/api/download");
    const response = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2Freport%22%0A.bin",
          { headers: { authorization: "Bearer browser-secret" } },
        ),
      ) as never,
    );

    expect(runtime.download).toHaveBeenCalledWith(
      '/docs/report"\n.bin',
      "198.51.100.7",
    );
    expect(response.headers.get("content-type")).toBe(
      "application/octet-stream",
    );
    expect(response.headers.get("content-disposition")).toBe(
      'attachment; filename="report__.bin"',
    );
    expect(response.headers.get("authorization")).toBeNull();
    expect(
      [...response.headers.keys()].filter((key) => key.startsWith("x-")),
    ).toEqual([
      "x-fslite-duration-ms",
      "x-fslite-method",
      "x-fslite-path",
      "x-fslite-status",
      "x-request-id",
    ]);
    expect(await response.bytes()).toEqual(new Uint8Array([0, 255, 7]));
  });

  it("validates query downloads once before runtime and preserves canonical percent and Unicode names", async () => {
    const { GET } = await import("../pages/api/download");
    const literalPercent = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2F100%25.txt",
        ),
      ) as never,
    );
    expect(literalPercent.status).toBe(200);
    expect(runtime.download).toHaveBeenCalledWith(
      "/docs/100%.txt",
      "198.51.100.7",
    );

    const encodedPercent = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2F100%2525.txt",
        ),
      ) as never,
    );
    expect(encodedPercent.status).toBe(200);
    expect(runtime.download).toHaveBeenCalledWith(
      "/docs/100%25.txt",
      "198.51.100.7",
    );

    const doubleEncodedTraversalName = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2F%252e%252e%2Fprivate.txt",
        ),
      ) as never,
    );
    expect(doubleEncodedTraversalName.status).toBe(200);
    expect(runtime.download).toHaveBeenCalledWith(
      "/docs/%2e%2e/private.txt",
      "198.51.100.7",
    );

    const unicode = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2F%E2%98%83.txt",
        ),
      ) as never,
    );
    expect(unicode.status).toBe(200);
    expect(runtime.download).toHaveBeenCalledWith(
      "/docs/☃.txt",
      "198.51.100.7",
    );

    for (const path of ["%2Fdocs%2F%2e%2e%2Fprivate.txt"]) {
      const invalid = await GET(
        context(
          new Request(`http://showcase.test/api/download?path=${path}`),
        ) as never,
      );
      expect(invalid.status).toBe(400);
      const error = (await invalid.json()) as {
        error: { requestId: string };
      };
      expect(invalid.headers.get("x-request-id")).toBe(error.error.requestId);
    }
    expect(runtime.download).toHaveBeenCalledTimes(4);
  });

  it("rejects an oversized download with a request-ID-consistent error", async () => {
    runtime.download.mockResolvedValueOnce({
      data: new Uint8Array(1024 * 1024 + 1),
      activity,
    });
    const { GET } = await import("../pages/api/download");
    const response = await GET(
      context(
        new Request(
          "http://showcase.test/api/download?path=%2Fdocs%2Flarge.bin",
        ),
      ) as never,
    );

    expect(response.status).toBe(502);
    const error = (await response.json()) as {
      error: { code: string; requestId: string };
    };
    expect(error.error.code).toBe("upstream_response_too_large");
    expect(response.headers.get("x-request-id")).toBe(error.error.requestId);
  });

  it.each([
    ["status", "../pages/api/status", "GET"],
    ["live", "../pages/api/health/live", "GET"],
    ["ready", "../pages/api/health/ready", "GET"],
    ["operation", "../pages/api/operation", "POST"],
    ["upload", "../pages/api/upload", "POST"],
    ["download", "../pages/api/download", "GET"],
  ])(
    "uses 405, Allow, and one request ID for %s",
    async (_name, module, allow) => {
      const { ALL } = (await import(module)) as {
        ALL: (context: never) => Promise<Response> | Response;
      };
      const response = await ALL(
        context(new Request("http://showcase.test/api/unsupported")) as never,
      );

      expect(response.status).toBe(405);
      expect(response.headers.get("allow")).toBe(allow);
      const error = (await response.json()) as { error: { requestId: string } };
      expect(response.headers.get("x-request-id")).toBe(error.error.requestId);
    },
  );
});

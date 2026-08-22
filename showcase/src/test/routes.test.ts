import { beforeEach, describe, expect, it, vi } from "vitest";

const runtime = {
  readiness: vi.fn(),
  status: vi.fn(),
  execute: vi.fn(),
  upload: vi.fn(),
  download: vi.fn(),
};

vi.mock("../lib/server/runtime", () => ({
  getShowcaseRuntime: vi.fn(async () => runtime),
}));

vi.mock("../lib/server/config", () => ({
  loadServerConfig: vi.fn(() => ({ trustProxy: false })),
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
  runtime.readiness.mockResolvedValue({ ready: true, workspaceId: "ws-1" });
  runtime.status.mockResolvedValue({
    ready: true,
    workspaceId: "ws-1",
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
      workspaceId: "ws-1",
      generation: 3,
      resetting: false,
      nextResetAt: 2_000,
      usage: { active_nodes: 1 },
    });
    expect(body.now).toEqual(expect.any(Number));
  });

  it("requires JSON and routes parsed operations with the direct client address", async () => {
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

  it("emits only safe download headers and a sanitized attachment filename", async () => {
    const { GET } = await import("../pages/api/download/[...path]");
    const response = await GET(
      context(
        new Request("http://showcase.test/api/download/docs/report%22%0A.bin", {
          headers: { authorization: "Bearer browser-secret" },
        }),
        { path: "docs/report%22%0A.bin" },
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

  it("uses 405 and Allow for an unsupported endpoint method", async () => {
    const { ALL } = await import("../pages/api/operation");
    const response = await ALL(
      context(
        new Request("http://showcase.test/api/operation", { method: "GET" }),
      ) as never,
    );

    expect(response.status).toBe(405);
    expect(response.headers.get("allow")).toBe("POST");
  });
});

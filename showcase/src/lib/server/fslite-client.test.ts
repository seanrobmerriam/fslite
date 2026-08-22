import { afterEach, describe, expect, it, vi } from "vitest";

import type { ClientDependencies } from "./fslite-client";
import type { ServerConfig } from "./config";
import {
  FsliteClient,
  UpstreamApiError,
  UpstreamResponseTooLargeError,
} from "./fslite-client";
import { validateVirtualPath } from "../shared/path";

const config: ServerConfig = {
  serverUrl: new URL("http://server"),
  token: "super-secret",
  resetIntervalMs: 900_000,
  requestTimeoutMs: 1_000,
  trustProxy: false,
};

function jsonResponse(
  body: unknown,
  status = 200,
  headers?: HeadersInit,
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

function client(
  configOverrides: Partial<ServerConfig> = {},
  dependencies: ClientDependencies = {},
) {
  return new FsliteClient({ ...config, ...configOverrides }, "ws", {
    requestId: () => "visitor-request",
    ...dependencies,
  });
}

function installFetch(response = jsonResponse({ items: [] })) {
  const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("FsliteClient route contracts", () => {
  it("uses identity, root tree, usage, and stat fixed routes", async () => {
    const fetchMock = installFetch(
      jsonResponse({ workspace_id: "ws", capabilities: [] }),
    );
    const api = client();

    await api.identity();
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/me",
      expect.objectContaining({ method: "GET" }),
    );

    await api.tree(validateVirtualPath("/"));
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/directories//tree?limit=250",
      expect.objectContaining({ method: "GET" }),
    );

    await api.usage();
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/usage",
      expect.objectContaining({ method: "GET" }),
    );

    await api.stat(validateVirtualPath("/docs/hello world.md"));
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/docs/hello%20world.md",
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("reads binary only for readFile and writes raw bytes with revisions", async () => {
    const fetchMock = installFetch(
      new Response(new Uint8Array([1, 2, 3]), {
        headers: { "content-type": "application/octet-stream" },
      }),
    );
    const api = client();

    const read = await api.readFile(validateVirtualPath("/a.bin"));
    expect(read.data).toEqual(new Uint8Array([1, 2, 3]));
    expect(read.contentType).toBe("application/octet-stream");
    expect(read.activity.response).toEqual({
      binary: true,
      bytes: 3,
      contentType: "application/octet-stream",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "http://server/v1/workspaces/ws/content/a.bin",
      expect.objectContaining({ method: "GET" }),
    );

    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "node" }));
    const bytes = new TextEncoder().encode("hello");
    await api.writeFile(validateVirtualPath("/a.txt"), bytes, 7);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/content/a.txt?expected_revision=7",
      expect.objectContaining({ method: "PUT", body: bytes }),
    );
  });

  it("uses exact mutation route methods, queries, and wire fields", async () => {
    const fetchMock = installFetch(jsonResponse({ id: "node" }));
    const api = client();
    const source = validateVirtualPath("/from");
    const target = validateVirtualPath("/to");

    await api.mkdir(validateVirtualPath("/docs"), true);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/docs?type=directory",
      expect.objectContaining({
        method: "PUT",
        body: JSON.stringify({
          parents: true,
          exist_ok: false,
          expected_revision: null,
        }),
      }),
    );

    await api.copy(source, target, {
      recursive: true,
      overwrite: true,
      expectedRevision: 3,
    });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/from?action=copy",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          to: "/to",
          recursive: true,
          overwrite: true,
          expected_revision: 3,
        }),
      }),
    );

    await api.move(source, target);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/from?action=move",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          to: "/to",
          recursive: false,
          overwrite: false,
          expected_revision: null,
        }),
      }),
    );

    await api.trash(source, 9);
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/from?action=trash",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ expected_revision: 9 }),
      }),
    );

    await api.remove(source, { recursive: true, expectedRevision: 11 });
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/fs/from?recursive=true&expected_revision=11",
      expect.objectContaining({ method: "DELETE" }),
    );
  });

  it("uses exact trash, search, changes, and reset routes", async () => {
    const fetchMock = installFetch(jsonResponse({ items: [] }));
    const api = client();
    const root = validateVirtualPath("/");

    await api.listTrash();
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/trash?limit=250",
      expect.objectContaining({ method: "GET" }),
    );

    await api.restore("trash-1", validateVirtualPath("/restored"));
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/trash/trash-1/restore",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          destination: "/restored",
          expected_revision: null,
        }),
      }),
    );

    await api.purge("trash-1");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/trash/trash-1",
      expect.objectContaining({ method: "DELETE" }),
    );

    await api.glob("/*.txt");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/search/glob?pattern=%2F*.txt&limit=250",
      expect.objectContaining({ method: "GET" }),
    );

    await api.find(root, "readme");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/search/find",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          query: { root: "/", name_contains: "readme" },
          page: { limit: 250 },
        }),
      }),
    );

    await api.searchContent(root, "snowman ☃");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/search/content",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          root: "/",
          needle_base64: "c25vd21hbiDimIM=",
          page: { limit: 250 },
        }),
      }),
    );

    await api.changes("cursor value");
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/changes?after=cursor%20value&limit=250",
      expect.objectContaining({ method: "GET" }),
    );

    await api.resetWorkspace();
    expect(fetchMock).toHaveBeenLastCalledWith(
      "http://server/v1/workspaces/ws/reset",
      expect.objectContaining({ method: "POST" }),
    );
  });

  it("sends credentials and a visitor request id without exposing either in activity", async () => {
    const fetchMock = installFetch(
      jsonResponse({ workspace_id: "ws", capabilities: [] }, 200, {
        "x-request-id": "upstream-request",
      }),
    );
    const result = await client().identity();
    const [, init] = fetchMock.mock.calls[0] ?? [];
    const headers = new Headers(init?.headers);

    expect(headers.get("authorization")).toBe("Bearer super-secret");
    expect(headers.get("x-request-id")).toBe("visitor-request");
    expect(result.activity.requestId).toBe("upstream-request");
    expect(JSON.stringify(result.activity)).not.toContain("super-secret");
  });

  it("turns JSON error envelopes into typed redacted errors", async () => {
    installFetch(
      jsonResponse(
        {
          error: {
            code: "revision_conflict",
            message: "bad super-secret",
            details: { revision: 4, token: "super-secret" },
          },
        },
        412,
        { "x-request-id": "upstream-request" },
      ),
    );

    await expect(
      client().stat(validateVirtualPath("/a.txt")),
    ).rejects.toMatchObject({
      name: UpstreamApiError.name,
      status: 412,
      code: "revision_conflict",
      requestId: "upstream-request",
      message: expect.not.stringContaining("super-secret"),
      details: { revision: 4, token: "[REDACTED]" },
    });
  });

  it("rejects oversized JSON responses before unbounded buffering", async () => {
    installFetch(jsonResponse({ payload: "x".repeat(1024 * 1024 + 1) }));

    await expect(client().usage()).rejects.toBeInstanceOf(
      UpstreamResponseTooLargeError,
    );
  });

  it("keeps the timeout alive until a successful response body has been read", async () => {
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let bodyController: ReadableStreamDefaultController<Uint8Array> | undefined;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(
      async () =>
        new Response(
          new ReadableStream({
            start(controller) {
              bodyController = controller;
            },
          }),
          { headers: { "content-type": "application/json" } },
        ),
    );
    const request = client({}, { fetch: fetchMock }).usage();

    await Promise.resolve();
    try {
      expect(clearTimeoutSpy).not.toHaveBeenCalled();
      bodyController?.enqueue(
        new TextEncoder().encode('{"workspace_id":"ws"}'),
      );
      bodyController?.close();
      await expect(request).resolves.toMatchObject({
        data: { workspace_id: "ws" },
      });
      expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    } finally {
      bodyController?.error(new Error("test cleanup"));
      await request.catch(() => undefined);
    }
  });

  it("keeps the timeout alive while parsing an upstream JSON error", async () => {
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let bodyController: ReadableStreamDefaultController<Uint8Array> | undefined;
    const fetchMock = vi.fn<typeof fetch>().mockImplementation(
      async () =>
        new Response(
          new ReadableStream({
            start(controller) {
              bodyController = controller;
            },
          }),
          {
            status: 409,
            headers: { "content-type": "application/json" },
          },
        ),
    );
    const request = client({}, { fetch: fetchMock }).stat(
      validateVirtualPath("/a.txt"),
    );

    await Promise.resolve();
    try {
      expect(clearTimeoutSpy).not.toHaveBeenCalled();
      bodyController?.enqueue(
        new TextEncoder().encode(
          '{"error":{"code":"already_exists","message":"exists","details":{}}}',
        ),
      );
      bodyController?.close();
      await expect(request).rejects.toBeInstanceOf(UpstreamApiError);
      expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    } finally {
      bodyController?.error(new Error("test cleanup"));
      await request.catch(() => undefined);
    }
  });

  it("aborts a stalled body after headers and surfaces only a sanitized request error", async () => {
    vi.useFakeTimers();
    const clearTimeoutSpy = vi.spyOn(globalThis, "clearTimeout");
    let signal: AbortSignal | undefined;
    let bodyController: ReadableStreamDefaultController<Uint8Array> | undefined;
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockImplementation(async (_input, init) => {
        signal = init?.signal ?? undefined;
        return new Response(
          new ReadableStream({
            start(controller) {
              bodyController = controller;
              signal?.addEventListener(
                "abort",
                () =>
                  controller.error(
                    new DOMException("super-secret abort", "AbortError"),
                  ),
                { once: true },
              );
            },
          }),
          { headers: { "content-type": "application/json" } },
        );
      });
    const request = client(
      { requestTimeoutMs: 25 },
      { fetch: fetchMock },
    ).usage();
    const settledRequest = request.then(
      () => undefined,
      (error: unknown) => error,
    );

    await vi.advanceTimersByTimeAsync(0);
    try {
      expect(clearTimeoutSpy).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(25);
      expect(signal?.aborted).toBe(true);
      await expect(settledRequest).resolves.toMatchObject({
        name: "UpstreamRequestError",
        requestId: "visitor-request",
        message: expect.not.stringContaining("super-secret"),
      });
      expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);
    } finally {
      bodyController?.error(new Error("test cleanup"));
      await settledRequest;
    }
  });

  it.each([
    [
      "a malformed successful JSON body",
      new Response("{malformed super-secret", {
        headers: { "content-type": "application/json" },
      }),
    ],
    [
      "an unreadable upstream error body",
      new Response(
        new ReadableStream({
          start(controller) {
            controller.error(new Error("super-secret read failure"));
          },
        }),
        { status: 500, headers: { "content-type": "application/json" } },
      ),
    ],
  ])("maps %s to a sanitized request error", async (_description, response) => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(response);

    await expect(
      client({}, { fetch: fetchMock }).usage(),
    ).rejects.toMatchObject({
      name: "UpstreamRequestError",
      requestId: "visitor-request",
      message: expect.not.stringContaining("super-secret"),
    });
  });
});

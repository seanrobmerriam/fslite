import { describe, expect, it, vi } from "vitest";

import { MAX_BROWSER_FILE_BYTES, ShowcaseApi, ShowcaseError } from "./api";

const usage = {
  active_logical_bytes: 1,
  trashed_logical_bytes: 0,
  staged_bytes: 0,
  active_nodes: 1,
  trashed_nodes: 0,
  max_logical_bytes: 10,
  max_nodes: 250,
  max_file_bytes: 1024 * 1024,
};
const workspaceId = "workspace-redacted-from-ui";

const node = {
  workspace_id: workspaceId,
  id: "node-1",
  parent_id: null,
  name: "readme.txt",
  kind: "file",
  logical_size: 5,
  created_at_ms: 1,
  modified_at_ms: 2,
  accessed_at_ms: 3,
  revision: 4,
  attributes: {},
};

const validActivity = {
  id: "activity-1",
  timestamp: "2026-08-22T00:00:00.000Z",
  method: "GET",
  path: "/safe",
  status: 200,
  durationMs: 1,
  requestId: "request-1",
  request: null,
  response: null,
  curl: "curl -X GET /safe",
};

const activityHeaders = new Headers({
  "x-fslite-method": "GET",
  "x-fslite-path": "/v1/workspaces/private/content/report.txt",
  "x-fslite-status": "200",
  "x-fslite-duration-ms": "12",
  "x-request-id": "request-1",
});

describe("ShowcaseApi", () => {
  it("posts only the finite operation payload to the same-origin gateway", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          data: { items: [], next_cursor: null },
          activity: validActivity,
        }),
        { headers: { "content-type": "application/json" } },
      ),
    );
    const api = new ShowcaseApi({ fetch });

    await expect(
      api.operation({ kind: "tree", path: "/" as never }),
    ).resolves.toMatchObject({ data: { items: [] } });

    expect(fetch).toHaveBeenCalledWith(
      "/api/operation",
      expect.objectContaining({
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ kind: "tree", path: "/" }),
      }),
    );
  });

  it("turns the strict public error envelope into a typed error", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          error: {
            code: "revision_conflict",
            message: "The file changed before the operation completed.",
            status: 409,
            requestId: "request-2",
          },
        }),
        { status: 409, headers: { "content-type": "application/json" } },
      ),
    );
    const api = new ShowcaseApi({ fetch });

    await expect(api.operation({ kind: "usage" })).rejects.toEqual(
      expect.objectContaining<Partial<ShowcaseError>>({
        name: "ShowcaseError",
        code: "revision_conflict",
        status: 409,
        requestId: "request-2",
      }),
    );
  });

  it("normalizes a browser network failure into a sanitized availability error", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockRejectedValue(
        new TypeError("request to https://private.example/token failed"),
      );
    const api = new ShowcaseApi({ fetch });

    await expect(api.operation({ kind: "usage" })).rejects.toEqual(
      expect.objectContaining<Partial<ShowcaseError>>({
        name: "ShowcaseError",
        code: "upstream_unavailable",
        status: 502,
        message: "The filesystem service is unavailable.",
      }),
    );

    try {
      await api.operation({ kind: "usage" });
    } catch (error) {
      expect(error).not.toHaveProperty("cause");
      expect(String(error)).not.toContain("private.example");
      expect(String(error)).not.toContain("token");
    }
  });

  it("rejects an oversized upload before making a request", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>();
    const api = new ShowcaseApi({ fetch });
    const file = new File([new Uint8Array(MAX_BROWSER_FILE_BYTES + 1)], "big");

    await expect(api.upload("/big" as never, file)).rejects.toEqual(
      expect.objectContaining<Partial<ShowcaseError>>({
        code: "payload_too_large",
        status: 413,
      }),
    );
    expect(fetch).not.toHaveBeenCalled();
  });

  it("sends a bounded file as a raw same-origin upload body", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      new Response(JSON.stringify({ data: node, activity: validActivity }), {
        headers: { "content-type": "application/json" },
      }),
    );
    const api = new ShowcaseApi({ fetch });
    const file = new File(["hello"], "hello.txt", { type: "text/plain" });

    await api.upload("/docs/hello.txt" as never, file);

    expect(fetch).toHaveBeenCalledWith(
      "/api/upload?path=%2Fdocs%2Fhello.txt",
      expect.objectContaining({
        method: "POST",
        body: file,
        headers: { "content-type": "application/octet-stream" },
      }),
    );
  });

  it("downloads through the query gateway and always removes and revokes its object URL", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValue(
        new Response("contents", { headers: activityHeaders }),
      );
    const createObjectURL = vi.fn(() => "blob:download-1");
    const revokeObjectURL = vi.fn();
    const click = vi.fn(() => {
      throw new Error("synthetic click failure");
    });
    const remove = vi.fn();
    const anchor = {
      href: "",
      download: "",
      click,
      remove,
    } as unknown as HTMLAnchorElement;
    const document = {
      body: { append: vi.fn() },
      createElement: vi.fn(() => anchor),
    } as unknown as Document;
    const api = new ShowcaseApi({
      fetch,
      document: () => document,
      objectUrl: { createObjectURL, revokeObjectURL },
    });

    await expect(api.download("/docs/report\r\n.txt" as never)).rejects.toThrow(
      "synthetic click failure",
    );

    expect(fetch).toHaveBeenCalledWith(
      "/api/download?path=%2Fdocs%2Freport%0D%0A.txt",
      expect.objectContaining({ method: "GET" }),
    );
    expect(anchor.download).toBe("report__.txt");
    expect(remove).toHaveBeenCalledOnce();
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:download-1");
  });

  it("uses the validated route-provided upstream path for download activity", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValue(
        new Response("contents", { headers: activityHeaders }),
      );
    const api = new ShowcaseApi({
      fetch,
      document: () => document,
      objectUrl: {
        createObjectURL: vi.fn(() => "blob:ok"),
        revokeObjectURL: vi.fn(),
      },
    });
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(
      () => undefined,
    );

    const result = await api.download("/docs/report.txt" as never);

    expect(result.activity).toMatchObject({
      method: "GET",
      path: "/v1/workspaces/private/content/report.txt",
      status: 200,
      durationMs: 12,
      requestId: "request-1",
    });
  });

  it("removes every C0 control and DEL from local download activity headers", async () => {
    const response = {
      ok: true,
      status: 200,
      blob: async () => new Blob(["contents"]),
      headers: {
        get: (name: string) =>
          ({
            "x-fslite-method": "GET",
            "x-fslite-status": "200",
            "x-fslite-duration-ms": "12",
            "x-request-id": "request\u0001\u001f\u007fend",
          })[name] ?? null,
      },
    } as unknown as Response;
    const api = new ShowcaseApi({
      fetch: vi.fn<typeof globalThis.fetch>().mockResolvedValue(response),
      document: () => document,
      objectUrl: {
        createObjectURL: vi.fn(() => "blob:controls"),
        revokeObjectURL: vi.fn(),
      },
    });
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(
      () => undefined,
    );

    const result = await api.download("/docs/report.txt" as never);

    expect(result.activity.requestId).toBe("request___end");
    expect(result.activity.path).toBe("/docs/report.txt");
    expect(
      [...result.activity.requestId].some((character) => {
        const code = character.codePointAt(0) ?? 0;
        return code <= 0x1f || code === 0x7f;
      }),
    ).toBe(false);
  });

  it("rejects malformed, extra, and incoherent public envelopes", async () => {
    const api = new ShowcaseApi({
      fetch: vi
        .fn<typeof globalThis.fetch>()
        .mockResolvedValueOnce(
          new Response(JSON.stringify({ ...validActivity, extra: true }), {
            headers: { "content-type": "application/json" },
          }),
        )
        .mockResolvedValueOnce(
          new Response(
            JSON.stringify({
              error: {
                code: "bad",
                message: "no",
                status: 700,
                extra: true,
              },
            }),
            { status: 502, headers: { "content-type": "application/json" } },
          ),
        )
        .mockResolvedValueOnce(
          new Response(
            JSON.stringify({
              ready: true,
              generation: 1,
              resetting: false,
              nextResetAt: 1,
              now: 2,
              usage: { ...usage, extra: true },
            }),
            { headers: { "content-type": "application/json" } },
          ),
        ),
    });

    await expect(api.operation({ kind: "usage" })).rejects.toMatchObject({
      code: "invalid_response",
    });
    await expect(api.operation({ kind: "usage" })).rejects.toMatchObject({
      code: "invalid_response",
    });
    await expect(api.status()).rejects.toMatchObject({
      code: "invalid_response",
    });
  });

  it("validates each operation against its expected response data shape", async () => {
    const operations = [
      { kind: "tree", path: "/" },
      { kind: "read_file", path: "/readme.txt" },
      { kind: "write_file", path: "/readme.txt", text: "next" },
      { kind: "mkdir", path: "/docs", parents: false },
      { kind: "copy", from: "/a", to: "/b", recursive: false },
      { kind: "move", from: "/a", to: "/b" },
      { kind: "trash", path: "/a" },
      { kind: "remove", path: "/a", recursive: false, confirmedPath: "/a" },
      { kind: "list_trash" },
      {
        kind: "restore",
        trashId: "0180c914-c06f-7ea1-8f12-123456789abc",
      },
      {
        kind: "purge",
        trashId: "0180c914-c06f-7ea1-8f12-123456789abc",
        confirmedName: "a",
      },
      { kind: "glob", pattern: "/**" },
      { kind: "find", root: "/", nameContains: "a" },
      { kind: "search_content", root: "/", text: "a" },
      { kind: "changes" },
      { kind: "usage" },
    ] as const;
    const fetch = vi.fn<typeof globalThis.fetch>();
    for (let index = 0; index < operations.length; index += 1) {
      fetch.mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            data: { unexpected: true },
            activity: validActivity,
          }),
          { headers: { "content-type": "application/json" } },
        ),
      );
    }
    const api = new ShowcaseApi({ fetch });

    for (const operation of operations) {
      await expect(api.operation(operation as never)).rejects.toMatchObject({
        code: "invalid_response",
      });
    }
  });

  it("accepts strict status and write-file node responses", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            ready: true,
            generation: 1,
            resetting: false,
            nextResetAt: 100,
            now: 1,
            usage,
          }),
          { headers: { "content-type": "application/json" } },
        ),
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ data: node, activity: validActivity }), {
          headers: { "content-type": "application/json" },
        }),
      );
    const api = new ShowcaseApi({ fetch });

    await expect(api.status()).resolves.toMatchObject({ usage });
    await expect(
      api.operation({
        kind: "write_file",
        path: "/readme.txt" as never,
        text: "next",
      }),
    ).resolves.toMatchObject({ data: { revision: 4 } });
  });
});

import { describe, expect, it, vi } from "vitest";

import { MAX_BROWSER_FILE_BYTES, ShowcaseApi, ShowcaseError } from "./api";

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
          data: { items: [] },
          activity: { id: "a", method: "GET", path: "/safe", status: 200 },
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

  it("turns a gateway network failure into a typed error", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockRejectedValue(new TypeError("network down"));
    const api = new ShowcaseApi({ fetch });

    await expect(api.operation({ kind: "usage" })).rejects.toEqual(
      expect.objectContaining<Partial<ShowcaseError>>({
        name: "ShowcaseError",
        code: "network_error",
        status: 502,
      }),
    );
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
      new Response(JSON.stringify({ data: {}, activity: { id: "upload" } }), {
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

  it("derives a bounded local activity without trusting upstream path or control headers", async () => {
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
      path: "/api/download",
      status: 200,
      durationMs: 12,
      requestId: "request-1",
    });
    expect(JSON.stringify(result.activity)).not.toContain("workspaces");
  });
});

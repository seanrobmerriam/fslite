import { describe, expect, it, vi } from "vitest";

import { GatewayRateLimitError, ShowcaseGateway } from "./gateway";
import { RollingWindowRateLimiter } from "./rate-limit";
import type { ActivityRecord } from "../shared/contracts";
import { validateVirtualPath } from "../shared/path";

const activity: ActivityRecord = {
  id: "activity-1",
  timestamp: "2026-08-22T00:00:00.000Z",
  method: "GET",
  path: "/fixed-route",
  status: 200,
  durationMs: 1,
  requestId: "request-1",
  request: null,
  response: null,
  curl: "curl",
};
const trashId = "019fbe44-865f-7222-bcfb-78895800892b";
const internalListActivity: ActivityRecord = {
  ...activity,
  id: "activity-list",
};

function upstream(data: unknown = { result: true }) {
  return { data, activity };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function client() {
  return {
    tree: vi.fn().mockResolvedValue(upstream({ items: [] })),
    readFile: vi.fn().mockResolvedValue(upstream(new Uint8Array([1]))),
    writeFile: vi.fn().mockResolvedValue(upstream({ id: "file" })),
    mkdir: vi.fn().mockResolvedValue(upstream({ id: "directory" })),
    copy: vi.fn().mockResolvedValue(upstream({ id: "copy" })),
    move: vi.fn().mockResolvedValue(upstream({ id: "move" })),
    trash: vi.fn().mockResolvedValue(upstream({ id: "trash" })),
    remove: vi.fn().mockResolvedValue(upstream(undefined)),
    listTrash: vi.fn().mockResolvedValue({
      data: { items: [{ id: trashId, node: { name: "readme.txt" } }] },
      activity: internalListActivity,
    }),
    restore: vi.fn().mockResolvedValue(upstream({ id: "restore" })),
    purge: vi.fn().mockResolvedValue(upstream(undefined)),
    glob: vi.fn().mockResolvedValue(upstream({ items: [] })),
    find: vi.fn().mockResolvedValue(upstream({ items: [] })),
    searchContent: vi.fn().mockResolvedValue(upstream({ items: [] })),
    changes: vi.fn().mockResolvedValue(upstream({ items: [] })),
    usage: vi.fn().mockResolvedValue(upstream({ active_nodes: 1 })),
  };
}

describe("ShowcaseGateway", () => {
  it("dispatches every finite public operation to one fixed client method", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);
    const path = validateVirtualPath("/docs/readme.txt");
    const cases: Array<[unknown, keyof typeof api]> = [
      [{ kind: "tree", path }, "tree"],
      [{ kind: "read_file", path }, "readFile"],
      [
        { kind: "write_file", path, text: "hello", expectedRevision: 3 },
        "writeFile",
      ],
      [{ kind: "mkdir", path, parents: true }, "mkdir"],
      [
        {
          kind: "copy",
          from: path,
          to: "/copy.txt",
          recursive: true,
          expectedRevision: 3,
        },
        "copy",
      ],
      [
        { kind: "move", from: path, to: "/moved.txt", expectedRevision: 3 },
        "move",
      ],
      [{ kind: "trash", path, expectedRevision: 3 }, "trash"],
      [
        {
          kind: "remove",
          path,
          recursive: true,
          confirmedPath: path,
          expectedRevision: 3,
        },
        "remove",
      ],
      [{ kind: "list_trash" }, "listTrash"],
      [
        {
          kind: "restore",
          trashId,
          destination: "/restored.txt",
          expectedRevision: 3,
        },
        "restore",
      ],
      [{ kind: "purge", trashId, confirmedName: "readme.txt" }, "purge"],
      [{ kind: "glob", pattern: "/**/*.txt" }, "glob"],
      [{ kind: "find", root: "/", nameContains: "readme" }, "find"],
      [{ kind: "search_content", root: "/", text: "needle" }, "searchContent"],
      [{ kind: "changes", after: "cursor" }, "changes"],
      [{ kind: "usage" }, "usage"],
    ];

    for (const [operation, method] of cases) {
      const result = await gateway.execute(operation, "203.0.113.1");
      expect(result).toMatchObject({ data: expect.anything() });
      expect(Object.keys(result)).toEqual(["data", "activity"]);
      expect(api[method]).toHaveBeenCalledTimes(1);
    }
  });

  it("encodes write text and forwards only allowlisted arguments", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    await gateway.execute(
      {
        kind: "write_file",
        path: "/hello.txt",
        text: "snowman ☃",
        expectedRevision: 4,
      },
      "203.0.113.1",
    );

    expect(api.writeFile).toHaveBeenCalledWith(
      validateVirtualPath("/hello.txt"),
      new TextEncoder().encode("snowman ☃"),
      4,
    );
  });

  it("forwards optimistic revisions for copy, move, remove, and restore", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    await gateway.execute(
      {
        kind: "copy",
        from: "/a",
        to: "/b",
        recursive: false,
        expectedRevision: 7,
      },
      "203.0.113.1",
    );
    await gateway.execute(
      { kind: "move", from: "/a", to: "/b", expectedRevision: 8 },
      "203.0.113.1",
    );
    await gateway.execute(
      {
        kind: "remove",
        path: "/a",
        recursive: true,
        confirmedPath: "/a",
        expectedRevision: 9,
      },
      "203.0.113.1",
    );
    await gateway.execute(
      { kind: "restore", trashId, expectedRevision: 10 },
      "203.0.113.1",
    );

    expect(api.copy).toHaveBeenCalledWith("/a", "/b", {
      recursive: false,
      expectedRevision: 7,
    });
    expect(api.move).toHaveBeenCalledWith("/a", "/b", {
      expectedRevision: 8,
    });
    expect(api.remove).toHaveBeenCalledWith("/a", {
      recursive: true,
      expectedRevision: 9,
    });
    expect(api.restore).toHaveBeenCalledWith(trashId, undefined, 10);
  });

  it("coalesces status usage upstream while charging the shared per-IP read bucket", async () => {
    let now = 1_000;
    const api = client();
    const pending = deferred<ReturnType<typeof upstream>>();
    api.usage.mockReturnValueOnce(pending.promise);
    const limiter = new RollingWindowRateLimiter({ now: () => now });
    const gateway = new ShowcaseGateway(api, limiter, {
      now: () => now,
      statusCacheMs: 1_000,
    });

    const first = gateway.statusUsage("203.0.113.1");
    const second = gateway.statusUsage("203.0.113.1");
    expect(api.usage).toHaveBeenCalledTimes(1);
    pending.resolve(upstream({ active_nodes: 2 }));
    await expect(Promise.all([first, second])).resolves.toEqual([
      { active_nodes: 2 },
      { active_nodes: 2 },
    ]);

    await gateway.execute({ kind: "tree", path: "/" }, "203.0.113.1");
    for (let count = 3; count < 120; count += 1) {
      await gateway.statusUsage("203.0.113.1");
    }
    await expect(gateway.download("/a", "203.0.113.1")).rejects.toMatchObject({
      bucket: "read",
    });
    expect(api.usage).toHaveBeenCalledTimes(1);

    now += 61_000;
    await gateway.statusUsage("203.0.113.1");
    expect(api.usage).toHaveBeenCalledTimes(2);
  });

  it("never exposes reset or workspace lifecycle dispatch", async () => {
    const api = { ...client(), resetWorkspace: vi.fn() };
    const gateway = new ShowcaseGateway(api);

    await expect(
      gateway.execute({ kind: "reset" }, "203.0.113.1"),
    ).rejects.toThrow();
    expect(api.resetWorkspace).not.toHaveBeenCalled();
  });

  it("returns exactly the one sanitized activity supplied by the client", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);
    const result = await gateway.execute({ kind: "usage" }, "203.0.113.1");

    expect(Object.keys(result)).toEqual(["data", "activity"]);
    expect(result.activity).toBe(activity);
  });

  it("validates and rate-limits a binary download while preserving upstream bytes, activity, and content type", async () => {
    const api = client();
    const binary = {
      data: new Uint8Array([0, 255, 7]),
      activity,
      contentType: "application/octet-stream",
    };
    api.readFile.mockResolvedValue(binary);
    const gateway = new ShowcaseGateway(api);

    await expect(
      gateway.download("/docs/archive.bin", "203.0.113.1"),
    ).resolves.toBe(binary);
    expect(api.readFile).toHaveBeenCalledWith(
      validateVirtualPath("/docs/archive.bin"),
    );

    await expect(
      gateway.download("../private.bin", "203.0.113.1"),
    ).rejects.toThrow("path must be canonical and absolute");
    expect(api.readFile).toHaveBeenCalledTimes(1);
  });

  it("shares the 120-per-minute read bucket with JSON read operations", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    for (let request = 0; request < 119; request += 1) {
      await gateway.execute({ kind: "tree", path: "/examples" }, "203.0.113.1");
    }
    await gateway.download("/examples/hello.txt", "203.0.113.1");

    await expect(
      gateway.download("/examples/hello.txt", "203.0.113.1"),
    ).rejects.toMatchObject({ bucket: "read" });
    expect(api.tree).toHaveBeenCalledTimes(119);
    expect(api.readFile).toHaveBeenCalledTimes(1);
  });

  it("confirms the current trash entry name before purging without returning lookup activity", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    const result = await gateway.execute(
      { kind: "purge", trashId, confirmedName: "readme.txt" },
      "203.0.113.1",
    );

    expect(api.listTrash).toHaveBeenCalledTimes(1);
    expect(api.purge).toHaveBeenCalledWith(trashId);
    expect(result).toEqual({ data: expect.anything(), activity });
    expect(result.activity).not.toBe(internalListActivity);
  });

  it("rejects a mismatched purge confirmation without calling purge", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    await expect(
      gateway.execute(
        { kind: "purge", trashId, confirmedName: "other.txt" },
        "203.0.113.1",
      ),
    ).rejects.toThrow("Purge confirmation did not match");
    expect(api.listTrash).toHaveBeenCalledTimes(1);
    expect(api.purge).not.toHaveBeenCalled();
  });

  it("rejects a missing trash entry without calling purge", async () => {
    const api = client();
    api.listTrash.mockResolvedValueOnce({
      data: { items: [] },
      activity: internalListActivity,
    });
    const gateway = new ShowcaseGateway(api);

    await expect(
      gateway.execute(
        { kind: "purge", trashId, confirmedName: "readme.txt" },
        "203.0.113.1",
      ),
    ).rejects.toThrow("Purge confirmation did not match");
    expect(api.purge).not.toHaveBeenCalled();
  });

  it("forwards a validated absolute glob pattern unchanged", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    await gateway.execute(
      { kind: "glob", pattern: "/docs/**/target?.txt" },
      "203.0.113.1",
    );

    expect(api.glob).toHaveBeenCalledWith("/docs/**/target?.txt");
  });

  it("enforces the mutation window before upstream dispatch", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);
    const operation = {
      kind: "mkdir" as const,
      path: "/limited",
      parents: false,
    };

    for (let request = 0; request < 30; request += 1) {
      await gateway.execute(operation, "203.0.113.1");
    }

    await expect(
      gateway.execute(operation, "203.0.113.1"),
    ).rejects.toBeInstanceOf(GatewayRateLimitError);
    expect(api.mkdir).toHaveBeenCalledTimes(30);
  });

  it("applies upload and mutation limits together to raw uploads", async () => {
    const api = client();
    const gateway = new ShowcaseGateway(api);

    for (let request = 0; request < 10; request += 1) {
      await gateway.upload(
        "/upload.txt",
        new Uint8Array([request]),
        "203.0.113.1",
      );
    }

    await expect(
      gateway.upload("/upload.txt", new Uint8Array([11]), "203.0.113.1"),
    ).rejects.toMatchObject({ bucket: "upload" });
    expect(api.writeFile).toHaveBeenCalledTimes(10);

    const mutation = {
      kind: "mkdir" as const,
      path: "/after-upload",
      parents: false,
    };
    for (let request = 0; request < 20; request += 1) {
      await gateway.execute(mutation, "203.0.113.1");
    }
    await expect(
      gateway.execute(mutation, "203.0.113.1"),
    ).rejects.toMatchObject({ bucket: "mutation" });
  });
});

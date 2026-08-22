import { describe, expect, it, vi } from "vitest";

import { GatewayRateLimitError, ShowcaseGateway } from "./gateway";
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

function upstream(data: unknown = { result: true }) {
  return { data, activity };
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
    listTrash: vi.fn().mockResolvedValue(upstream({ items: [] })),
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
      [{ kind: "copy", from: path, to: "/copy.txt", recursive: true }, "copy"],
      [{ kind: "move", from: path, to: "/moved.txt" }, "move"],
      [{ kind: "trash", path, expectedRevision: 3 }, "trash"],
      [
        { kind: "remove", path, recursive: true, confirmedPath: path },
        "remove",
      ],
      [{ kind: "list_trash" }, "listTrash"],
      [
        { kind: "restore", trashId: "trash-1", destination: "/restored.txt" },
        "restore",
      ],
      [
        { kind: "purge", trashId: "trash-1", confirmedName: "readme.txt" },
        "purge",
      ],
      [{ kind: "glob", pattern: "/**/*.txt" }, "glob"],
      [{ kind: "find", root: "/", nameContains: "readme" }, "find"],
      [{ kind: "search_content", root: "/", text: "needle" }, "searchContent"],
      [{ kind: "changes", after: "cursor" }, "changes"],
      [{ kind: "usage" }, "usage"],
    ];

    for (const [operation, method] of cases) {
      const result = await gateway.execute(operation, "203.0.113.1");
      expect(result).toEqual({ data: expect.anything(), activity });
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

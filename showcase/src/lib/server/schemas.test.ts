import { describe, expect, it } from "vitest";

import { parsePublicOperation } from "./schemas";

const path = "/docs/readme.txt";

const acceptedOperations = [
  { kind: "tree", path },
  { kind: "read_file", path },
  { kind: "write_file", path, text: "hello", expectedRevision: 1 },
  { kind: "mkdir", path: "/docs/new", parents: true },
  { kind: "copy", from: path, to: "/docs/copy.txt", recursive: false },
  { kind: "move", from: path, to: "/docs/moved.txt" },
  { kind: "trash", path, expectedRevision: 2 },
  { kind: "remove", path, recursive: false, confirmedPath: path },
  { kind: "remove", path, recursive: true, confirmedPath: path },
  { kind: "list_trash" },
  { kind: "restore", trashId: "trash-1", destination: "/restored.txt" },
  { kind: "purge", trashId: "trash-1", confirmedName: "readme.txt" },
  { kind: "glob", pattern: "/**/*.txt" },
  { kind: "find", root: "/", nameContains: "readme" },
  { kind: "search_content", root: "/", text: "needle" },
  { kind: "changes", after: "cursor-1" },
  { kind: "usage" },
] as const;

describe("parsePublicOperation", () => {
  it.each(acceptedOperations)("accepts the $kind operation", (operation) => {
    expect(parsePublicOperation(operation)).toMatchObject(operation);
  });

  it.each([
    { kind: "reset" },
    { kind: "create_workspace" },
    { kind: "delete_workspace" },
    { kind: "request", method: "DELETE", url: "https://example.test" },
  ])("rejects an unallowlisted operation %#", (operation) => {
    expect(() => parsePublicOperation(operation)).toThrow();
  });

  it.each([
    { kind: "tree", path, method: "DELETE" },
    { kind: "tree", path, url: "https://example.test/v1/workspaces/private" },
    { kind: "tree", path, workspaceId: "another-workspace" },
    { kind: "read_file", path, headers: { authorization: "Bearer private" } },
  ])("rejects request-routing input %#", (operation) => {
    expect(() => parsePublicOperation(operation)).toThrow();
  });

  it.each([
    { kind: "tree", path: "docs/readme.txt" },
    { kind: "tree", path: "/docs/../private" },
    { kind: "tree", path: "/docs//private" },
    { kind: "tree", path: "/docs/" },
    { kind: "tree", path: "/docs/\0private" },
  ])("rejects unsafe virtual path %#", (operation) => {
    expect(() => parsePublicOperation(operation)).toThrow();
  });

  it.each([
    { kind: "write_file", path, text: "ok", expectedRevision: 0 },
    { kind: "trash", path, expectedRevision: 0 },
    { kind: "write_file", path, text: "x".repeat(1024 * 1024 + 1) },
    { kind: "search_content", root: "/", text: "x".repeat(1024 * 1024 + 1) },
  ])("rejects unsafe revision or oversized text %#", (operation) => {
    expect(() => parsePublicOperation(operation)).toThrow();
  });

  it("measures text input using UTF-8 bytes", () => {
    expect(() =>
      parsePublicOperation({
        kind: "write_file",
        path,
        text: "☃".repeat(Math.ceil((1024 * 1024) / 3)),
      }),
    ).toThrow();
  });

  it("requires the exact path confirmation for recursive remove", () => {
    expect(() =>
      parsePublicOperation({
        kind: "remove",
        path: "/docs",
        recursive: true,
        confirmedPath: "/",
      }),
    ).toThrow();
  });
});

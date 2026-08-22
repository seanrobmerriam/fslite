import { describe, expect, it } from "vitest";

import { parsePublicOperation } from "./schemas";

const path = "/docs/readme.txt";
const trashId = "019fbe44-865f-7222-bcfb-78895800892b";

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
  { kind: "restore", trashId, destination: "/restored.txt" },
  { kind: "purge", trashId, confirmedName: "readme.txt" },
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

  it.each([
    "trash-1",
    ".",
    "..",
    "%2Ftrash",
    "/019fbe44-865f-7222-bcfb-78895800892b",
    "019fbe44865f7222bcfb78895800892b",
    "019FBE44-865F-7222-BCFB-78895800892B",
    "550e8400-e29b-41d4-a716-446655440000",
    "019fbe44-865f-6222-bcfb-78895800892b",
    "019fbe44-865f-7222-7cfb-78895800892b",
  ])("rejects noncanonical or non-v7 trash ID %s", (invalidTrashId) => {
    expect(() =>
      parsePublicOperation({
        kind: "restore",
        trashId: invalidTrashId,
      }),
    ).toThrow();
  });

  it.each(["/", "/*.txt", "/docs/**/target?.txt", "/literal-[brackets].txt"])(
    "accepts safe absolute glob pattern %s",
    (pattern) => {
      expect(parsePublicOperation({ kind: "glob", pattern })).toMatchObject({
        kind: "glob",
        pattern,
      });
    },
  );

  it.each([
    "docs/*.txt",
    "/docs/../*.txt",
    "/docs/./*.txt",
    "/docs//*.txt",
    "/docs/",
    "/docs/\0*.txt",
    "/docs/\u0001*.txt",
  ])("rejects unsafe or noncanonical glob pattern %s", (pattern) => {
    expect(() => parsePublicOperation({ kind: "glob", pattern })).toThrow();
  });
});

import { describe, expect, it } from "vitest";

import { encodeVirtualPath, validateVirtualPath } from "./path";

describe("virtual paths", () => {
  it("accepts root and nested Unicode names", () => {
    expect(validateVirtualPath("/")).toBe("/");
    expect(validateVirtualPath("/文書/étoile.txt")).toBe("/文書/étoile.txt");
  });

  it("encodes each canonical virtual-path segment", () => {
    expect(encodeVirtualPath(validateVirtualPath("/docs/hello world.md"))).toBe(
      "docs/hello%20world.md",
    );
    expect(encodeVirtualPath(validateVirtualPath("/"))).toBe("");
  });

  it.each(["docs", "/a//b", "/a/../b", "/a/./b", "/a\0b", "/a/"])(
    "rejects %s",
    (path) => {
      expect(() => validateVirtualPath(path)).toThrow();
    },
  );
});

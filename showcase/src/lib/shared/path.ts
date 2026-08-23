export type VirtualPath = string & { readonly __virtualPath: unique symbol };

/** Validates the canonical, absolute virtual paths accepted by fslite. */
export function validateVirtualPath(value: string): VirtualPath {
  if (
    !value.startsWith("/") ||
    value.includes("\0") ||
    value.includes("//") ||
    (value !== "/" && value.endsWith("/"))
  ) {
    throw new Error("path must be canonical and absolute");
  }

  const segments = value.split("/").slice(1);
  if (segments.some((segment) => segment === "." || segment === "..")) {
    throw new Error("path may not contain traversal segments");
  }

  return value as VirtualPath;
}

/** Validates the absolute, canonical glob grammar accepted by the public gateway. */
export function validateGlobPattern(value: string): string {
  if (value.length === 0 || value.length > 1024 || !value.startsWith("/")) {
    throw new Error(
      "glob pattern must be absolute and no longer than 1024 characters",
    );
  }
  if (
    [...value].some((character) => {
      const code = character.codePointAt(0) ?? 0;
      return code < 0x20 || code === 0x7f;
    })
  ) {
    throw new Error("glob pattern may not contain control characters");
  }
  if (value === "/") return value;
  const segments = value.split("/").slice(1);
  if (
    segments.some(
      (segment) => segment === "" || segment === "." || segment === "..",
    )
  ) {
    throw new Error("glob pattern must use canonical path segments");
  }
  return value;
}

/** Encodes canonical path segments without encoding the separating slashes. */
export function encodeVirtualPath(path: VirtualPath): string {
  return path.split("/").slice(1).map(encodeURIComponent).join("/");
}

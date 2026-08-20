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

/** Encodes canonical path segments without encoding the separating slashes. */
export function encodeVirtualPath(path: VirtualPath): string {
  return path.split("/").slice(1).map(encodeURIComponent).join("/");
}

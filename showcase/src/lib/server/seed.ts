import type { VirtualPath } from "../shared/path";
import { validateVirtualPath } from "../shared/path";

export interface SeedDirectory {
  kind: "directory";
  path: VirtualPath;
}

export interface SeedFile {
  kind: "file";
  path: VirtualPath;
  text: string;
}

export type SeedEntry = SeedDirectory | SeedFile;

export interface SeedClient {
  mkdir(path: VirtualPath, parents: boolean): Promise<unknown>;
  writeFile(path: VirtualPath, bytes: Uint8Array): Promise<unknown>;
}

const README = `# fslite filesystem showcase

This shared workspace resets to this small starter tree every fifteen minutes.

- Browse the tree and open files to inspect their content.
- Create, move, copy, download, or send files to trash while exploring.
- Try name and content search against the seeded examples.
- The reset lifecycle restores this tree, so changes are intentionally temporary.

The browser talks to narrow same-origin endpoints; the filesystem service stays private.`;

const HTTP_API = `# HTTP API examples

The showcase uses fixed filesystem operations for tree, content, search, trash,
and change-history requests. The examples are intentionally small so you can
inspect the resulting API activity without filling the shared workspace.`;

/** The order is intentional: parent directories exist before any file write. */
export const SEED_ENTRIES = [
  { kind: "directory", path: validateVirtualPath("/docs") },
  { kind: "directory", path: validateVirtualPath("/examples") },
  {
    kind: "file",
    path: validateVirtualPath("/README.md"),
    text: README,
  },
  {
    kind: "file",
    path: validateVirtualPath("/docs/http-api.md"),
    text: HTTP_API,
  },
  {
    kind: "file",
    path: validateVirtualPath("/examples/hello.txt"),
    text: "Hello from the shared fslite workspace.\n",
  },
  {
    kind: "file",
    path: validateVirtualPath("/examples/metadata.json"),
    text: `${JSON.stringify(
      {
        name: "fslite showcase",
        description: "Small deterministic data for tree and search examples.",
        reset: "This file is restored with the rest of the starter tree.",
      },
      null,
      2,
    )}\n`,
  },
] as const satisfies readonly SeedEntry[];

/** Creates the public starter tree one deterministic upstream request at a time. */
export async function seedWorkspace(client: SeedClient): Promise<void> {
  for (const entry of SEED_ENTRIES) {
    if (entry.kind === "directory") {
      await client.mkdir(entry.path, true);
    } else {
      await client.writeFile(entry.path, new TextEncoder().encode(entry.text));
    }
  }
}

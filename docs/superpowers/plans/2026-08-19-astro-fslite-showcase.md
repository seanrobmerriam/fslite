# Astro fslite Showcase Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-hosted public Astro file explorer that demonstrates fslite's real HTTP API while keeping credentials private and resetting a shared seeded workspace every 15 minutes.

**Architecture:** Astro runs as a standalone Node SSR gateway behind Caddy and talks to `fslite-server` on Docker's private network. A React island renders the tree/editor experience; narrow Astro endpoints accept a finite typed operation set and return sanitized upstream request metadata for the API activity panel.

**Tech Stack:** Node 22.12+, pnpm 10.12.4, Astro 7.2.4, `@astrojs/node` 11.1.4, `@astrojs/react` 6.0.4, React 19.2.8, TypeScript 7.0.2, Zod 4.4.3, Lucide React 1.33.0, Vitest 4.1.11, Testing Library 16.3.2, Playwright 1.62.1, Docker, Caddy.

## Global Constraints

- Source lives under `showcase/`; Cargo packages never include it.
- Caddy exposes Astro only; `fslite-server` has no public host port.
- Never return or log the upstream bearer token; activity and curl examples use `$FSLITE_TOKEN` and `$FSLITE_SERVER_URL`.
- Support tree, read/edit, create file/directory, upload/download, rename/move, copy, trash/restore/purge, permanent delete, glob/find/content search, and changes.
- Deletion uses trash by default; permanent delete and purge require named confirmation.
- Accept at most 1 MiB per file, 10 MiB per workspace, and 250 nodes.
- Default per-IP rolling-minute limits are 120 reads, 30 mutations, and 10 uploads.
- Reset on Node startup and every 15 minutes; block new operations during reset and never replay mutations automatically.
- Refresh the shared tree every 10 seconds without adding background traffic to API activity.
- Visual direction is clean editorial: bright neutral surfaces, restrained blue accent, sans-serif UI, monospace paths/API only.
- Use a plain accessible text area, not a heavy code-editor dependency.
- One Astro Node process is the supported initial deployment topology.
- Do not push images, alter the user's live Compose host, or deploy Caddy during implementation.

---

## File Map

### Application foundation

- Create `showcase/package.json`, `pnpm-lock.yaml`, `astro.config.mjs`, `tsconfig.json`, `vitest.config.ts`, `playwright.config.ts`, `eslint.config.js`, `.prettierrc.mjs`, and `.env.example`.
- Create `showcase/src/layouts/Layout.astro`, `src/pages/index.astro`, and `src/styles/global.css`.
- Create `showcase/src/lib/shared/contracts.ts`, `src/lib/shared/path.ts`, and
  `src/test/setup.ts`.

### Private server runtime

- Create `showcase/src/lib/server/config.ts`: server-only environment and secret-file resolution.
- Create `showcase/src/lib/server/activity.ts`: bounded sanitized upstream records and curl rendering.
- Create `showcase/src/lib/server/fslite-client.ts`: typed upstream HTTP client.
- Create `showcase/src/lib/server/schemas.ts`: finite public operation schemas.
- Create `showcase/src/lib/server/rate-limit.ts`: in-memory per-IP rolling windows.
- Create `showcase/src/lib/server/reset-coordinator.ts`: operation/reset gate and schedule.
- Create `showcase/src/lib/server/seed.ts`: deterministic seed manifest.
- Create `showcase/src/lib/server/runtime.ts`: process singleton and readiness.

### Browser gateway and interface

- Create `showcase/src/pages/api/status.ts`, `health/live.ts`, `health/ready.ts`, `operation.ts`, `upload.ts`, and `download.ts`.
- Create `showcase/src/lib/browser/api.ts`, `reducer.ts`, and `use-showcase.ts`.
- Create focused React components under `showcase/src/components/explorer/` for status, tree, editor, dialogs, search, trash, changes, and activity.
- Create unit/component tests beside modules as `*.test.ts`/`*.test.tsx`.
- Create Playwright tests under `showcase/e2e/`.

### Containers and documentation

- Create `showcase/Dockerfile`, `showcase/docker-entrypoint.sh`, `deploy/showcase/compose.yml`, `deploy/showcase/Caddyfile`, `deploy/showcase/fslite-token.example`, and `deploy/showcase/README.md`.
- Modify root `README.md` and `.gitignore`.

---

### Task 1: Astro SSR and Typed Foundation

**Files:**
- Create: `showcase/package.json`
- Create: `showcase/astro.config.mjs`
- Create: `showcase/tsconfig.json`
- Create: `showcase/vitest.config.ts`
- Create: `showcase/eslint.config.js`
- Create: `showcase/.prettierrc.mjs`
- Create: `showcase/.env.example`
- Create: `showcase/src/layouts/Layout.astro`
- Create: `showcase/src/pages/index.astro`
- Create: `showcase/src/styles/global.css`
- Create: `showcase/src/lib/shared/contracts.ts`
- Create: `showcase/src/lib/shared/path.ts`
- Create: `showcase/src/test/setup.ts`
- Test: `showcase/src/lib/shared/path.test.ts`

**Interfaces:**
- Produces: `Node`, `TreeEntry`, `TrashEntry`, `Change`, `WorkspaceUsage`, `ActivityRecord`, `GatewayResult<T>`, `VirtualPath`, `validateVirtualPath`, and `encodeVirtualPath` for every later task.

- [ ] **Step 1: Create the pinned Astro package and tool configuration**

Use package scripts:

```json
{
  "name": "fslite-showcase",
  "private": true,
  "type": "module",
  "packageManager": "pnpm@10.12.4",
  "engines": { "node": ">=22.12.0" },
  "scripts": {
    "dev": "astro dev",
    "build": "astro check && astro build",
    "start": "node ./dist/server/entry.mjs",
    "check": "astro check && eslint . && prettier --check .",
    "test": "vitest run",
    "test:e2e": "playwright test"
  }
}
```

Pin the stack versions from the plan header, `@astrojs/check 0.9.10`, and testing dependencies:
`@testing-library/dom 10.4.1`, `@testing-library/jest-dom 7.0.1`,
`@testing-library/user-event 14.6.5`, `jsdom 30.0.1`, `eslint 10.8.1`,
`typescript-eslint 8.67.0`, `prettier 3.9.6`, and
`prettier-plugin-astro 0.14.1`. Configure Astro with React and
`node({ mode: "standalone" })`. Configure Vitest for `jsdom`, globals, and
`src/test/setup.ts`; that setup imports `@testing-library/jest-dom/vitest` and
runs Testing Library cleanup after every test.

- [ ] **Step 2: Write path validation tests**

Cover root, nested Unicode names, segment encoding, missing leading slash,
NUL, empty segments from `//`, and `.`/`..` traversal:

```ts
it("encodes each canonical virtual-path segment", () => {
  expect(encodeVirtualPath("/docs/hello world.md")).toBe("docs/hello%20world.md");
  expect(encodeVirtualPath("/")).toBe("");
});

it.each(["docs", "/a//b", "/a/../b", "/a\0b"])("rejects %s", (path) => {
  expect(() => validateVirtualPath(path)).toThrow();
});
```

- [ ] **Step 3: Implement shared wire contracts and path helpers**

Model Rust's serialized fields exactly. Brand validated paths:

```ts
export type VirtualPath = string & { readonly __virtualPath: unique symbol };

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

export function encodeVirtualPath(path: VirtualPath): string {
  return path.split("/").slice(1).map(encodeURIComponent).join("/");
}
```

Define `ActivityRecord` with `id`, `timestamp`, `method`, `path`, `status`,
`durationMs`, `requestId`, `request`, `response`, and `curl`; never include a
headers map in the browser type.

- [ ] **Step 4: Add the minimal SSR page and design tokens**

Create `Layout.astro` with title, description, viewport, and global CSS. The
initial page renders a semantic `<main>`, hero copy, and a placeholder
`<section aria-label="Filesystem showcase">`. Define CSS custom properties for
white/blue/slate surfaces, 8px spacing rhythm, focus rings, and max content
width; do not build explorer components yet.

- [ ] **Step 5: Install and verify the foundation**

Run:

```bash
corepack pnpm --dir showcase install
corepack pnpm --dir showcase test
corepack pnpm --dir showcase build
corepack pnpm --dir showcase check
```

Expected: path tests pass and Astro builds the standalone Node entry.

- [ ] **Step 6: Commit the showcase foundation**

```bash
git add -- showcase/package.json showcase/pnpm-lock.yaml showcase/astro.config.mjs showcase/tsconfig.json showcase/vitest.config.ts showcase/eslint.config.js showcase/.prettierrc.mjs showcase/.env.example showcase/src
git commit -m "feat(showcase): scaffold Astro application"
```

### Task 2: Sanitized Typed fslite Client

**Files:**
- Create: `showcase/src/lib/server/config.ts`
- Create: `showcase/src/lib/server/activity.ts`
- Create: `showcase/src/lib/server/fslite-client.ts`
- Test: `showcase/src/lib/server/config.test.ts`
- Test: `showcase/src/lib/server/activity.test.ts`
- Test: `showcase/src/lib/server/fslite-client.test.ts`

**Interfaces:**
- Consumes: shared contracts and encoded paths from Task 1; Rust `GET /v1/me` from the server plan.
- Produces: `FsliteClient`, `UpstreamResult<T>`, `loadServerConfig`, and sanitized `ActivityRecord` values.

- [ ] **Step 1: Write secret-resolution and sanitization tests**

Test token-file-over-error behavior, whitespace trimming, missing/empty secret
failure, URL normalization, header redaction, 64 KiB JSON truncation, binary
summaries, and curl placeholders:

```ts
it("never puts the bearer credential in activity", async () => {
  const record = buildActivity({
    token: "super-secret",
    serverUrl: "http://fslite-server:8080",
    method: "GET",
    path: "/v1/me",
    status: 200,
    durationMs: 4,
    response: { workspace_id: "w" },
  });
  expect(JSON.stringify(record)).not.toContain("super-secret");
  expect(record.curl).toContain("Authorization: Bearer $FSLITE_TOKEN");
  expect(record.curl).toContain("$FSLITE_SERVER_URL/v1/me");
});
```

- [ ] **Step 2: Implement server-only configuration**

Define:

```ts
export interface ServerConfig {
  serverUrl: URL;
  token: string;
  resetIntervalMs: number;
  requestTimeoutMs: number;
  trustProxy: boolean;
}
```

Read `FSLITE_SERVER_URL` (default `http://fslite-server:8080`), then
`FSLITE_TOKEN_FILE`, then `FSLITE_TOKEN`. Require a token, strip a trailing URL
slash, default reset to `900000`, timeout to `10000`, and never export this
module from browser/shared entry points.

- [ ] **Step 3: Write fetch-contract tests for every client family**

Mock `globalThis.fetch` and assert exact method/path/body for identity, tree,
usage, stat, read/write, mkdir, copy, move, trash, remove, list/restore/purge
trash, glob, find, content search, changes, and reset. Include root tree's
double slash:

```ts
await client.tree(validateVirtualPath("/"));
expect(fetch).toHaveBeenCalledWith(
  "http://server/v1/workspaces/ws/directories//tree?limit=250",
  expect.objectContaining({ method: "GET" }),
);
```

- [ ] **Step 4: Implement one bounded request primitive and typed methods**

`FsliteClient.request<T>` attaches Authorization, an abort timeout, and a
visitor request ID; parses JSON error envelopes; reads binary only for
`readFile`; and returns:

```ts
export interface UpstreamResult<T> {
  data: T;
  activity: ActivityRecord;
  contentType?: string;
}
```

Every public method constructs a fixed route. `writeFile` sends raw bytes and
optional `expected_revision`; `move` and `copy` send `{ to, recursive,
overwrite, expected_revision }`; content search base64-encodes UTF-8 search
text. `resetWorkspace` is exposed only on this server-side class.

- [ ] **Step 5: Verify server modules**

Run:

```bash
corepack pnpm --dir showcase test -- src/lib/server
corepack pnpm --dir showcase check
```

Expected: all configuration, sanitization, route, timeout, and error tests pass.

- [ ] **Step 6: Commit the private client**

```bash
git add -- showcase/src/lib/server showcase/src/lib/shared showcase/package.json showcase/pnpm-lock.yaml
git commit -m "feat(showcase): add private fslite client"
```

### Task 3: Operation Allowlist and Rate Limits

**Files:**
- Create: `showcase/src/lib/server/schemas.ts`
- Create: `showcase/src/lib/server/rate-limit.ts`
- Create: `showcase/src/lib/server/gateway.ts`
- Test: `showcase/src/lib/server/schemas.test.ts`
- Test: `showcase/src/lib/server/rate-limit.test.ts`
- Test: `showcase/src/lib/server/gateway.test.ts`

**Interfaces:**
- Consumes: `FsliteClient` and shared contracts.
- Produces: `PublicOperation` discriminated union and `ShowcaseGateway.execute(operation, clientIp)`.

- [ ] **Step 1: Write schema rejection tests**

Use a Zod discriminated union on `kind`. Cover all accepted operations and
reject unknown kinds, arbitrary methods/URLs, workspace IDs, >1 MiB text,
invalid paths, revision `0`, and recursive deletion without
`confirmedPath === path`.

- [ ] **Step 2: Define the finite operation contract**

Include these tags with only their required fields:

```ts
type PublicOperation =
  | { kind: "tree"; path: VirtualPath }
  | { kind: "read_file"; path: VirtualPath }
  | { kind: "write_file"; path: VirtualPath; text: string; expectedRevision?: number }
  | { kind: "mkdir"; path: VirtualPath; parents: boolean }
  | { kind: "copy"; from: VirtualPath; to: VirtualPath; recursive: boolean }
  | { kind: "move"; from: VirtualPath; to: VirtualPath }
  | { kind: "trash"; path: VirtualPath; expectedRevision?: number }
  | { kind: "remove"; path: VirtualPath; recursive: boolean; confirmedPath: string }
  | { kind: "list_trash" }
  | { kind: "restore"; trashId: string; destination?: VirtualPath }
  | { kind: "purge"; trashId: string; confirmedName: string }
  | { kind: "glob"; pattern: string }
  | { kind: "find"; root: VirtualPath; nameContains: string }
  | { kind: "search_content"; root: VirtualPath; text: string }
  | { kind: "changes"; after?: string }
  | { kind: "usage" };
```

- [ ] **Step 3: Write rolling-window rate-limit tests**

Inject a clock. Assert request 121 read, 31 mutation, and 11 upload fail inside
one minute, then pass after 60 seconds. Keys are `clientIp:bucket`; prune stale
timestamps on every check.

- [ ] **Step 4: Implement dispatch and rate classification**

`ShowcaseGateway.execute` parses with Zod, selects `read` or `mutation`, checks
the limiter, then switches exhaustively over `operation.kind`. It does not
accept reset/create-workspace/delete-workspace. Return `GatewayResult<T>` with
data plus exactly one activity record for visitor-initiated operations.

- [ ] **Step 5: Verify gateway behavior and commit**

Run `corepack pnpm --dir showcase test -- src/lib/server` and
`corepack pnpm --dir showcase check`; expect all tests to pass.

```bash
git add -- showcase/src/lib/server/schemas.ts showcase/src/lib/server/schemas.test.ts showcase/src/lib/server/rate-limit.ts showcase/src/lib/server/rate-limit.test.ts showcase/src/lib/server/gateway.ts showcase/src/lib/server/gateway.test.ts
git commit -m "feat(showcase): constrain public filesystem operations"
```

### Task 4: Reset Coordinator, Seed, and Process Runtime

**Files:**
- Create: `showcase/src/lib/server/reset-coordinator.ts`
- Create: `showcase/src/lib/server/seed.ts`
- Create: `showcase/src/lib/server/runtime.ts`
- Test: `showcase/src/lib/server/reset-coordinator.test.ts`
- Test: `showcase/src/lib/server/seed.test.ts`
- Test: `showcase/src/lib/server/runtime.test.ts`

**Interfaces:**
- Produces: `ResetCoordinator.withOperation`, `resetNow`, `snapshot`; `getShowcaseRuntime`; deterministic `SEED_ENTRIES`.
- Consumes: identity/reset/write/mkdir methods from `FsliteClient`.

- [ ] **Step 1: Test operation/reset exclusion**

Use deferred promises to prove reset waits for admitted operations, new
operations fail with `WorkspaceResettingError` once reset is pending, reset
calls upstream once, seeds in deterministic order, increments generation, and
sets `nextResetAt = completedAt + 900000`.

- [ ] **Step 2: Define deterministic seed content**

Create a constant manifest containing `/README.md`, `/docs`,
`/docs/http-api.md`, `/examples`, `/examples/hello.txt`, and
`/examples/metadata.json`. Directories precede files. Seed text explains tree,
content, trash, search, and reset endpoints and contains no environment values.

- [ ] **Step 3: Implement the reset gate and scheduler**

The coordinator tracks `activeOperations`, `resetting`, `generation`, and
`nextResetAt`. `withOperation` increments/decrements in `try/finally`.
`resetNow` marks resetting, waits for active count zero, calls reset, seeds,
updates status, and clears resetting in `finally`. `start()` resets immediately,
then starts a `setInterval(..., 900000)` and calls `.unref()`.

- [ ] **Step 4: Implement the lazy process singleton**

`getShowcaseRuntime()` loads config once, constructs client, calls `/v1/me`,
builds gateway/coordinator, and memoizes the initialization promise on
`globalThis` under a symbol. It exposes `liveness()`, `readiness()`,
`status()`, `execute()`, `upload()`, and `download()`; failed initialization
clears the memoized promise so a later request can retry.

- [ ] **Step 5: Verify reset/runtime tests and commit**

Run `corepack pnpm --dir showcase test -- src/lib/server` and expect all tests
to pass without real timers left open.

```bash
git add -- showcase/src/lib/server/reset-coordinator.ts showcase/src/lib/server/reset-coordinator.test.ts showcase/src/lib/server/seed.ts showcase/src/lib/server/seed.test.ts showcase/src/lib/server/runtime.ts showcase/src/lib/server/runtime.test.ts
git commit -m "feat(showcase): reset and seed shared sandbox"
```

### Task 5: Narrow Astro API Routes

**Files:**
- Create: `showcase/src/pages/api/status.ts`
- Create: `showcase/src/pages/api/health/live.ts`
- Create: `showcase/src/pages/api/health/ready.ts`
- Create: `showcase/src/pages/api/operation.ts`
- Create: `showcase/src/pages/api/upload.ts`
- Create: `showcase/src/pages/api/download.ts`
- Create: `showcase/src/lib/server/http.ts`
- Test: `showcase/src/lib/server/http.test.ts`

**Interfaces:**
- Produces: browser contract at `/api/status`, `/api/operation`, `/api/upload`, `/api/download?path=<canonical-path>`, `/api/health/live`, and `/api/health/ready`.

- [ ] **Step 1: Test request parsing, IP trust, and error envelopes**

Test JSON/content-type enforcement, 1 MiB upload rejection before buffering,
`X-Forwarded-For` use only when `FSLITE_TRUST_PROXY=true`, malformed operation `400`,
rate limit `429`, reset `503` with retry data, upstream structured errors, and
generic `502` without internal URL leakage.

- [ ] **Step 2: Implement shared HTTP helpers**

Provide `json`, `gatewayErrorResponse`, `clientIp`, and `readBoundedBody`.
Return:

```ts
interface PublicError {
  error: {
    code: string;
    message: string;
    status: number;
    requestId?: string;
    retryAfterMs?: number;
  };
}
```

- [ ] **Step 3: Implement operation and status endpoints**

`POST /api/operation` accepts JSON only, limits to 1 MiB, passes parsed data
and client IP to runtime, and returns `{ data, activity }`. `GET /api/status`
returns only readiness, generation, reset state, server `now`, `nextResetAt`,
and usage. It never exposes runtime workspace IDs or active-operation counts.
Liveness never initializes upstream; readiness does.

- [ ] **Step 4: Implement streaming upload and download endpoints**

Upload accepts a canonical `path` query parameter and the file as its raw body,
checks `Content-Length`, applies `readBoundedBody(request, 1_048_576)`,
classifies against the upload limit, and writes those bytes upstream. Download
requires the same canonical `path` query parameter, returns upstream bytes, and sets
`Content-Disposition: attachment` with a sanitized basename. It exposes only
sanitized `X-Fslite-Method`, `X-Fslite-Path`, `X-Fslite-Status`,
`X-Fslite-Duration-Ms`, and `X-Request-Id` headers so the browser can add the
download to activity. Neither route buffers more than 1 MiB, and download
never forwards upstream Authorization.

> Security requirement: do not restore a dynamic or catch-all download route.
> Astro Node constructs a Fetch URL before endpoint code runs, which resolves
> encoded dot segments and loses the evidence required to reject them. Query
> parsing exposes `URLSearchParams.get("path")` exactly once. Validate that
> decoded value directly with `validateVirtualPath` before runtime
> initialization; never decode it a second time. A singly encoded `%2e%2e`
> query segment becomes `..` and is rejected, whereas a double-encoded
> `%252e%252e` becomes the literal filename segment `%2e%2e`. The fixed
> `FsliteClient` content route re-encodes each validated segment, including `%`
> as `%25`, so literal percent names cannot normalize into traversal upstream.

- [ ] **Step 5: Verify the built route manifest**

Run:

```bash
corepack pnpm --dir showcase test
corepack pnpm --dir showcase build
```

Expected: the build contains all API routes and standalone server entry.

- [ ] **Step 6: Commit the gateway routes**

```bash
git add -- showcase/src/pages/api showcase/src/lib/server/http.ts showcase/src/lib/server/http.test.ts
git commit -m "feat(showcase): expose safe Astro API gateway"
```

### Task 6: Browser State and API Client

**Files:**
- Create: `showcase/src/lib/browser/api.ts`
- Create: `showcase/src/lib/browser/reducer.ts`
- Create: `showcase/src/lib/browser/use-showcase.ts`
- Test: `showcase/src/lib/browser/api.test.ts`
- Test: `showcase/src/lib/browser/reducer.test.ts`
- Test: `showcase/src/lib/browser/use-showcase.test.tsx`

**Interfaces:**
- Produces: `ShowcaseApi`, `ShowcaseState`, reducer actions, and `useShowcase` consumed by React components.

- [ ] **Step 1: Write reducer and polling tests**

Cover initial load, selection, dirty editor, successful mutation refresh,
activity append/clear, no activity for ten-second background tree refresh,
generation change, resetting state, preserved unsaved text, and revision
conflict state.

- [ ] **Step 2: Implement browser API methods**

One `operation<T>(PublicOperation)` method posts JSON and throws a typed
`ShowcaseError`. Add `upload(path, File)`, which sends the file as a bounded raw
body, and `download(path)`, which fetches a blob, derives a sanitized activity
entry from the response headers, and saves it through an object URL. The
browser module references only same-origin `/api/*` URLs and contains no
upstream URL, token, or workspace ID.

- [ ] **Step 3: Implement reducer and hook**

State includes status, tree, selected node/path, editor text/original/revision,
busy action, dialogs, search/trash/changes results, activities, and error. The
hook loads status/tree, polls every ten seconds, refreshes after mutations,
and stops timers on unmount. It never retries a failed mutation automatically.

- [ ] **Step 4: Verify hooks and commit**

Run `corepack pnpm --dir showcase test -- src/lib/browser` and expect fake-timer
tests to leave no pending timers.

```bash
git add -- showcase/src/lib/browser
git commit -m "feat(showcase): manage shared explorer state"
```

### Task 7: Tree, Editor, and Workspace Status UI

**Files:**
- Create: `showcase/src/components/explorer/ShowcaseExplorer.tsx`
- Create: `showcase/src/components/explorer/WorkspaceStatus.tsx`
- Create: `showcase/src/components/explorer/FileTree.tsx`
- Create: `showcase/src/components/explorer/FileEditor.tsx`
- Create: `showcase/src/components/explorer/Toolbar.tsx`
- Create: `showcase/src/components/explorer/ToastRegion.tsx`
- Test: `showcase/src/components/explorer/FileTree.test.tsx`
- Test: `showcase/src/components/explorer/FileEditor.test.tsx`
- Test: `showcase/src/components/explorer/WorkspaceStatus.test.tsx`
- Test: `showcase/src/components/explorer/ShowcaseExplorer.test.tsx`
- Modify: `showcase/src/pages/index.astro`

**Interfaces:**
- Consumes: `useShowcase` from Task 6.
- Produces: the core hydrated explorer with create/edit/save and reset awareness.

- [ ] **Step 1: Write accessible component tests**

Use roles/names rather than implementation selectors. Verify hierarchical tree
keyboard navigation, selection opens text, binary selection offers download,
dirty save uses expected revision, Ctrl/Cmd+S, reset countdown, usage meter,
disabled mutations while resetting, and revision conflict choices "Reload
server version" and "Copy my unsaved text".

- [ ] **Step 2: Implement status, toolbar, and tree**

Use semantic buttons and `role="tree"`/`treeitem` with `aria-expanded`.
Workspace status renders server health, `used / 10 MiB`, `nodes / 250`, and a
countdown computed from server `now` plus local elapsed time.

- [ ] **Step 3: Implement the plain text editor**

Render path, revision, byte size, dirty indicator, textarea, Save, Download,
and destructive actions. Decode only UTF-8 text; for invalid UTF-8 or known
binary content, render metadata and Download. Keep unsaved text in state across
background refresh/reset notices.

- [ ] **Step 4: Mount the island and verify**

Mount `<ShowcaseExplorer client:load />` below the hero. Run component tests,
`pnpm build`, and `pnpm check`; expect all to pass.

- [ ] **Step 5: Commit the core explorer**

```bash
git add -- showcase/src/components/explorer showcase/src/pages/index.astro showcase/src/lib/browser
git commit -m "feat(showcase): browse and edit SQLite files"
```

### Task 8: Full Filesystem Actions

**Files:**
- Create: `showcase/src/components/explorer/ActionDialog.tsx`
- Create: `showcase/src/components/explorer/CreateDialog.tsx`
- Create: `showcase/src/components/explorer/MoveCopyDialog.tsx`
- Create: `showcase/src/components/explorer/DeleteDialog.tsx`
- Create: `showcase/src/components/explorer/UploadDialog.tsx`
- Modify: `showcase/src/components/explorer/ShowcaseExplorer.tsx`
- Modify: `showcase/src/components/explorer/FileTree.tsx`
- Modify: `showcase/src/components/explorer/FileEditor.tsx`
- Modify: `showcase/src/components/explorer/Toolbar.tsx`
- Test: `showcase/src/components/explorer/CreateDialog.test.tsx`
- Test: `showcase/src/components/explorer/MoveCopyDialog.test.tsx`
- Test: `showcase/src/components/explorer/DeleteDialog.test.tsx`
- Test: `showcase/src/components/explorer/UploadDialog.test.tsx`

**Interfaces:**
- Adds: create file/folder, upload/download, move/rename, copy, trash, and confirmed permanent delete.

- [ ] **Step 1: Write action-flow tests**

Verify exact operation payloads, destination validation, recursive directory
copy, rename as move, upload size rejection before request, trash as default
delete, permanent-delete confirmation requiring the exact path, dialog focus
trap/return, Escape close, and tree refresh after success.

- [ ] **Step 2: Implement focused dialogs**

Each dialog owns only form state and submits one typed callback. `DeleteDialog`
defaults to trash and reveals permanent deletion behind a second choice. The
confirm button remains disabled until `confirmation === path`.

- [ ] **Step 3: Integrate actions and keyboard behavior**

Toolbar buttons create/upload at the selected directory. Node action menus
provide Rename, Move, Copy, Download, Move to trash, and Delete permanently as
valid for kind. Use native file input with `accept` unrestricted and enforce
1 MiB in both browser and server.

- [ ] **Step 4: Verify and commit full actions**

Run component tests, complete Vitest, build, and check. Expect no React act or
accessibility warnings.

```bash
git add -- showcase/src/components/explorer showcase/src/lib/browser
git commit -m "feat(showcase): demonstrate filesystem mutations"
```

### Task 9: Search, Trash, Changes, and API Activity

**Files:**
- Create: `showcase/src/components/explorer/SearchPanel.tsx`
- Create: `showcase/src/components/explorer/TrashPanel.tsx`
- Create: `showcase/src/components/explorer/ChangesPanel.tsx`
- Create: `showcase/src/components/explorer/ApiActivity.tsx`
- Modify: `showcase/src/components/explorer/ShowcaseExplorer.tsx`
- Modify: `showcase/src/lib/browser/reducer.ts`
- Modify: `showcase/src/lib/browser/use-showcase.ts`
- Test: `showcase/src/components/explorer/SearchPanel.test.tsx`
- Test: `showcase/src/components/explorer/TrashPanel.test.tsx`
- Test: `showcase/src/components/explorer/ChangesPanel.test.tsx`
- Test: `showcase/src/components/explorer/ApiActivity.test.tsx`

**Interfaces:**
- Completes: glob/find/content search, trash restore/purge, change feed, and permanently visible sanitized upstream activity.

- [ ] **Step 1: Write panel contract tests**

Test search-mode payloads and results, restore destination, purge exact-name
confirmation, change pagination, activity method/path/status/duration/request
ID, expand/collapse, copy curl, clear local history, bounded response notice,
and absence of bearer/server internals.

- [ ] **Step 2: Implement search, trash, and changes panels**

Use an accessible tablist for Explorer, Search, Trash, and Changes while
keeping the editor alongside Explorer on wide screens. Search offers Filename,
Glob, and Contents modes. Trash exposes Restore and Purge. Changes lists
sequence, kind, old/new path, revision, and timestamp.

- [ ] **Step 3: Implement always-visible API activity**

Place `<ApiActivity>` below the explorer regardless of active tab. Entries use
`<details>`, render JSON in `<pre>`, and copy only the sanitized curl string.
Background refresh and reset seeding never enter this browser-local list.

- [ ] **Step 4: Verify and commit discovery/activity features**

Run all component tests, build, and check; expect pass.

```bash
git add -- showcase/src/components/explorer showcase/src/lib/browser
git commit -m "feat(showcase): expose search trash changes and API activity"
```

### Task 10: Clean Editorial Responsive Polish

**Files:**
- Modify: `showcase/src/styles/global.css`
- Modify: `showcase/src/layouts/Layout.astro`
- Modify: all explorer components only for class names/semantic fixes
- Create: `showcase/public/favicon.svg`
- Test: `showcase/src/components/explorer/accessibility.test.tsx`

**Interfaces:**
- Produces: approved visual direction and responsive behavior without changing data contracts.

- [ ] **Step 1: Add accessibility and responsive assertions**

Run axe-compatible semantic assertions through Testing Library, verify one
`h1`, labels for every form control, visible focus, live toast region,
`prefers-reduced-motion`, and no horizontal overflow classes at 375px.

- [ ] **Step 2: Apply the visual system**

Use a 1200px content shell, blue `#315be8` accent, slate text, white panels,
subtle borders/shadows, 44px minimum touch targets, sans-serif system stack,
and monospace paths/API. Wide layout uses `minmax(240px, 0.36fr) 1fr`; below
760px it stacks tree, editor, then activity.

- [ ] **Step 3: Add loading, empty, and failure states**

Provide skeleton-free text loading states, seeded empty-state guidance,
backend-unavailable banner, resetting overlay that does not hide unsaved text,
and clear disabled control explanations.

- [ ] **Step 4: Verify formatting, accessibility, and build**

Run `pnpm test`, `pnpm check`, and `pnpm build`; inspect at 375px, 768px, and
1440px with browser screenshots during implementation.

- [ ] **Step 5: Commit visual polish**

```bash
git add -- showcase/src showcase/public/favicon.svg
git commit -m "style(showcase): apply clean editorial explorer design"
```

### Task 11: Real-Server Playwright Coverage

**Files:**
- Create: `showcase/playwright.config.ts`
- Create: `showcase/e2e/fixtures.ts`
- Create: `showcase/e2e/explorer.spec.ts`
- Create: `showcase/e2e/reset.spec.ts`
- Create: `showcase/e2e/security.spec.ts`
- Modify: `showcase/package.json`

**Interfaces:**
- Consumes: installed local `fslite-server` from the Rust plan and built Astro app.
- Produces: end-to-end proof of the public experience.

- [ ] **Step 1: Build an isolated E2E process fixture**

Use `mkdtemp`, allocate free loopback ports, create a token file, spawn
`cargo run -p fslite-server -- --db ... --config ... --bind ... --token-file
...`, then spawn built Astro with `FSLITE_SERVER_URL`, `FSLITE_TOKEN_FILE`, and
a test reset interval. Kill both process groups and delete only the fixture
directory in teardown.

- [ ] **Step 2: Write the complete filesystem journey**

Through visible controls: create folder/file, edit/save, upload/download,
rename, move, copy, search by name/content, trash, restore, permanently delete,
purge, and view changes. After each action assert the API panel's underlying
method/path/status.

- [ ] **Step 3: Write reset and concurrency journeys**

Assert seeded startup, visible countdown, automatic reset to deterministic
seed, blocked mutation during reset, preserved unsaved text, and stale-revision
UI offering reload/copy without overwrite.

- [ ] **Step 4: Write security and responsive journeys**

Intercept all browser responses and rendered text; assert the real token and
private Docker hostname never appear. Assert unknown operation rejection,
oversize upload rejection, rate-limit response, and functional 375px stacked
navigation.

- [ ] **Step 5: Run and commit E2E coverage**

```bash
corepack pnpm --dir showcase exec playwright install chromium
corepack pnpm --dir showcase build
corepack pnpm --dir showcase test:e2e
```

Expected: all journeys pass against the real Rust server.

```bash
git add -- showcase/playwright.config.ts showcase/e2e showcase/package.json showcase/pnpm-lock.yaml
git commit -m "test(showcase): verify real fslite journeys"
```

### Task 12: Docker, Compose, Caddy, and Operator Documentation

**Files:**
- Create: `showcase/Dockerfile`
- Create: `showcase/docker-entrypoint.sh`
- Create: `deploy/showcase/compose.yml`
- Create: `deploy/showcase/Caddyfile`
- Create: `deploy/showcase/fslite-token.example`
- Create: `deploy/showcase/README.md`
- Modify: `.gitignore`
- Modify: `README.md`

**Interfaces:**
- Produces: a copyable deployment matching the user's `docker-caddy-astro` stack.

- [ ] **Step 1: Add the non-root Astro image**

Build with `node:22-alpine`, Corepack pnpm 10.12.4, `pnpm fetch`, frozen-lockfile
install, test-free production build, and a runtime stage containing production
dependencies plus `dist`. Run as UID/GID 10001 on port 4321. Entrypoint validates
required server URL/token file without printing the token.

- [ ] **Step 2: Add the focused Compose reference**

Define `caddy`, `astro`, and `fslite-server`; mount `fslite_data`; mount one
read-only token secret into both application containers; expose only Caddy
ports; add existing-style health checks; make Astro readiness depend on the
server; and declare `restart: unless-stopped`. The Astro environment includes:

```yaml
FSLITE_SERVER_URL: http://fslite-server:8080
FSLITE_TOKEN_FILE: /run/secrets/fslite_token
FSLITE_RESET_INTERVAL_MS: 900000
FSLITE_TRUST_PROXY: "true"
```

- [ ] **Step 3: Add the Caddy route**

The reference Caddyfile reverse-proxies the showcase hostname to
`astro:4321`, enables compression and security headers, and contains no route
to `fslite-server:8080`.

- [ ] **Step 4: Document integration with the supplied stack**

Explain that the blank `./app` can be replaced by `showcase/`; add the
`fslite-server` service/volume/secret; retain unrelated `next`, `postgres`,
`rustfs`, and `toolchain` services unchanged; generate a 64-hex-character token
with `openssl rand -hex 32`; set file mode `0600`; run `docker compose up -d
--build`; and verify `/api/health/ready`.

- [ ] **Step 5: Validate containers and configuration**

Run:

```bash
docker compose -f deploy/showcase/compose.yml config
docker compose -f deploy/showcase/compose.yml build
docker compose -f deploy/showcase/compose.yml up -d
curl --fail --insecure --resolve localhost:443:127.0.0.1 \
  https://localhost/api/health/ready
docker compose -f deploy/showcase/compose.yml down
```

Expected: config/build succeed, both services become healthy, browser API works,
and no token or server port appears in the public response. Do not delete the
named data volume as part of ordinary teardown.

- [ ] **Step 6: Commit deployment assets**

```bash
git add -- showcase/Dockerfile showcase/docker-entrypoint.sh deploy/showcase .gitignore README.md
git commit -m "docs(showcase): add Docker and Caddy deployment"
```

### Task 13: Showcase Final Verification

**Files:**
- Verify only; fix failures in the smallest owning file and commit separately.

**Interfaces:**
- Produces: release-grade evidence for the complete approved specification.

- [ ] **Step 1: Run frontend quality gates**

```bash
corepack pnpm --dir showcase install --frozen-lockfile
corepack pnpm --dir showcase check
corepack pnpm --dir showcase test
corepack pnpm --dir showcase build
corepack pnpm --dir showcase test:e2e
```

Expected: lint/type/format, unit/component, build, and real-server E2E pass.

- [ ] **Step 2: Run complete Rust gates again**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Expected: all Rust checks remain green after deployment/documentation changes.

- [ ] **Step 3: Run Compose smoke and inspect isolation**

Bring up the reference stack, confirm Caddy and Astro are the only publicly
addressable services, execute create/read/trash/reset through the browser API,
restart containers, and verify the persistent server database survives while
the showcase reset schedule reseeds it.

- [ ] **Step 4: Verify repository and secret hygiene**

```bash
git diff --check
git status --short
rg -n "Bearer [A-Za-z0-9_-]{20,}|FSLITE_TOKEN=" showcase deploy README.md --glob '!*.example'
```

Expected: no unintended changes and no credential values. Example files contain
placeholders only.

- [ ] **Step 5: Record final evidence without deployment claims**

Report exact pass counts, built image names, package warnings, and skipped
environment-only checks. State explicitly that crates, images, and the user's
live Caddy deployment were not published or changed.

# Persistent fslite-server and Astro Showcase Design

## Purpose

Turn `fslite-server` from reference wiring into a persistent, immediately
usable binary and add a public Astro showcase that demonstrates SQLite as a
filesystem through the real HTTP API.

The local server experience begins with:

```bash
cargo install fslite-server
fslite-server
```

The public showcase presents a familiar filesystem tree, a text editor, the
complete set of useful file-management operations, and the underlying HTTP
request and response for every interaction. It runs behind Caddy in the
user's existing Docker deployment and resets its shared sandbox every 15
minutes.

## Scope

This project includes:

- persistent zero-configuration startup for the `fslite-server` binary;
- secure persisted local credentials and configurable Docker startup;
- authenticated identity discovery through `GET /v1/me`;
- an administrative same-workspace reset operation;
- an Astro SSR application in `showcase/`;
- a private, typed Astro-to-fslite API gateway;
- a tree and editor interface with full filesystem demonstration features;
- a permanently visible API activity panel;
- periodic sandbox reset and deterministic seed content;
- production Dockerfiles, Compose examples, Caddy guidance, and health checks;
- automated Rust, TypeScript, component, and browser tests; and
- `fslite-sqlite 0.2.0` and `fslite-server 0.2.0` release preparation.

This project does not include user accounts, per-visitor workspaces, billing,
multi-node SQLite replication, durable public visitor content, arbitrary
server administration from the browser, or a general-purpose web desktop.
The public showcase is one shared, intentionally disposable sandbox.

## Repository and Deployment Boundaries

The showcase lives in this repository under `showcase/`. Keeping the UI and
server contract together lets one pull request update and test both sides of
the integration. The application remains independently buildable and
deployable; it is not included in any Cargo package.

Production uses separate containers connected by Docker's private network:

```text
Internet
   |
 Caddy
   |
 Astro SSR gateway
   |
 fslite-server
   |
 /data/fslite.db on the fslite_data volume
```

Caddy exposes only Astro. `fslite-server` has no host port in the reference
Compose deployment. Astro calls it at `http://fslite-server:8080`, so the
browser never needs CORS access or a bearer token. Existing PostgreSQL,
RustFS, Next.js, and toolchain services in the user's broader Compose stack
remain independent and are not fslite dependencies.

## fslite-server Startup Experience

### Defaults

With no flags or environment variables, the binary:

1. resolves platform-specific fslite data and configuration paths;
2. opens or creates a persistent SQLite database;
3. creates a default workspace if no usable persisted workspace is found;
4. generates a cryptographically strong bearer token if no credential is
   supplied or persisted;
5. writes the workspace ID and generated token to a permission-restricted
   JSON configuration file;
6. binds to `127.0.0.1:8080`; and
7. prints its database path, workspace ID, address, and ready-to-run `fslite`
   connection commands.

When the database or usable workspace is missing, startup prints this exact
line:

```text
No database or workspace found, creating default database and workspace
```

The generated token is printed only during first-run bootstrap. Subsequent
starts report the credential file path without echoing the secret. On Unix,
the credential file is created with owner-only permissions. Writes use an
atomic replace so an interrupted startup cannot leave partial JSON.

If a database exists but its server configuration is absent, the server
creates a new default workspace in that database without removing any
unknown existing workspaces. If the configured workspace no longer exists,
the server creates a replacement and atomically updates the configuration.

### Configuration

Startup follows the precedence `CLI flag > environment > persisted config >
default`. Supported settings are:

- database path: `--db` / `FSLITE_DB`;
- listener address: `--bind` / `FSLITE_BIND`;
- server state path: `--config` / `FSLITE_CONFIG`;
- credential input: `--token-file`, `FSLITE_TOKEN_FILE`, or `FSLITE_TOKEN`;
- first-workspace maximum bytes, nodes, and file bytes; and
- structured log filtering through the existing tracing environment.

There is deliberately no plain `--token` flag because command arguments are
commonly exposed through process listings and shell history. An explicitly
supplied token overrides the stored token for that process while preserving
the configured workspace. Docker deployments supply the same secret to
`fslite-server` and Astro through environment or secret files.

The reference container configuration is equivalent to:

```yaml
fslite-server:
  build:
    context: .
    dockerfile: crates/fslite-server/Dockerfile
  environment:
    FSLITE_DB: /data/fslite.db
    FSLITE_CONFIG: /data/server.json
    FSLITE_BIND: 0.0.0.0:8080
    FSLITE_TOKEN_FILE: /run/secrets/fslite_token
  volumes:
    - fslite_data:/data
  healthcheck:
    test: ["CMD", "wget", "-qO-", "http://127.0.0.1:8080/readyz"]
```

The final image uses a multi-stage Rust build and a small runtime image. The
process runs as a non-root user that owns `/data`.

## Server API Additions

### Credential identity

`GET /v1/me` authenticates from request headers without requiring a workspace
path segment and returns safe actor identity:

```json
{
  "workspace_id": "019...",
  "capabilities": ["read", "write", "delete", "trash_restore", "workspace_admin"]
}
```

Actor metadata and the bearer credential are not returned. Astro calls this
route during readiness and caches the workspace ID for constructing ordinary
workspace-scoped routes.

### Same-workspace reset

`POST /v1/workspaces/{workspace_id}/reset` requires authentication for the
same workspace and the `WorkspaceAdmin` capability. It atomically returns the
workspace to its empty initial state while retaining:

- the workspace ID;
- workspace creation metadata and quota configuration; and
- the credential-to-workspace relationship.

It removes every non-root node, content chunk, attribute, trash entry, and
change record and restores usage counters to the empty-workspace values. It
does not create showcase seed files; seeding remains an Astro concern.

The operation is added to the server's `WorkspaceAdmin` boundary and to the
SQLite backend as an inherent administrative method. The SQLite work occurs
in one transaction. Failure rolls back the entire reset.

These additive public APIs require `fslite-sqlite 0.2.0` under the repository's
pre-1.0 semver policy. `fslite-server 0.2.0` depends on
that release, so publication order is `fslite-sqlite` and then
`fslite-server`.

## Astro Application Architecture

`showcase/` is an Astro application configured with the Node adapter in
standalone SSR mode. Astro owns the page frame, explanatory content, metadata,
and server endpoints. A React island owns the stateful file explorer. Custom
CSS supplies the visual system; the first release does not introduce a
utility-CSS framework or a heavy code-editor dependency.

The application is divided into focused modules:

- `src/pages/index.astro` renders the showcase shell and initial status;
- `src/components/explorer/` contains the React tree, editor, toolbars,
  dialogs, search, trash, history, and activity components;
- `src/lib/server/fslite-client.ts` is the only module that understands the
  upstream fslite HTTP contract;
- `src/lib/server/gateway.ts` validates and dispatches the finite public
  operation set;
- `src/lib/server/reset-coordinator.ts` serializes requests against resets
  and schedules reset plus seed;
- `src/lib/shared/contracts.ts` defines browser/gateway request and response
  types; and
- `src/pages/api/` exposes narrow same-origin endpoints to the React island.

The browser cannot submit an arbitrary upstream URL, HTTP method, header, or
workspace ID. Each supported UI action maps to one typed gateway operation.

## Showcase Interaction Design

### Page structure and visual direction

The approved visual direction is clean editorial: a bright surface, restrained
blue accent, crisp sans-serif interface typography, and monospace only where
code, paths, or API data benefits from it. The page is documentation-friendly
and avoids both generic dashboard chrome and terminal cosplay.

The page contains:

1. a concise hero explaining that files, directories, metadata, and bodies
   are persisted in SQLite;
2. live server, workspace usage, and reset-countdown status;
3. a two-column filesystem tree and file editor;
4. operation-specific dialogs and result views; and
5. an always-visible API activity panel below the explorer.

On narrow screens, the tree, editor, and activity panel stack in that order.
The selected path remains visible, and primary controls remain reachable
without horizontal scrolling. All controls have keyboard focus treatment,
accessible names, and reduced-motion behavior.

### Filesystem operations

The first public version demonstrates:

- paginated directory and recursive tree browsing;
- file and directory creation;
- text file open, edit, and save;
- host-file upload and file download;
- rename and move;
- file and directory copy;
- trash, restore, and purge;
- confirmed permanent recursive deletion;
- filename/path search, glob, and text search;
- change history; and
- manual refresh plus background synchronization.

Deleting from the main tree uses trash by default. Permanent deletion and
purge require an explicit confirmation that names the target. Binary files
are downloadable but are not decoded into the text editor. The editor uses a
plain, accessible text area with dirty-state tracking and keyboard save for
the first release.

The tree refreshes after every successful mutation and performs a lightweight
background refresh every ten seconds because all visitors share
one workspace. Background refresh traffic is not added to the visitor's API
activity history. Revision-aware writes protect a visitor from silently
overwriting another visitor's newer content.

### API activity

Every visitor-initiated operation adds an entry below the explorer containing:

- HTTP method and the actual upstream fslite path;
- sanitized query and request data;
- response status, elapsed time, and request ID;
- JSON response or binary length/content-type metadata; and
- a copyable `curl` example.

The panel describes the real request made from Astro to `fslite-server`, not
only the browser-to-Astro wrapper. Authorization values are always represented
as `$FSLITE_TOKEN`. Binary bodies are summarized rather than embedded, and
responses are bounded before display. Visitors can expand entries and clear
their local history.

## Gateway and Data Flow

For a normal mutation:

1. the React island submits a typed JSON request to a same-origin Astro API
   endpoint;
2. the API route applies body limits, schema validation, path validation,
   operation allowlisting, and per-IP rate limits;
3. the reset coordinator admits the request under its shared-operation lock;
4. the fslite client attaches the private token and calls the private Rust
   server;
5. Astro sanitizes bounded request/response details into an activity record;
6. the browser updates the tree/editor and appends that record; and
7. the gateway releases the shared-operation lock.

The client encodes virtual paths segment by segment and never concatenates an
untrusted raw path into a URL. It preserves fslite's structured error envelope,
status code, and request ID.

The activity record is per browser and is not stored in SQLite or on the
Astro server.

## Reset and Seed Lifecycle

The public sandbox resets when Astro starts and every 15 minutes afterward.
One Node process is the supported deployment topology for this first shared
sandbox.

The reset coordinator implements a reader/writer-style gate:

- ordinary API operations share access;
- reset waits for admitted operations to finish, then blocks new operations;
- the coordinator invokes the protected reset endpoint;
- it creates a deterministic seed tree through ordinary fslite APIs; and
- it publishes the new reset generation and next reset timestamp before
  admitting traffic again.

During the short reset window, new public operations receive a structured
`503 workspace_resetting` response with retry guidance. The browser displays
a resetting state, retains unsaved editor text locally, and reloads the tree
when the generation changes.

Seed content includes a welcome document, an API-oriented examples directory,
small text and JSON files, and nested directories that make tree navigation,
search, copy, move, trash, and downloads immediately demonstrable. Seeds stay
small and contain no secrets.

`GET /api/status` reports backend readiness, current generation, reset state,
next reset timestamp, and workspace usage. The countdown derives from the
server timestamp rather than assuming the browser clock and interval are
authoritative.

## Security and Resource Controls

The showcase is intentionally writable by anonymous visitors, so the gateway
uses defense in depth:

- `fslite-server` is reachable only on the Docker network;
- only Astro holds the bearer token;
- upstream Authorization headers never enter browser responses or application
  logs;
- browser operations use a finite allowlist and validated schemas;
- paths use the same absolute virtual-path rules as fslite and reject NUL or
  malformed input;
- request bodies and displayed response bodies have explicit byte limits;
- the gateway applies per-IP read and stricter mutation/upload rate limits;
- the deployment trusts forwarding headers only from its Caddy hop;
- destructive operations require UI confirmation;
- Astro cannot create or delete workspaces through its public routes;
- reset cannot be invoked by a browser route; and
- the workspace enforces 250 nodes, 10 MiB total content, and
  1 MiB per file in addition to gateway upload limits.

Rate values are configurable. The defaults per client IP are 120 read requests,
30 mutation requests, and 10 uploads in a rolling minute. Rate-limit state is
in memory because the deployment has one Astro process and reset state is
intentionally ephemeral.

## Error and Concurrency Behavior

Astro wraps transport failures in one browser-facing contract while retaining
the upstream fslite error code, status, message, request ID, and safe details
for the activity panel.

- Field and path errors render next to their initiating control.
- Ordinary operation failures render a concise toast plus an expandable
  activity entry.
- Backend unavailability disables mutations but leaves explanatory content and
  the most recently loaded tree visible.
- A stale expected revision never overwrites newer data. The editor offers to
  reload or copy the visitor's unsaved content.
- A reset conflict produces the resetting state and an automatic retry of
  read-only refresh after the advertised delay; mutations are not replayed
  automatically.
- Network timeouts are bounded and reported without leaking internal URLs or
  credentials.

## Docker and Caddy Integration

The repository provides:

- a multi-stage `fslite-server` Dockerfile;
- a multi-stage Astro/Node Dockerfile;
- a focused Compose example containing Caddy, Astro, fslite-server, the
  `fslite_data` volume, health checks, and a shared secret file;
- an environment example with non-secret defaults; and
- a Caddy route that proxies the public hostname to Astro only.

The user's existing blank `./app` Astro context can be replaced by the
contents of `showcase/`, or its Compose service can build the showcase image
from this repository. The documentation gives both approaches without
modifying or depending on the unrelated Postgres, RustFS, Next.js, or
toolchain services.

Astro readiness verifies that `GET /v1/me` succeeds against the private
backend. Liveness only verifies the Node process, preventing a transient
backend outage from creating a restart loop. The server exposes its existing
`/healthz` and `/readyz` probes.

## Testing Strategy

### Rust

Rust tests cover:

- first-run database, workspace, credential, and configuration creation;
- the exact bootstrap message;
- restart reuse of the database, workspace, and generated token;
- explicit configuration precedence and malformed configuration errors;
- owner-only credential permissions on Unix;
- `/v1/me` authentication and safe response shape;
- reset authorization, workspace matching, atomicity, quota preservation, and
  removal of nodes, content, attributes, trash, usage, and change history;
- persistence after reopening the reset database; and
- all pre-existing server and SQLite tests.

### Astro and browser

Vitest covers the typed fslite client, path encoding, schema validation,
secret sanitization, bounded activity records, error mapping, rate limiting,
and reset coordination. React Testing Library covers tree navigation, dirty
editor behavior, revision-conflict choices, confirmation dialogs, trash,
search, activity expansion, and keyboard interactions.

Playwright runs the built showcase against a real `fslite-server` and verifies
the complete create/edit/upload/download/rename/move/copy/trash/restore/purge/
search flow, API activity output, secret redaction, responsive navigation,
backend-unavailable behavior, and reset-to-seed behavior.

CI and local verification run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pnpm --dir showcase lint
pnpm --dir showcase test
pnpm --dir showcase build
pnpm --dir showcase test:e2e
docker compose config
```

Package dry runs verify `fslite-sqlite 0.2.0` before
`fslite-server 0.2.0`. Container smoke tests verify non-root startup, volume
persistence, health checks, and secret-file configuration.

## Documentation and Release

The root README gains a concise server quick start and showcase link. Server
documentation describes local defaults, configuration precedence, token
handling, Docker startup, API identity, reset authorization, and the
difference between a reusable server and the disposable public demo.

`CHANGELOG.md`, the affected crate release notes, and package versions are
updated for `fslite-sqlite 0.2.0` and `fslite-server 0.2.0`. The server's
package metadata points to its own docs.rs page, and its external example is
packaged correctly so `cargo publish --dry-run` has no missing-example warning.

Publishing and deployment are separate, explicitly authorized steps. The
implementation prepares packages, images, and documentation but does not
publish crates, push images, change the live Compose host, or deploy Caddy.

## Acceptance Criteria

- A newly installed `fslite-server` starts with no required flags, creates a
  persistent database and default workspace, prints the approved bootstrap
  message, and supplies a usable connection command.
- Restarting the server preserves content, workspace identity, and credential
  validity.
- Docker can run the server non-root against `/data`, and Astro reaches it only
  through the private network.
- `GET /v1/me` returns the authenticated workspace and capabilities without
  credential or actor-metadata leakage.
- An authorized reset atomically empties a workspace while preserving its ID,
  credential scope, and quotas.
- The Astro showcase presents the approved clean-editorial tree-and-editor
  interface and every scoped filesystem feature works through the real API.
- Every visitor action produces a sanitized, copyable API activity entry.
- The workspace returns to deterministic seed content on startup and every 15
  minutes, with safe behavior for concurrent and unsaved work.
- Tokens never reach browser payloads, generated activity, or logs.
- Quotas, upload limits, rate limits, and destructive confirmations are
  enforced.
- Rust, TypeScript, component, end-to-end, formatting, lint, build, package,
  and container smoke checks pass.

# fslite

A transport-independent, async filesystem interface; a persistent,
multi-workspace SQLite implementation suitable for direct embedding; and an
HTTP server, typed command codec, and CLI built on top of it.

**Docs**: [guides and concepts](https://docs.fslite.rusty.yachts) ·
[API reference on docs.rs](https://docs.rs/fslite-core) ·
[project homepage](https://fslite.rusty.yachts)

## Quick start

Install the command and use a persistent SQLite-backed filesystem immediately:

```console
cargo install fslite
fslite mkdir docs
fslite write docs/hello.txt --text=hello
fslite cat docs/hello.txt
```

CLI paths may be absolute (`/docs/hello.txt`) or relative to the active
workspace root (`docs/hello.txt`). fslite has no virtual current-directory
state, so relative paths always start at that workspace root.

On the first filesystem command, fslite writes this one-time notice to stderr:

```text
No database or workspace found, creating default database and workspace
```

It creates a filesystem and workspace both named `default`, stores `fslite.db`
in the operating system's local application-data directory, and silently
reuses them afterward. Set `FSLITE_DATA_DIR` to choose a different data
directory. The existing `FSLITE_CONFIG_DIR` controls registry/context files.
Explicit `--db`, `--memory`, `--server`, `--filesystem`, and
`--create-workspace` workflows bypass automatic initialization.

## Workspace layout

| Crate | Purpose |
| --- | --- |
| [`fslite-core`](crates/fslite-core) | The canonical `FileSystem` trait, domain types (`VirtualPath`, `Node`, `Revision`, ...), and stable typed errors. Transport-independent: no SQL, no HTTP, no host filesystem paths. |
| [`fslite-sqlite`](crates/fslite-sqlite) | `SqliteFileSystem`: a `FileSystem` implementation backed by one SQLite database that can hold many isolated workspaces. |
| [`fslite-conformance`](crates/fslite-conformance) | A backend-agnostic contract test suite. Any `FileSystem` implementation can prove basic compliance by implementing `ConformanceFactory` and calling `run_conformance`. |
| [`fslite-server`](crates/fslite-server) | An `axum`-based HTTP adapter exposing `FileSystem` as a resource-oriented API: nodes, directories, trash, content (including ranged reads), search, batch, and workspace-admin routes, gated behind a pluggable `AuthProvider`. |
| [`fslite-command`](crates/fslite-command) | A typed `Command` codec (one variant per `FileSystem` operation), a constrained shell-like lexer/parser, and local/remote executors that drive either an in-process `FileSystem` or a running `fslite-server` over HTTP. |
| [`fslite`](crates/fslite-cli) | `fslite`: a command-line client built on `fslite-command`, usable against a local SQLite database, an in-memory database, or a remote `fslite-server`, in one-shot or REPL mode. |

## Embedded Rust quick start

```rust
use fslite_core::{RequestContext, VirtualPath, WriteSource};
use fslite_sqlite::SqliteFileSystem;
use futures::StreamExt;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let fs = SqliteFileSystem::open_in_memory(Default::default()).await?;
let workspace = fs.create_workspace(Default::default()).await?;
let ctx = RequestContext::trusted(workspace.id);

let path = VirtualPath::parse("/hello.txt")?;
fs.write(&ctx, &path, WriteSource::from_bytes(b"hello".to_vec()), Default::default()).await?;

let read = fs.read(&ctx, &path, Default::default()).await?;
let mut stream = read.into_stream();
while let Some(chunk) = stream.next().await {
    print!("{}", String::from_utf8_lossy(&chunk?));
}
# Ok(()) }
```

See [`examples/embedded.rs`](examples/embedded.rs) for a complete, runnable
version (`cargo run --example embedded`), and the full list below for more.

## Examples

Every example is self-contained (opens its own in-memory database, needs no
setup) and runnable with `cargo run --example <name>`:

| Example | Demonstrates |
| --- | --- |
| [`embedded`](examples/embedded.rs) | Open a database, write a file from a byte stream, read it back, list a directory, print workspace usage. |
| [`batch`](examples/batch.rs) | Atomic multi-operation batches via `batch` — an aborted batch commits nothing, a valid one commits every operation together. |
| [`trash_lifecycle`](examples/trash_lifecycle.rs) | `trash` hides a subtree without touching its data, `restore` brings it back (optionally under a new name), and `purge` is the only way its content is actually reclaimed. |
| [`workspace_isolation`](examples/workspace_isolation.rs) | Two workspaces in one database hold the same path independently, with no cross-workspace visibility — including a rejected cross-workspace pagination cursor. |
| [`search_and_glob`](examples/search_and_glob.rs) | `glob` (path-shape matching), `find` (bounded metadata predicates), and `search_content` (literal byte matches inside files). |
| [`server_and_remote_cli`](crates/fslite-server/examples/server_and_remote_cli.rs) | Runs `fslite-server`'s HTTP API in-process and drives it with `fslite-command`'s `RemoteExecutor` — the same client `fslite --server` uses — over a real TCP connection. Run with `cargo run -p fslite-server --example server_and_remote_cli`. |

## CLI and server

`fslite` can talk to a local database file, a private in-memory one, or a
remote `fslite-server` over HTTP — the same verb syntax works in all three
modes. Flag values use `--name=value` (the lexer does not treat a following
bare word as a flag's value). `--memory` opens a fresh, unshared database for
that single process only, so it's suited to one-off experiments or scripts
that create a workspace and use it within the same invocation; use
`--db <path>` whenever a workspace needs to be visible across separate
`fslite` invocations:

For everyday interactive use, `fslite` also supports named filesystems and
workspaces, persisted in a small local registry
(`$XDG_CONFIG_HOME/fslite` or `~/.config/fslite`, overridable via
`FSLITE_CONFIG_DIR`) so you don't need to repeat `--db`/`--workspace` on
every invocation:

```bash
fslite create filesystem-main -f fsmain.db -w workspace-main
fslite use filesystem-main -w workspace-main

fslite mkdir docs
fslite touch docs/file.md
fslite write docs/file.md --text=hello
fslite cat docs/file.md
fslite rm docs/file.md

fslite delete filesystem-main -y   # permanently deletes fsmain.db
```

`create`/`use`/`delete` manage this registry only — the names it stores
have no meaning to `fslite-core`/`fslite-sqlite`/`fslite-server`, which
only ever see raw workspace ids. `--filesystem <name>` and `--workspace
<name-or-id>` override the persisted context for a single invocation
without changing it (`fslite --filesystem other-fs mkdir /tmp`); the raw
`--db`/`--memory`/`--server` + `--workspace <uuid>` flags shown below
remain fully supported and bypass the registry/context entirely — useful
for scripting against a database you don't want registered under a name.

```bash
# Local, one-shot, persisted to a file. --create-workspace prints the new
# workspace's id as its only line of output (after cargo's own build/run
# noise) — capture it with `-q` to suppress that noise and keep the
# variable clean:
WORKSPACE=$(cargo run -q -p fslite -- --db ./fslite.db --create-workspace)
cargo run -p fslite -- --db ./fslite.db --workspace "$WORKSPACE" mkdir /docs
cargo run -p fslite -- --db ./fslite.db --workspace "$WORKSPACE" write /docs/hello.txt --text=hi
cargo run -p fslite -- --db ./fslite.db --workspace "$WORKSPACE" ls /

# Local, interactive REPL (the workspace must already exist — create it
# first, as above, or against the same --db file)
cargo run -p fslite -- --db ./fslite.db --workspace "$WORKSPACE" --repl

# Remote, against a running fslite-server
cargo run -p fslite -- --server http://localhost:8080 --workspace <id> \
  --token "$FSLITE_TOKEN" ls /
```

`fslite-server` is a standalone persistent SQLite server. It creates a
default database, workspace, and bearer credential on its first start, then
reuses them on later starts.

```bash
cargo install fslite-server
fslite-server

# Docker/private-network deployment
FSLITE_TOKEN_FILE=/run/secrets/fslite_token \
FSLITE_DB=/data/fslite.db \
FSLITE_BIND=0.0.0.0:8080 \
fslite-server
```

## Server operation and security

With no options, `fslite-server` stores its database at the platform fslite
local-data path plus `fslite.db`, and its durable server state at the platform
fslite configuration path plus `server.json`. On Linux these are normally
`$XDG_DATA_HOME/fslite/fslite.db` (or `~/.local/share/fslite/fslite.db`) and
`$XDG_CONFIG_HOME/fslite/server.json` (or `~/.config/fslite/server.json`).
Use `--db` / `FSLITE_DB` and `--config` / `FSLITE_CONFIG` to make locations
explicit; the binary does not print either path during a normal first start.

When the database or the configured workspace is absent, it prints this exact
line once before the connection guidance:

```text
No database or workspace found, creating default database and workspace
```

The same first start prints one ready-to-run client command containing a newly
generated 64-hex-character bearer credential and the new workspace ID. Copy
the credential into a protected secret store immediately; it is not printed on
later starts. Later starts name the persisted configuration file and print a
client command using `$FSLITE_TOKEN` instead. The default listener is
`127.0.0.1:8080` (not publicly reachable); `--bind` / `FSLITE_BIND` changes
it. Supplying `--bind 127.0.0.1:0` makes the listening line and connection
guidance use the actual assigned port.

Configuration uses command-line values ahead of the matching environment
variables, then persisted state, then defaults. This applies to `--db`,
`--bind`, `--config`, `--token-file`, `--max-bytes`, `--max-nodes`, and
`--max-file-bytes`. Credential resolution has one deliberate security
exception: `FSLITE_TOKEN` wins over `--token-file` / `FSLITE_TOKEN_FILE`,
which win over the persisted credential; there is intentionally no plaintext
`--token` flag. A token supplied through the environment or a token file is a
process-only override and is never written back to the state file.

The quota flags set the byte, node, and regular-file limits when the server
creates or replaces its default workspace. Existing workspace limits are not
retroactively changed. The defaults are 10 GiB total logical bytes, 1,000,000
nodes, and 1 GiB per regular file.

Prefer `FSLITE_TOKEN_FILE` (or `--token-file`) for containers, service
managers, and shared hosts so the bearer value stays out of command lines and
environment inspection. The server trims one token from that file. Its durable
state file also contains the credential: on Unix it is atomically replaced
with mode `0600`. On non-Unix platforms fslite does not set or verify a
platform ACL; run it under a dedicated account and restrict the state file and
its parent directory with that platform's ACL tools. If the credential is
lost, recover the protected state file from backup or run with a replacement
token file/environment value for that process. Do not casually delete
`server.json`: with the same database, its absence creates a new default
workspace and credential while leaving unrelated existing workspaces intact.

Use the authenticated identity endpoint to discover the scoped workspace:

```bash
curl --fail \
  -H "Authorization: Bearer $FSLITE_TOKEN" \
  http://127.0.0.1:8080/v1/me
```

`GET /v1/me` returns only `workspace_id` and `capabilities`; it never returns
the bearer credential or actor metadata. To empty that same workspace, send
`POST /v1/workspaces/{workspace_id}/reset` with its bearer credential. Reset
requires the same workspace plus the `workspace_admin` capability, and
atomically preserves its ID and quotas while deleting non-root nodes, content,
attributes, trash, usage, and change history.

The repository includes a non-root image:

```bash
docker build -f crates/fslite-server/Dockerfile -t fslite-server:local .
```

It defaults to `/data/fslite.db`, `/data/server.json`, and
`0.0.0.0:8080`, with `/data` as its persistent volume. For a web application,
place the server on a Docker-private network, mount the same token file only
into trusted server-side services, and publish the web gateway's port—not the
fslite-server port. Public browsers must call that server-side gateway; never
send them this bearer credential. The image runs as UID/GID `10001` and its
`/readyz` health check is available to private-network peers.

Every verb `fslite-command` understands corresponds one-to-one to a
`FileSystem` method: `usage`, `stat`, `exists`, `ls`, `tree`, `mkdir`, `cat`,
`write`, `write-at`, `append`, `truncate`, `touch`, `cp`, `mv`, `rm`, `ln`,
`readlink`, `trash`, `trash-ls`, `restore`, `purge`, `setattr`, `rmattr`,
`glob`, `find`, `grep`, `changes`, and `batch` — plus `--json` on the CLI for
machine-readable output.

Run `fslite help` to list every verb with a one-line summary, or
`fslite help <verb>` for a verb's full flag table (the same metadata is
also exposed as `reference/cli-verbs` in the docs site).

## Limits

These are the SQLite backend's *defaults*; per-workspace byte/node/file-size
quotas are configurable via `WorkspaceOptions` at `create_workspace` time.

- **Regular file size**: up to the configured `max_file_bytes` (workspace
  default: 1 GiB). Enforced incrementally as bytes stream in, before they're
  flushed to storage — an oversized write fails partway through, not after
  fully buffering the input.
- **Workspace byte/node quotas**: `max_bytes` (workspace default: 10 GiB)
  and `max_nodes` (workspace default: 1,000,000), checked against active
  + trashed logical bytes and node counts.
- **Custom attributes**: a 256-byte key, a 4096-byte value, and 64
  attributes per node. Attribute values are arbitrary bytes; since
  `Node::attributes` is a JSON map, each value is base64url-encoded into a
  JSON string.
- **Symbolic link resolution**: at most 40 hops per path lookup, after which
  resolution fails with `LinkLoop` rather than looping forever.
- **Content chunking**: file content is stored as immutable 1 MiB chunks.
  Reads and writes are streamed chunk-by-chunk in bounded memory — a read
  never buffers more than the chunk currently being produced, regardless of
  file size.

## Transaction guarantees

- Every mutation (`write`, `mkdir`, `copy`, `move`, `remove`, `trash`,
  `restore`, `purge`, attribute changes, ...) commits in one short SQLite
  transaction: the node/content change, its change-feed row, and any
  cascading cleanup (e.g. reclaiming a replaced file's old content
  generation) either all land together or none do.
- **A replacement upload is invisible until it fully commits.** `write`
  stages incoming bytes into a brand-new, independent content generation as
  they stream in. If the source stream errors partway through — or a
  configured quota is exceeded — the partially staged generation is
  discarded and the target's previous content and revision are left exactly
  as they were. Nothing about the existing file is touched until the new
  content is completely staged.
- **`batch` is all-or-nothing.** Every operation in a batch runs against one
  shared transaction, using each operation's internal transaction-level
  logic directly (never by recursively opening another connection call,
  which would deadlock against the single dedicated connection thread). The
  first failing operation aborts the whole batch — nothing commits — and
  the returned error's safe details include `{"index": N}` identifying
  which operation failed.
- Mutations accept an optional expected revision (`expected_revision`) and
  are rejected with `RevisionConflict` if the target has changed since the
  caller last observed it — a standard optimistic-concurrency precondition.

## Workspace isolation

A single SQLite database holds many independent workspaces. Every stored
query is scoped by `workspace_id`, so two workspaces can contain identically
named paths (even at the root) without collision, and no operation in one
workspace can observe or mutate another's nodes, trash, attributes, or
change feed. Pagination cursors are versioned and bound to the workspace
(and, where relevant, the specific parent/root they were issued for) that
produced them; presenting a cursor to a different workspace fails with
`InvalidCursor` rather than silently returning the wrong data.

## Permanent remove vs. recoverable trash

- **`remove`** deletes a node (and, recursively, its entire subtree)
  permanently in one step. There is no way to recover a removed node short
  of restoring a database backup.
- **`trash`** marks a node's own row as trashed and records its original
  location; the node and its descendants become invisible to every
  directory listing and lookup immediately (descendants are never
  individually marked — they simply become unreachable once their parent
  is), but the underlying data is untouched. `list_trash` enumerates
  trashed subtrees, `restore` moves one back to its original location (or
  an alternate destination) if nothing already occupies that name, and
  `purge` permanently deletes a trashed subtree — this is the only way a
  trashed node's content is actually reclaimed.
- The workspace root can never be moved, trashed, or removed.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --example embedded
cargo doc --workspace --no-deps
```

The backend's own test suite (`crates/fslite-sqlite/tests/`) exercises each
operation group in depth; `crates/fslite-conformance` captures the smaller,
backend-agnostic contract every `FileSystem` implementation must satisfy and
is run against `SqliteFileSystem` in `crates/fslite-sqlite/tests/conformance.rs`.

## Status

The canonical `FileSystem` trait and the SQLite backend are complete: all 28
trait methods are implemented and covered by the conformance suite plus the
SQLite backend's own extensive test suite. `fslite-server`, `fslite-command`,
and `fslite` build on this crate's exact public API and are workspace
members with their own test suites (HTTP contract tests for the server;
lexer/parser/sanitizer/executor tests for the command codec; end-to-end
local, remote, and REPL tests for the CLI). They've had several rounds of
security hardening (bidi-override stripping, output sanitization, token
handling) but are newer than `fslite-core`/`fslite-sqlite` and should be
treated as less battle-tested.

# fslite

A transport-independent, async filesystem interface; a persistent,
multi-workspace SQLite implementation suitable for direct embedding; and an
HTTP server, typed command codec, and CLI built on top of it.

## Workspace layout

| Crate | Purpose |
| --- | --- |
| [`fslite-core`](crates/fslite-core) | The canonical `FileSystem` trait, domain types (`VirtualPath`, `Node`, `Revision`, ...), and stable typed errors. Transport-independent: no SQL, no HTTP, no host filesystem paths. |
| [`fslite-sqlite`](crates/fslite-sqlite) | `SqliteFileSystem`: a `FileSystem` implementation backed by one SQLite database that can hold many isolated workspaces. |
| [`fslite-conformance`](crates/fslite-conformance) | A backend-agnostic contract test suite. Any `FileSystem` implementation can prove basic compliance by implementing `ConformanceFactory` and calling `run_conformance`. |
| [`fslite-server`](crates/fslite-server) | An `axum`-based HTTP adapter exposing `FileSystem` as a resource-oriented API: nodes, directories, trash, content (including ranged reads), search, batch, and workspace-admin routes, gated behind a pluggable `AuthProvider`. |
| [`fslite-command`](crates/fslite-command) | A typed `Command` codec (one variant per `FileSystem` operation), a constrained shell-like lexer/parser, and local/remote executors that drive either an in-process `FileSystem` or a running `fslite-server` over HTTP. |
| [`fslite-cli`](crates/fslite-cli) | `fslite-cli`: a command-line client built on `fslite-command`, usable against a local SQLite database, an in-memory database, or a remote `fslite-server`, in one-shot or REPL mode. |

## Quick start

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
| [`server_and_remote_cli`](examples/server_and_remote_cli.rs) | Runs `fslite-server`'s HTTP API in-process and drives it with `fslite-command`'s `RemoteExecutor` — the same client `fslite-cli --server` uses — over a real TCP connection. |

## CLI and server

`fslite-cli` can talk to a local database file, a private in-memory one, or a
remote `fslite-server` over HTTP — the same verb syntax works in all three
modes. Flag values use `--name=value` (the lexer does not treat a following
bare word as a flag's value). `--memory` opens a fresh, unshared database for
that single process only, so it's suited to one-off experiments or scripts
that create a workspace and use it within the same invocation; use
`--db <path>` whenever a workspace needs to be visible across separate
`fslite-cli` invocations:

For everyday interactive use, `fslite` also supports named filesystems and
workspaces, persisted in a small local registry
(`$XDG_CONFIG_HOME/fslite` or `~/.config/fslite`, overridable via
`FSLITE_CONFIG_DIR`) so you don't need to repeat `--db`/`--workspace` on
every invocation:

```bash
fslite create filesystem-main -f fsmain.db -w workspace-main
fslite use filesystem-main -w workspace-main

fslite mkdir /docs
fslite touch /docs/file.md
fslite write /docs/file.md --text=hello
fslite cat /docs/file.md
fslite rm /docs/file.md

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
WORKSPACE=$(cargo run -q -p fslite-cli -- --db ./fslite.db --create-workspace)
cargo run -p fslite-cli -- --db ./fslite.db --workspace "$WORKSPACE" mkdir /docs
cargo run -p fslite-cli -- --db ./fslite.db --workspace "$WORKSPACE" write /docs/hello.txt --text=hi
cargo run -p fslite-cli -- --db ./fslite.db --workspace "$WORKSPACE" ls /

# Local, interactive REPL (the workspace must already exist — create it
# first, as above, or against the same --db file)
cargo run -p fslite-cli -- --db ./fslite.db --workspace "$WORKSPACE" --repl

# Remote, against a running fslite-server
cargo run -p fslite-cli -- --server http://localhost:8080 --workspace <id> \
  --token "$FSLITE_TOKEN" ls /
```

`fslite-server` is a standalone binary; it reads bearer tokens from
`FSLITE_TOKENS` (`token=workspace_uuid` pairs) and serves an in-memory
`SqliteFileSystem` on `0.0.0.0:8080`:

```bash
FSLITE_TOKENS="devtoken=<workspace-uuid>" cargo run -p fslite-server
```

This shipped `main.rs` is reference wiring, not a deployable server as-is: the
backing store is a fresh in-memory database on every start (nothing survives
a restart), and `FSLITE_TOKENS` is read once at startup, so a workspace
created afterward via `POST /v1/workspaces` has no token that can reach it
until the process is restarted with that id added — which then loses the
workspace again. A real deployment needs its own `main` wiring a persistent
`SqliteFileSystem::open(path, ..)` and its own `AuthProvider` that can mint or
look up tokens for workspaces created at runtime.

Every verb `fslite-command` understands corresponds one-to-one to a
`FileSystem` method: `usage`, `stat`, `exists`, `ls`, `tree`, `mkdir`, `cat`,
`write`, `write-at`, `append`, `truncate`, `touch`, `cp`, `mv`, `rm`, `ln`,
`readlink`, `trash`, `trash-ls`, `restore`, `purge`, `setattr`, `rmattr`,
`glob`, `find`, `grep`, `changes`, and `batch` — plus `--json` on the CLI for
machine-readable output.

## Limits

These are the SQLite backend's *defaults*; per-workspace byte/node/file-size
quotas are configurable via `WorkspaceOptions` at `create_workspace` time.

- **Regular file size**: up to the configured `max_file_bytes` (workspace
  default: 1 GiB). Enforced incrementally as bytes stream in, before they're
  flushed to storage — an oversized write fails partway through, not after
  fully buffering the input.
- **Workspace byte/node quotas**: `max_bytes` and `max_nodes`, checked
  against active + trashed logical bytes and node counts.
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
and `fslite-cli` build on this crate's exact public API and are workspace
members with their own test suites (HTTP contract tests for the server;
lexer/parser/sanitizer/executor tests for the command codec; end-to-end
local, remote, and REPL tests for the CLI). They've had several rounds of
security hardening (bidi-override stripping, output sanitization, token
handling) but are newer than `fslite-core`/`fslite-sqlite` and should be
treated as less battle-tested.

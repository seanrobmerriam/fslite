# fslite

A transport-independent, async filesystem interface and a persistent,
multi-workspace SQLite implementation suitable for direct embedding.

## Workspace layout

| Crate | Purpose |
| --- | --- |
| [`fslite-core`](crates/fslite-core) | The canonical `FileSystem` trait, domain types (`VirtualPath`, `Node`, `Revision`, ...), and stable typed errors. Transport-independent: no SQL, no HTTP, no host filesystem paths. |
| [`fslite-sqlite`](crates/fslite-sqlite) | `SqliteFileSystem`: a `FileSystem` implementation backed by one SQLite database that can hold many isolated workspaces. |
| [`fslite-conformance`](crates/fslite-conformance) | A backend-agnostic contract test suite. Any `FileSystem` implementation can prove basic compliance by implementing `ConformanceFactory` and calling `run_conformance`. |

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
version (`cargo run --example embedded`).

## Limits

These are the SQLite backend's defaults; per-workspace byte/node/file-size
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
  caller last observed it; a standard optimistic-concurrency precondition.

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
  individually marked; they simply become unreachable once their parent
  is), but the underlying data is untouched. `list_trash` enumerates
  trashed subtrees, `restore` moves one back to its original location (or
  an alternate destination) if nothing already occupies that name, and
  `purge` permanently deletes a trashed subtree; this is the only way a
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

## Project Status

The `FileSystem` trait and the SQLite backend are complete: all 28
trait methods are implemented and covered by the conformance suite plus the
SQLite backend's test suite.

 `fslite-server` (an HTTP adapter) and `fslite-command`/`fslite-cli` (a typed command codec and CLI) are currently being developed.

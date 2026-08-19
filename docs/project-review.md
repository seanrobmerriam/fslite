# fslite Project Review

## Executive summary

fslite has a strong filesystem engine and unusually broad behavioral coverage
for a 0.1 project. Its main weakness was product entry: users had to understand
the internal filesystem/workspace model before their first CLI operation. The
zero-configuration CLI work closes that P0 gap, aligns the Cargo package with
the `fslite` command, and makes client state crash-safe.

The project is ready for practical local CLI and embedded use. The HTTP server
remains reference wiring rather than a production service, and operational
features such as diagnostics, backups, completions, and packaged binaries are
the best next investments.

## What is working

### Core contract and domain model

The [`FileSystem` trait](../crates/fslite-core/src/fs.rs) defines 28 operations
behind transport-independent, async interfaces. Paths, workspace IDs,
revisions, node types, capabilities, errors, paging, byte ranges, and operation
options are strongly typed. Public API tests in
[`api.rs`](../crates/fslite-core/tests/api.rs) protect object safety, method
inventory, conservative defaults, and capability behavior.

### SQLite backend

[`fslite-sqlite`](../crates/fslite-sqlite/src/lib.rs) implements the complete
trait and persists multiple isolated workspaces in one database. Its tests
cover files, directories, links, attributes, mutations, trash, batches,
search, changes, quotas, security, and workspace isolation. The backend also
runs the reusable contract from
[`fslite-conformance`](../crates/fslite-conformance/src/lib.rs).

Important implementation strengths are:

- streamed, chunked reads and writes rather than whole-file buffering;
- replacement writes that remain invisible until commit;
- atomic mutations and all-or-nothing batches;
- optimistic revision checks;
- byte, node, and file-size quotas;
- recoverable trash with explicit purge;
- workspace-bound cursors and query isolation.

### Command, CLI, and HTTP layers

[`fslite-command`](../crates/fslite-command/src/lib.rs) provides one typed
command model for local and remote execution. Parser and security tests reject
shell metacharacters, expansions, oversized input, malformed flags, and stray
positionals. Rendering tests cover terminal-control and row-forging defenses.

The CLI has end-to-end coverage for local SQLite, named contexts, REPL use,
remote execution, JSON output, and output sanitization. The new
[`e2e_bootstrap.rs`](../crates/fslite-cli/tests/e2e_bootstrap.rs) additionally
proves first-run initialization, clean stdout, explicit-target bypass,
corruption safety, persistence, and concurrent initialization.

The server has route-level and contract coverage for authentication, ranges,
content, nodes, directories, trash, batches, search, workspaces, tracing,
health, readiness, and consistent JSON errors.

### Documentation and examples

The root README explains storage limits, transaction guarantees, isolation,
trash semantics, CLI/server modes, and development gates. Six runnable
examples cover embedding, batches, trash, isolation, search, and remote HTTP
execution.

## What was missing or unfriendly

### P0 usability gaps addressed by this change

- A new user could not run a filesystem verb without first creating and
  selecting both a database and workspace.
- The published package name `fslite-cli` differed from the installed command
  `fslite`.
- The README led with library embedding instead of the shortest user journey.
- Registry and context JSON were written directly and could be truncated by an
  interrupted process.
- First-run creation had no cross-process coordination.

The current change addresses each item: `cargo install fslite`, implicit
`default/default` creation, a CLI-first quick start, atomic JSON replacement,
and a locked/rechecked bootstrap path.

### Remaining product gaps

- Users cannot ask which database/workspace is active or where it lives.
- There is no single diagnostics command for registry, context, database,
  migration, permission, and integrity checks.
- Host-file and stdin import/export workflows are less direct than ordinary
  filesystem tools.
- There are no generated shell completions or man pages.
- Installation requires a Rust toolchain; release binaries and platform
  packages are absent.
- Backup, restore, integrity-check, and compaction workflows are not exposed as
  user-facing commands.
- The shipped server uses an in-memory database and startup-only token mapping,
  so it is explicitly not a deployable persistent service.

## Recommended additions

### P1: user visibility and daily ergonomics

1. Add `fslite status` to show the active filesystem/workspace, database path,
   workspace ID, usage, and whether the selection was explicit or persisted.
2. Add `fslite doctor` to validate JSON state, paths, SQLite migrations,
   workspace existence, write permissions, and database integrity without
   mutating user data.
3. Add shell completions and man-page generation from the Clap and verb-help
   metadata.
4. Add direct stdin and host-file import/export forms with unambiguous binary
   handling and pipeline-safe output.
5. Add backup/export and restore/import commands, including an integrity check
   and documented consistency guarantees.
6. Publish signed prebuilt binaries for major targets, while retaining
   `cargo install fslite` as the source-install path.

### Later: operations and deployment

1. Provide a production server binary with persistent database configuration,
   runtime workspace/token management, TLS/proxy guidance, graceful shutdown,
   and operational defaults.
2. Add structured health and maintenance commands for migrations, SQLite
   integrity, WAL/checkpoint state, and compaction.
3. Define database-format migration/version diagnostics and backup compatibility
   guarantees before the storage format becomes broadly deployed.
4. Add Homebrew, WinGet/Scoop, and selected Linux packaging after release
   automation and signing are established.

## Suggested delivery order

Implement `status` and `doctor` first because they make every support and
recovery workflow easier. Follow with import/export and backup, then completion
and binary distribution. Treat the production server as a separate design and
implementation project: its authentication and operational choices are larger
than a CLI usability increment and deserve independent review.

## Verification posture

The repository's existing release gates are appropriate: formatting, Clippy
with warnings denied, the complete all-features workspace suite, package
dry-run, and runnable examples. Release work should additionally install the
packaged CLI into an isolated Cargo root and verify the first-command journey
with isolated config/data directories.

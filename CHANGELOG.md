# Changelog

All notable changes to the `fslite` workspace are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the 0.x carve-out described in `SEMVER.md`.

## [0.2.0] - 2026-08-19

### Added

- **fslite-sqlite**: `SqliteFileSystem::reset_workspace` atomically returns a
  workspace to an empty-root state without changing its identity or quota
  configuration. It removes active and trashed content, attributes, usage,
  and change history together, or rolls the whole reset back.
- **fslite-server**: the installed binary now bootstraps a persistent SQLite
  database and default workspace with a durable scoped credential; it exposes
  safe authenticated identity through `GET /v1/me` and an authorized
  same-workspace reset endpoint.
- **fslite-server**: a non-root, multi-stage Docker image persists `/data`,
  supports secret-file credentials, and defaults to private-network binding on
  `0.0.0.0:8080`.

### Security

- **fslite-server**: generated credentials are saved atomically and with
  owner-only mode on Unix, never appear in debug output, and are printed only
  in first-run connection guidance. `FSLITE_TOKEN` and token-file overrides
  are process-local and do not overwrite the stored credential.
- **fslite-server**: no plaintext `--token` argument is accepted. Deployments
  should use `FSLITE_TOKEN_FILE` and keep bearer credentials out of browsers,
  shell history, command lines, and untrusted logs.

### Changed

- **fslite-sqlite**, **fslite-server**, and **fslite** are prepared as
  `0.2.0`. Under this repository's pre-1.0 policy, the additive SQLite reset
  API requires the minor release; the server depends on it, and the CLI is
  rebuilt with the updated SQLite dependency requirement.

### Release order

Publish `fslite-sqlite 0.2.0`, wait for crates.io indexing, then publish
`fslite-server 0.2.0`; publish `fslite 0.2.0` when releasing its manifest with
the new SQLite dependency. The package preparation in this change does not
publish, tag, push, or deploy anything.

## [0.1.1] - 2026-08-19

### Changed
- **fslite-command**: CLI virtual-path operands now accept either canonical
  absolute paths or workspace-root-relative paths. Relative glob patterns are
  rooted and normalized the same way.
- **fslite**: quick-start examples and help now use the shorter
  `mkdir docs` / `write docs/file` workflow. Existing absolute commands remain
  compatible, and relative paths always resolve from the workspace root.

### Notes
- `fslite-core::VirtualPath` and serialized command paths remain strictly
  absolute; relative-path handling is confined to the CLI parser boundary.

## [0.1.0] - YYYY-MM-DD

### Added
- **fslite-core**: canonical `FileSystem` trait (28 methods), domain types
  (`VirtualPath`, `LinkTarget`, `Node`, `Revision`, `Capability`,
  `RequestContext`, `WorkspaceUsage`, `TrashEntry`, `Change`, `ChangeKind`,
  `ChangeCursor`, `SearchMatch`, `ByteRange`, `Page`, `TreeEntry`,
  `BatchOperation`, `BatchResult`, `WriteSource`, `FileRead`, all `*Options`
  builders), stable typed `FsError` / `ErrorCode`.
- **fslite-sqlite**: `SqliteFileSystem` implementing every `FileSystem`
  method; multi-workspace isolation; per-workspace `WorkspaceOptions`
  (default `max_bytes = 10 GiB`, `max_nodes = 1_000_000`,
  `max_file_bytes = 1 GiB`); chunked content (1 MiB); recoverable trash;
  per-workspace change feed; cursor-paginated listing/tree/search;
  bounded work budget for glob/find/search_content (5000 rows per call).
- **fslite-conformance**: `ConformanceFactory` trait + `run_conformance`
  driving 11 case groups (paths, directories, files, mutations, links,
  trash, attributes, batches, search, changes, security) against any
  `FileSystem` implementation.
- **fslite-server**: `axum` HTTP adapter exposing `FileSystem` as a
  resource-oriented API (`nodes`, `directories`, `trash`, `content`,
  `search`, `batch`, `workspaces`); pluggable `AuthProvider`; reference
  `BearerTokenAuthProvider`; `range::resolve_range` for HTTP `Range:`
  header parsing; per-request `RequestId` correlation.
- **fslite-command**: typed `Command` codec (one variant per
  `FileSystem` method); shell-like lexer/parser with no shell expansion;
  `LocalExecutor` (in-process) and `RemoteExecutor` (HTTP) sharing the
  `Executor` trait; three-tier terminal-output sanitizer
  (`sanitize_name`, `sanitize_for_terminal`, `sanitize_preview`).
- **fslite**: `fslite` binary with `--db`/`--memory`/`--server`
  modes, `create`/`delete`/`use` subcommands for the local
  filesystem/workspace registry, REPL mode, `--json` output, per-verb
  help (`fslite help [<verb>]`); package renamed from `fslite-cli` so
  installation is `cargo install fslite`; first un-targeted filesystem command
  creates a persistent `default` database/workspace; registry and context JSON
  updates are atomic and concurrent initialization is serialized.
- Six runnable examples: `embedded`, `batch`, `trash_lifecycle`,
  `workspace_isolation`, `search_and_glob`, `server_and_remote_cli`.

### Notes
- This is the first public release. See `SEMVER.md` for the 0.x
  stability commitment.
- The original `fslite-server` binary was non-persistent. The `0.2.0` release
  above replaces that limitation with durable bootstrap behavior.

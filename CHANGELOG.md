# Changelog

All notable changes to the `fslite` workspace are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
with the 0.x carve-out described in `SEMVER.md`.

## [0.1.0] - 2026-08-02

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
- **fslite-cli**: `fslite` binary with `--db`/`--memory`/`--server`
  modes, `create`/`delete`/`use` subcommands for the local
  filesystem/workspace registry, REPL mode, `--json` output, per-verb
  help (`fslite help [<verb>]`).
- Six runnable examples: `embedded`, `batch`, `trash_lifecycle`,
  `workspace_isolation`, `search_and_glob`, `server_and_remote_cli`.

### Notes
- This is the first public release. See `SEMVER.md` for the 0.x
  stability commitment.
- `fslite-server`'s shipped `main.rs` is reference wiring only; it
  opens an in-memory database on every start and reads `FSLITE_TOKENS`
  once at startup. A real deployment needs its own `main` (see the
  [Auth provider guide](https://docs.fslite.rusty.yachts/guides/auth-provider)
  on the project docs site).

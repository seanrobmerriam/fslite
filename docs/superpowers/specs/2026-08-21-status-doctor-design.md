# `fslite status` and `fslite doctor` Design

## Purpose

New users and support workflows currently have no way to ask "what is
fslite's CLI pointing at right now?" or "is my local fslite state healthy?"
without reading `registry.json`/`context.json` by hand or reasoning about
SQLite internals. `docs/project-review.md` names `status` and `doctor` as the
top P1 usability gap: they make every other support and recovery workflow
easier, so they are implemented before import/export, backup, or completions.

- `fslite status` answers "what filesystem/workspace is active, where does
  its data live, and how did the CLI pick it?"
- `fslite doctor` answers "is my local fslite state (registry, context, every
  registered database) internally consistent and healthy?"

Both commands are strictly read-only: neither mutates `registry.json`,
`context.json`, the bootstrap lock, or any database.

## Scope

In scope:

- A new `fslite status` subcommand reporting the active filesystem/workspace.
- A new `fslite doctor` subcommand validating registry, context, and every
  registered filesystem's database.
- Two new public methods on `fslite-sqlite`'s `SqliteFileSystem`:
  `schema_version()` and `integrity_check()`.
- `--json` output for both commands.

Out of scope (left for later P1/P2 work per `docs/project-review.md`):

- Repairing problems `doctor` finds (no `--fix`; it only reports).
- Import/export, backup/restore, shell completions.
- Remote/server-side status or health endpoints — both commands report on
  local CLI client state (registry/context files, local DB paths) that has
  no server-side equivalent.

## Architecture

`status` and `doctor` are CLI-only concerns, not `fslite-command` verbs.
`fslite-command::Command` models one operation against one already-resolved
workspace; it has no concept of the registry, multiple filesystems, or local
config files. Both commands are added as new `Action` variants in
`crates/fslite-cli/src/cli.rs` (alongside the existing `Create`/`Delete`/
`Use`/`Help` variants) and dispatched early in `main.rs`, before the
bootstrap/target-resolution block — the same way `Help` is special-cased
today — so neither command triggers bootstrap or requires a resolved target.

Implementation lives in two new modules:

- `crates/fslite-cli/src/status.rs`
- `crates/fslite-cli/src/doctor.rs`

Both read `Registry::load()`, `Context::load()`, and `paths::config_dir()` /
`paths::data_dir()` directly, mirroring how `bootstrap.rs` and `main.rs`
already use them.

### `fslite-sqlite` additions

`doctor` needs to open and check every registered database independently,
not just the currently active connection. This requires promoting two
pieces of existing private logic to public API on `SqliteFileSystem`:

```rust
pub async fn schema_version(&self) -> Result<i64, FsError>;
pub async fn integrity_check(&self) -> Result<Vec<String>, FsError>;
```

`schema_version` exposes the existing `read_current_version` query.
`integrity_check` runs SQLite's `PRAGMA integrity_check` and returns the
problem rows (an empty vec means healthy). The latest known migration
version (already computed privately as `latest_schema_version()`) is also
exposed so `doctor` can compare current-vs-expected without duplicating the
migration table.

## `fslite status`

Never bootstraps, never mutates state. Determines the target the same way
`main.rs::resolve_target` does today:

- `--filesystem`/`--workspace` passed → **explicit**.
- Otherwise `Context::load()` has a value → **persisted**.
- Otherwise → **none configured yet**.

Human-readable output:

```
Filesystem: default (persisted from context.json)
Database:   /home/sean/.local/share/fslite/fslite.db
Workspace:  default (ws_01h...)
Usage:      1,204 nodes, 3.2 MB active, 0 B trashed (limit: 1,000,000 nodes / 10 GiB)
Selection:  persisted (context.json)
```

If nothing is configured yet (no context, never bootstrapped):

```
No active filesystem yet — run any command (e.g. `fslite mkdir docs`) to
bootstrap the default workspace.
```

This is not an error (exit 0).

`--json` output is an object with `filesystem_name`, `database_path`,
`workspace_name`, `workspace_id`, `usage` (mirroring `WorkspaceUsage`
fields), and `selection: "explicit" | "persisted" | "none"`.

If `registry.json` or `context.json` fails to parse, or the resolved
database can't be opened, `status` does not crash: it prints a short
diagnosis (`context.json is corrupt: <reason>. Run \`fslite doctor\` for
details.`) and exits non-zero, deferring deep diagnosis to `doctor`.

## `fslite doctor`

Runs a fixed sequence of read-only checks, each reported as
`pass` / `warn` / `fail`:

1. **`registry.json`** readable and parses as a valid `Registry`. Missing is
   `pass` (expected pre-bootstrap), not a failure.
2. **`context.json`** readable and parses as a valid `Context`; if it names a
   filesystem/workspace, that name exists in the registry.
3. **`bootstrap.lock`** is not stuck: attempt a non-blocking
   `try_lock_exclusive` and immediately release it. This only probes lock
   state — it does not alter the lock file's contents. Failure to acquire is
   `warn`, not `fail` (a concurrent fslite process may legitimately hold it).
4. **Per registered filesystem** (`registry.filesystems`): DB file exists at
   its path; opens successfully; `schema_version()` matches the latest known
   migration; `integrity_check()` returns no problems; the DB file and its
   parent directory are writable (checked via file metadata permissions, not
   an actual write).
5. **Per registered workspace** (`registry.workspaces`): the workspace ID
   exists in its filesystem's database, checked via the existing
   `FileSystem::workspace_usage` call — a not-found error is a failed check,
   not a crash.

Human-readable output is a checklist grouped by filesystem:

```
✓ registry.json is valid
✓ context.json is valid
✓ bootstrap.lock is not stuck
✗ filesystem "default": schema version 1, expected 2 — run a newer fslite or restore a backup
✓ filesystem "default": integrity check passed
✓ filesystem "default": database and directory are writable
✓ workspace "default" exists in filesystem "default"

1 problem found.
```

`--json` output is an array of
`{ check, scope, status: "pass" | "warn" | "fail", detail }` objects.

**Exit code:** `0` if every check is `pass` or `warn`; `1` if any check is
`fail`. This lets scripts use `fslite doctor && echo ok`.

## Errors and Compatibility

Neither command changes any existing verb, the parser, the command codec, or
wire format. Both are purely additive `Action` variants. Existing bootstrap,
registry, and context behavior is unchanged; `doctor` only reads the same
files that behavior already writes.

## Testing

- `crates/fslite-sqlite/`: unit tests for `schema_version()` on a fresh DB
  (returns the latest version) and `integrity_check()` on a healthy DB
  (returns empty), plus a corrupted-DB fixture (e.g. a truncated file)
  returning non-empty problems.
- `crates/fslite-cli/tests/e2e_status.rs` (new): status before bootstrap (no
  active-filesystem message, exit 0), status after bootstrap with explicit
  flags vs. persisted context, status after corrupting `context.json` (error
  path, non-zero exit).
- `crates/fslite-cli/tests/e2e_doctor.rs` (new): a clean registry (exit 0,
  all pass); a corrupted `context.json` (fail); a missing DB file for a
  registered filesystem (fail); a stale workspace entry in the registry not
  present in its DB (fail); and the `--json` output shape.
- Both new e2e test files reuse the existing `Fixture` pattern from
  `crates/fslite-cli/tests/e2e_bootstrap.rs` (isolated `FSLITE_CONFIG_DIR`/
  `FSLITE_DATA_DIR` per test).

## Acceptance Criteria

- `fslite status` reports the active filesystem, database path, workspace ID,
  usage, and whether selection was explicit or persisted, without bootstrapping
  or mutating any state.
- `fslite status` before any bootstrap reports "no active filesystem" as a
  non-error.
- `fslite doctor` validates registry.json, context.json, the bootstrap lock,
  and every registered filesystem's database (existence, schema version,
  integrity, writability) and every registered workspace's existence.
- `fslite doctor` exits non-zero if and only if at least one check fails.
- Both commands support `--json` output matching the documented shapes.
- Neither command mutates `registry.json`, `context.json`, the bootstrap
  lock, or any database under any circumstances.
- Focused tests, the full workspace test suite, formatting, and Clippy pass.
